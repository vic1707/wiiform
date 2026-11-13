// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use wiiform::{
    native::{self, CacheMode, WadPolicy},
    recipe::{HackSettings, PriiloaderHack, PriiloaderSettings, Sha256, TitleId, parse_on_off},
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
        settings: PriiloaderSettings,
        #[arg(long = "hack", value_parser = parse_hack, value_name = "KEY=on|off")]
        hacks: Vec<HackOverride>,
    },
}

#[derive(Clone)]
struct HackOverride {
    hack: PriiloaderHack,
    enabled: bool,
}

fn parse_hack(value: &str) -> Result<HackOverride> {
    let (key, state) = value.split_once('=').context("expected KEY=on|off")?;
    let hack = PriiloaderHack::from_str(key, false).map_err(anyhow::Error::msg)?;
    let enabled = parse_on_off(state).map_err(anyhow::Error::msg)?;
    Ok(HackOverride { hack, enabled })
}

fn hack_settings(overrides: Vec<HackOverride>) -> Result<HackSettings> {
    let mut hacks = HackSettings::default();
    for HackOverride { hack, enabled } in overrides {
        ensure!(
            hacks.set(hack, enabled),
            "repeated Priiloader hack {}",
            hack.as_str()
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
                native::apply_priiloader(nand, &app, &definitions, &settings, hacks.as_ref())
            })
        }
    }
}
