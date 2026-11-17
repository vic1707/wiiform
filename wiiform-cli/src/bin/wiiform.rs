// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use clap::Parser;
use std::{path::PathBuf, process::ExitCode};
use wiiform::{
    recipe::Menu,
    workflow::{self, BootPolicy, BuildOptions, FileOrStdin, NusCache},
};

#[derive(Debug, Parser)]
#[command(
    name = "wiiform",
    version,
    about = "Build and verify a Wii NAND restore bundle from a seed and recipe"
)]
struct Cli {
    /// Plaintext console seed YAML: a path, which may be a pipe such as
    /// <(age -d seed.age), or - to read from stdin.
    #[arg(long)]
    seed: FileOrStdin,

    /// Declarative Wiiform recipe YAML, likewise. Its WAD paths are relative to
    /// the recipe file, or to the working directory for stdin. Only one of the
    /// seed and the recipe can be -.
    #[arg(long)]
    recipe: FileOrStdin,

    /// New output directory for nand.bin and its matching keys.bin.
    #[arg(long, value_name = "DIR")]
    output: PathBuf,

    /// Boot-area validation policy.
    #[arg(long, value_enum, default_value = "stock")]
    boot_policy: BootPolicy,

    /// Override the recipe menu with 4.3E, 4.3U, or 4.3J.
    #[arg(long, value_enum)]
    menu: Option<Menu>,
}

fn run(cli: Cli) -> Result<()> {
    let options = BuildOptions {
        seed: cli.seed,
        recipe: cli.recipe,
        output: cli.output,
        cache: NusCache::from_environment()?,
        boot_policy: cli.boot_policy,
        menu: cli.menu,
    };
    workflow::build(&options).context("Wiiform build failed")?;
    println!("Built NAND restore bundle at {}", options.output.display());
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    wiiform::cli::run("wiiform", || run(cli))
}
