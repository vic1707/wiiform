// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};
use wiiform::{
    native::{Nand, Session},
    priiloader_payload,
    recipe::{CiosTarget, Priiloader, ResolvedPath, Sha256, TitleId, Wad},
    workflow::{NusCache, publish_edit, validate_output, warn_if_already_present},
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
    /// Runs one edit on a copy of the input and publishes it only if the edit succeeds.
    fn edit(self, operation: impl FnOnce(&Session, &mut Nand) -> Result<()>) -> Result<()> {
        validate_output(&self.output)?;
        let session = Session::acquire()?;
        let mut nand = Nand::read(self.input.as_ref())?;
        operation(&session, &mut nand)?;
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
        /// Local WAD path or HTTPS URL.
        #[arg(value_name = "WAD")]
        wad: PathBuf,
        #[arg(long)]
        sha256: Sha256,
        /// Permit an unsigned community channel after content validation.
        #[arg(long)]
        allow_unsigned: bool,
    },
    /// Install a prebuilt, hash-pinned Wii d2x cIOS WAD.
    InstallCios {
        #[command(flatten)]
        image: ImagePaths,
        /// Local WAD path or HTTPS URL.
        #[arg(value_name = "WAD")]
        wad: PathBuf,
        #[arg(long)]
        sha256: Sha256,
        #[command(flatten)]
        target: CiosTarget,
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
        config: Priiloader,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    wiiform::cli::run("wii-nand", || run(cli))
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::InstallTitle {
            image,
            title_id,
            version,
        } => {
            let cache = NusCache::from_environment()?;
            image.edit(|session, nand| {
                cache.prepare()?;
                session.install_nus_title(nand, &cache, title_id, version)
            })
        }
        Command::InstallWad {
            image,
            wad,
            sha256,
            allow_unsigned,
        } => {
            let wad = Wad::try_from((wad, sha256))?;
            image.edit(|session, nand| {
                let installed = if allow_unsigned {
                    session.install_unsigned_channel(nand, &wad)
                } else {
                    session.install_wad(nand, &wad)
                }?;
                warn_if_already_present(&installed);
                Ok(())
            })
        }
        Command::InstallCios {
            image,
            wad,
            sha256,
            target,
        } => {
            let wad = Wad::try_from((wad, sha256))?;
            image.edit(|session, nand| {
                warn_if_already_present(&session.install_cios(nand, &wad, &target)?);
                Ok(())
            })
        }
        Command::RemoveTitle { image, title_id } => {
            image.edit(|session, nand| session.remove_title(nand, title_id))
        }
        Command::InstallPriiloader { image, config } => {
            let (app, definitions) = priiloader_payload::load()?;
            image.edit(|session, nand| {
                session.install_priiloader(nand, &app)?;
                session.configure_priiloader(nand, &config)?;
                session.set_priiloader_hacks(nand, &definitions, &config.hacks)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiiform::recipe::{Autoboot, Hack};

    #[test]
    fn priiloader_cli_uses_defaults_and_accepts_named_flags() {
        let parse = |args: &[&str]| {
            let input = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
            let fixed = ["wii-nand", "install-priiloader", input, "output"];
            let cli = Cli::try_parse_from(fixed.into_iter().chain(args.iter().copied())).unwrap();
            let Command::InstallPriiloader { config, .. } = cli.command else {
                unreachable!()
            };
            config
        };
        assert_eq!(parse(&[]), Priiloader::default());
        assert_eq!(
            parse(&[
                "--white-background",
                "--autoboot=homebrew_channel",
                "--hack=fix_480p",
                "--hack=wiimmfi"
            ]),
            Priiloader {
                white_background: true,
                autoboot: Autoboot::HomebrewChannel,
                hacks: vec![Hack::Fix480p, Hack::Wiimmfi],
                ..Priiloader::default()
            }
        );
    }
}
