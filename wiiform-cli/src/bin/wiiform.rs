// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use clap::{
    Parser,
    builder::{PathBufValueParser, TypedValueParser},
};
use std::{path::PathBuf, process::ExitCode};
use wiiform::{
    recipe::{Menu, resolve_regular_file},
    workflow::{self, BootPolicy, BuildOptions},
};

#[derive(Debug, Parser)]
#[command(
    name = "wiiform",
    version,
    about = "Build and verify a Wii NAND restore bundle from a seed and recipe"
)]
struct Cli {
    /// Plaintext console seed YAML, or - to read from stdin.
    #[arg(long, value_parser = PathBufValueParser::new().try_map(seed_path))]
    seed: PathBuf,

    /// Declarative Wiiform recipe YAML.
    #[arg(long, value_parser = PathBufValueParser::new().try_map(|path| resolve_regular_file(&path)))]
    recipe: PathBuf,

    /// New output directory for nand.bin and its matching keys.bin.
    #[arg(long, value_name = "DIR")]
    output: PathBuf,

    /// Boot-area validation policy.
    #[arg(long, value_enum, default_value = "stock")]
    boot_policy: BootPolicy,

    /// Override the recipe menu with 4.3E, 4.3U, or 4.3J.
    #[arg(long)]
    menu: Option<Menu>,
}

fn seed_path(path: PathBuf) -> Result<PathBuf> {
    if path.as_os_str() == "-" {
        Ok(path)
    } else {
        resolve_regular_file(&path)
    }
}

fn run(cli: Cli) -> Result<()> {
    let (cache, offline) = workflow::nus_environment()?;
    let options = BuildOptions {
        seed: cli.seed,
        recipe: cli.recipe,
        output: cli.output,
        cache,
        offline,
        boot_policy: cli.boot_policy,
        menu: cli.menu,
    };
    workflow::build(&options).context("Wiiform build failed")?;
    println!("Built NAND restore bundle at {}", options.output.display());
    Ok(())
}

fn main() -> ExitCode {
    #[cfg(unix)]
    unsafe {
        libc::umask(0o077);
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("wiiform: {error:#}");
            ExitCode::FAILURE
        }
    }
}
