// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};
use wiiform::{
    native::Nand,
    recipe::{
        Priiloader, ResolvedPath, Rvloader, Sha256, TitleId, Wad,
        nand::{rvloader, titles::cios},
    },
    workflow::{
        NusCache, install_priiloader, publish_edit, validate_output, warn_if_already_present,
    },
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
    fn edit(self, operation: impl FnOnce(&mut Nand) -> Result<()>) -> Result<()> {
        validate_output(&self.output)?;
        let mut nand = Nand::read(self.input.as_ref())?;
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
        /// Local WAD path or HTTPS URL.
        #[arg(value_name = "WAD")]
        wad: PathBuf,
        #[arg(long)]
        sha256: Sha256,
        /// Permit an unsigned community channel after content validation.
        #[arg(long)]
        allow_unsigned: bool,
    },
    /// Install a d2x cIOS: a prebuilt, hash-pinned WAD, or with --d2x built from a
    /// d2x release on its base IOS from NUS.
    InstallCios {
        #[command(flatten)]
        image: ImagePaths,
        /// Local path or HTTPS URL of the WAD, or of the d2x release with --d2x.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        #[arg(long)]
        sha256: Sha256,
        /// FILE is a d2x release archive (installer, modules, ciosmaps.xml) to build from.
        #[arg(long)]
        d2x: bool,
        #[command(flatten)]
        target: cios::Target,
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
        /// Also do what RVLoader's installer does. `--vga` only writes RVLoader's
        /// flag: the image's settings must already enable 480p.
        #[command(flatten)]
        rvloader: Option<rvloader::Arguments>,
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
            image.edit(|nand| {
                cache.prepare()?;
                warn_if_already_present(&nand.install_nus_title(&cache, title_id, version)?);
                Ok(())
            })
        }
        Command::InstallWad {
            image,
            wad,
            sha256,
            allow_unsigned,
        } => {
            let wad = Wad::try_from((wad, sha256))?;
            image.edit(|nand| {
                let installed = if allow_unsigned {
                    nand.install_unsigned_channel(&wad)
                } else {
                    nand.install_wad(&wad)
                }?;
                warn_if_already_present(&installed);
                Ok(())
            })
        }
        Command::InstallCios {
            image,
            file,
            sha256,
            d2x: false,
            target,
        } => {
            let wad = Wad::try_from((file, sha256))?;
            image.edit(|nand| {
                warn_if_already_present(&nand.install_cios(&wad, &target)?);
                Ok(())
            })
        }
        Command::InstallCios {
            image,
            file,
            sha256,
            d2x: true,
            target,
        } => {
            let build = cios::Build::from_release(file, sha256, target.base.into())?;
            let cache = NusCache::from_environment()?;
            image.edit(|nand| {
                cache.prepare()?;
                warn_if_already_present(&nand.install_built_cios(&cache, &build, &target)?);
                Ok(())
            })
        }
        Command::RemoveTitle { image, title_id } => image.edit(|nand| nand.remove_title(title_id)),
        Command::InstallPriiloader {
            image,
            config,
            rvloader,
        } => {
            let rvloader = rvloader.map(Rvloader::try_from).transpose()?;
            let cache = NusCache::from_environment()?;
            image.edit(|nand| install_priiloader(nand, &config, rvloader.as_ref(), &cache))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiiform::recipe::nand::priiloader::{Autoboot, Hack};

    fn install_priiloader(
        args: &[&str],
    ) -> clap::error::Result<(Priiloader, Option<rvloader::Arguments>)> {
        let input = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let fixed = ["wii-nand", "install-priiloader", input, "output"];
        let cli = Cli::try_parse_from(fixed.into_iter().chain(args.iter().copied()))?;
        let Command::InstallPriiloader {
            config, rvloader, ..
        } = cli.command
        else {
            unreachable!()
        };
        Ok((config, rvloader))
    }

    #[test]
    fn rvloader_flags_come_as_a_set() {
        let sha = "30be3599e13f91bba0d5060a316d13bb428ea4d71ba7b35ebe89682cdedd94d7";
        let release = ["--rvloader=RVLoader_v2.1.zip", "--rvloader-sha256", sha];
        assert!(install_priiloader(&[]).unwrap().1.is_none());
        let (_, rvloader) = install_priiloader(&[&release[..], &["--vga"]].concat()).unwrap();
        let rvloader = rvloader.expect("expected the RVLoader flags");
        assert_eq!(rvloader.release, PathBuf::from("RVLoader_v2.1.zip"));
        assert!(rvloader.vga && !rvloader.patch_out_wifi);
        for partial in [
            &release[..1],
            &release[1..],
            &["--vga"],
            &["--patch-out-wifi"],
        ] {
            assert!(install_priiloader(partial).is_err(), "{partial:?}");
        }
        assert!(install_priiloader(&[&release[..], &["--autoboot=file"]].concat()).is_err());
    }

    #[test]
    fn priiloader_cli_uses_defaults_and_accepts_named_flags() {
        let parse = |args: &[&str]| install_priiloader(args).unwrap().0;
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
                autoboot: Some(Autoboot::HomebrewChannel),
                hacks: vec![Hack::Fix480p, Hack::Wiimmfi],
                ..Priiloader::default()
            }
        );
    }
}
