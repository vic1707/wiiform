// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use wiiform::{
    native::{self, CacheMode, WadPolicy},
    recipe::{
        Autoboot, HackSettings, MountPoint, PriiloaderSettings, ReturnTo, Sha256, ShutdownTo,
        TitleId,
    },
    workflow::{publish_edit, read_bounded, validate_output},
};

#[derive(Parser)]
#[command(name = "wii-nand", version, about = "Edit a Wii NAND into a new file")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct ImagePaths {
    /// Existing footer-bearing NAND.
    input: PathBuf,
    /// New path for the edited NAND.
    output: PathBuf,
}

impl ImagePaths {
    fn edit(self, operation: impl FnOnce(&mut [u8]) -> Result<()>) -> Result<()> {
        validate_output(&self.output)?;
        let mut nand = read_bounded(&self.input, native::NAND_BYTES, native::NAND_BYTES)?;
        operation(&mut nand)?;
        publish_edit(&nand, &self.output)
    }
}

#[derive(Subcommand)]
enum Command {
    /// Install one exact Nintendo title version.
    InstallTitle {
        #[command(flatten)]
        image: ImagePaths,
        cache: PathBuf,
        title_id: TitleId,
        #[arg(value_parser = clap::value_parser!(u16).range(..65535))]
        version: u16,
        #[arg(long)]
        offline: bool,
    },
    /// Install a WAD after verifying its SHA-256.
    InstallWad {
        #[command(flatten)]
        image: ImagePaths,
        wad: PathBuf,
        #[arg(long)]
        sha256: Sha256,
        #[arg(long)]
        allow_unverified_signatures: bool,
    },
    /// Remove one allowlisted channel by title ID.
    RemoveTitle {
        #[command(flatten)]
        image: ImagePaths,
        title_id: TitleId,
    },
    /// Install or configure the pinned Priiloader release.
    InstallPriiloader {
        #[command(flatten)]
        image: ImagePaths,
        /// Extracted Wii Priiloader 0.10.0 app, required for a fresh install.
        #[arg(long)]
        app: Option<PathBuf>,
        /// Pinned release hacks_hash.ini.
        #[arg(long)]
        hack_definitions: Option<PathBuf>,
        #[command(flatten)]
        settings: PriiloaderArgs,
        #[arg(long = "hack", value_parser = parse_hack, value_name = "KEY=on|off")]
        hacks: Vec<HackOverride>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

impl OnOff {
    fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

#[derive(Args)]
struct PriiloaderArgs {
    #[arg(long, value_parser = ["disabled", "homebrew_channel", "bootmii_ios", "system_menu", "file"])]
    autoboot: Option<String>,
    #[arg(long, value_parser = ["system_menu", "priiloader", "autoboot", "file"])]
    return_to: Option<String>,
    #[arg(long, value_parser = ["none", "priiloader", "autoboot"])]
    shutdown_to: Option<String>,
    #[arg(long, value_parser = ["auto", "sd", "usb"])]
    preferred_mount_point: Option<String>,
    #[arg(long, value_enum)]
    stop_disc: Option<OnOff>,
    #[arg(long, value_enum)]
    lid_slot_on_error: Option<OnOff>,
    #[arg(long, value_enum)]
    ignore_shutdown_mode: Option<OnOff>,
    #[arg(long, value_enum)]
    black_background: Option<OnOff>,
    #[arg(long, value_enum)]
    dump_gecko_text: Option<OnOff>,
    #[arg(long, value_enum)]
    show_rc_updates: Option<OnOff>,
}

impl From<PriiloaderArgs> for PriiloaderSettings {
    fn from(args: PriiloaderArgs) -> Self {
        Self {
            autoboot: args.autoboot.as_deref().map(|name| match name {
                "disabled" => Autoboot::Disabled,
                "homebrew_channel" => Autoboot::HomebrewChannel,
                "bootmii_ios" => Autoboot::BootmiiIos,
                "system_menu" => Autoboot::SystemMenu,
                "file" => Autoboot::File,
                _ => unreachable!("validated by clap"),
            }),
            return_to: args.return_to.as_deref().map(|name| match name {
                "system_menu" => ReturnTo::SystemMenu,
                "priiloader" => ReturnTo::Priiloader,
                "autoboot" => ReturnTo::Autoboot,
                "file" => ReturnTo::File,
                _ => unreachable!("validated by clap"),
            }),
            shutdown_to: args.shutdown_to.as_deref().map(|name| match name {
                "none" => ShutdownTo::None,
                "priiloader" => ShutdownTo::Priiloader,
                "autoboot" => ShutdownTo::Autoboot,
                _ => unreachable!("validated by clap"),
            }),
            preferred_mount_point: args
                .preferred_mount_point
                .as_deref()
                .map(|name| match name {
                    "auto" => MountPoint::Auto,
                    "sd" => MountPoint::Sd,
                    "usb" => MountPoint::Usb,
                    _ => unreachable!("validated by clap"),
                }),
            stop_disc: args.stop_disc.map(OnOff::enabled),
            lid_slot_on_error: args.lid_slot_on_error.map(OnOff::enabled),
            ignore_shutdown_mode: args.ignore_shutdown_mode.map(OnOff::enabled),
            black_background: args.black_background.map(OnOff::enabled),
            dump_gecko_text: args.dump_gecko_text.map(OnOff::enabled),
            show_rc_updates: args.show_rc_updates.map(OnOff::enabled),
        }
    }
}

#[derive(Clone)]
struct HackOverride {
    name: String,
    enabled: bool,
}

fn parse_hack(value: &str) -> Result<HackOverride> {
    let (name, state) = value.split_once('=').context("expected KEY=on|off")?;
    let enabled = match state {
        "on" => true,
        "off" => false,
        _ => bail!("expected KEY=on|off"),
    };
    Ok(HackOverride {
        name: name.to_owned(),
        enabled,
    })
}

fn hack_settings(overrides: Vec<HackOverride>) -> Result<HackSettings> {
    let mut hacks = HackSettings::default();
    for HackOverride { name, enabled } in overrides {
        let field = match name.as_str() {
            "fix_480p" => &mut hacks.fix_480p,
            "block_online_updates" => &mut hacks.block_online_updates,
            "block_disc_updates" => &mut hacks.block_disc_updates,
            "block_disc_autoboot" => &mut hacks.block_disc_autoboot,
            "auto_press_a" => &mut hacks.auto_press_a,
            "remove_deflicker" => &mut hacks.remove_deflicker,
            "move_disc_channel" => &mut hacks.move_disc_channel,
            "remove_nocopy_save_protection" => &mut hacks.remove_nocopy_save_protection,
            "replace_health_screen" => &mut hacks.replace_health_screen,
            "no_background_music" => &mut hacks.no_background_music,
            "wiimmfi" => &mut hacks.wiimmfi,
            "region_free_everything" => &mut hacks.region_free_everything,
            _ => bail!("unknown Priiloader hack {name}"),
        };
        ensure!(
            field.replace(enabled).is_none(),
            "repeated Priiloader hack {name}"
        );
    }
    Ok(hacks)
}

fn optional_bytes(path: Option<&Path>, limit: usize) -> Result<Vec<u8>> {
    path.map(|path| read_bounded(path, 1, limit))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::InstallTitle {
            image,
            cache,
            title_id,
            version,
            offline,
        } => {
            let mode = if offline {
                CacheMode::Offline
            } else {
                CacheMode::DownloadMissing
            };
            if !offline {
                std::fs::create_dir_all(&cache).context("cannot create title cache")?;
            }
            image.edit(|nand| {
                native::install_nus_title(nand, &cache, title_id.as_u64(), version, mode)
            })
        }
        Command::InstallWad {
            image,
            wad,
            sha256,
            allow_unverified_signatures,
        } => {
            let policy = if allow_unverified_signatures {
                WadPolicy::LegacyChannel
            } else {
                WadPolicy::Standard
            };
            image.edit(|nand| {
                let bytes = read_bounded(&wad, 0x40, native::WAD_MAX_BYTES)?;
                native::install_wad(nand, &bytes, &sha256, policy)?;
                Ok(())
            })
        }
        Command::RemoveTitle { image, title_id } => {
            image.edit(|nand| native::remove_title(nand, title_id.as_u64()))
        }
        Command::InstallPriiloader {
            image,
            app,
            hack_definitions,
            settings,
            hacks,
        } => {
            let hacks = if hacks.is_empty() {
                None
            } else {
                Some(hack_settings(hacks)?)
            };
            image.edit(|nand| {
                let app = optional_bytes(app.as_deref(), 747_968)?;
                let definitions = optional_bytes(hack_definitions.as_deref(), 64 * 1024)?;
                native::apply_priiloader(nand, &app, &definitions, &settings.into(), hacks.as_ref())
            })
        }
    }
}
