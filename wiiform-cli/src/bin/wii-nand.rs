// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};
use wiiform::{
    native::{Nand, Session, WadKind},
    priiloader_payload,
    recipe::{
        Cios, CiosOptions, CiosTarget, HackSettings, PriiloaderSettings, ResolvedPath, Sha256,
        Title, TitleId, TitleOptions,
    },
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
        #[arg(long)]
        allow_unverified_signatures: bool,
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
        settings: PriiloaderSettings,
        #[command(flatten)]
        hacks: HackSettings,
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
            allow_unverified_signatures,
        } => {
            let kind = if allow_unverified_signatures {
                WadKind::LegacyChannel
            } else {
                WadKind::Standard
            };
            let title = Title::from_argument(wad, sha256, TitleOptions::default())?;
            let (wad, sha256) = title.resolve(wiiform::fetch)?;
            image.edit(|session, nand| {
                warn_if_already_present(&session.install_wad(nand, &wad, sha256, kind)?);
                Ok(())
            })
        }
        Command::InstallCios {
            image,
            wad,
            sha256,
            target,
        } => {
            let cios = Cios::from_argument(wad, sha256, CiosOptions { target })?;
            let (wad, sha256) = cios.resolve(wiiform::fetch)?;
            image.edit(|session, nand| {
                let installed = session.install_cios(nand, &wad, sha256, &cios.options.target)?;
                warn_if_already_present(&installed);
                Ok(())
            })
        }
        Command::RemoveTitle { image, title_id } => {
            image.edit(|session, nand| session.remove_title(nand, title_id))
        }
        Command::InstallPriiloader {
            image,
            settings,
            hacks,
        } => {
            let (app, definitions) = priiloader_payload::load()?;
            image.edit(|session, nand| {
                session.install_priiloader(nand, &app)?;
                session.configure_priiloader(nand, &settings)?;
                session.set_priiloader_hacks(nand, &definitions, &hacks)
            })
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
