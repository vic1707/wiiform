// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use wiiform::{
    native, priiloader_payload,
    recipe::{
        Data, HackSettings, PriiloaderSettings, ResolvedPath, Sha256, SignaturePolicy, Title,
        TitleId, read_bounded,
    },
    workflow::{nus_environment, prepare_nus_cache, publish_edit, validate_output},
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
    input: ResolvedPath,
    /// New path for the edited NAND.
    output: PathBuf,
}

impl ImagePaths {
    fn edit(self, operation: impl FnOnce(&mut [u8]) -> Result<()>) -> Result<()> {
        validate_output(&self.output)?;
        let mut nand = read_bounded(self.input.as_ref(), native::NAND_BYTES..=native::NAND_BYTES)?;
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
        title_id: TitleId,
        #[arg(value_parser = clap::value_parser!(u16).range(..65535))]
        version: u16,
    },
    /// Install a WAD after verifying its SHA-256.
    InstallWad {
        #[command(flatten)]
        image: ImagePaths,
        wad: ResolvedPath,
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
        #[command(flatten)]
        settings: PriiloaderSettings,
        #[command(flatten)]
        hacks: HackSettings,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::InstallTitle {
            image,
            title_id,
            version,
        } => {
            let (cache, offline) = nus_environment()?;
            image.edit(|nand| {
                prepare_nus_cache(&cache, offline)?;
                native::install_nus_title(nand, &cache, title_id.as_u64(), version, offline)
            })
        }
        Command::InstallWad {
            image,
            wad,
            sha256,
            allow_unverified_signatures,
        } => {
            let title = Title {
                source: Data::Path(wad.as_ref().to_owned()),
                sha256,
                signature_policy: if allow_unverified_signatures {
                    SignaturePolicy::AllowUnsigned
                } else {
                    SignaturePolicy::Nintendo
                },
            };
            image.edit(|nand| {
                native::install_wad(nand, &title)?.warn_if_already_present();
                Ok(())
            })
        }
        Command::RemoveTitle { image, title_id } => {
            image.edit(|nand| native::remove_title(nand, title_id.as_u64()))
        }
        Command::InstallPriiloader {
            image,
            settings,
            hacks,
        } => {
            let (app, definitions) = priiloader_payload::load()?;
            image.edit(|nand| native::apply_priiloader(nand, &app, &definitions, &settings, &hacks))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priiloader_cli_uses_defaults_and_accepts_named_flags() {
        let parse = |args: &[&str]| {
            let cli = Cli::try_parse_from(
                [
                    "wii-nand",
                    "install-priiloader",
                    concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
                    "output",
                ]
                .into_iter()
                .chain(args.iter().copied()),
            )
            .unwrap();
            let Command::InstallPriiloader {
                settings, hacks, ..
            } = cli.command
            else {
                unreachable!()
            };
            (settings, hacks)
        };
        let (settings, hacks) = parse(&[]);
        assert_eq!(settings, PriiloaderSettings::default());
        assert_eq!(hacks, HackSettings::default());

        let (settings, hacks) = parse(&["--white-background", "--stop-disc", "--fix-480p"]);
        assert!(!settings.black_background);
        assert!(settings.stop_disc);
        assert_eq!(
            hacks,
            HackSettings {
                fix_480p: true,
                ..HackSettings::default()
            }
        );
    }
}
