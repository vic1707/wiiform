// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::{path::PathBuf, process::ExitCode};
use wiiform::{
    recipe::Menu,
    workflow::{self, BootPolicy, BuildOptions},
};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BootPolicyArg {
    Stock,
    #[value(name = "preserve-source")]
    PreserveSource,
}

#[derive(Debug, Parser)]
#[command(
    name = "wiiform",
    version,
    about = "Build and verify a Wii NAND restore bundle from a seed and recipe"
)]
struct Cli {
    /// Decrypted console seed YAML.
    #[arg(long)]
    seed: PathBuf,

    /// Declarative Wiiform recipe YAML.
    #[arg(long)]
    recipe: PathBuf,

    /// New output directory for nand.bin and its matching keys.bin.
    #[arg(long, value_name = "DIR")]
    output: PathBuf,

    /// NUS and Priiloader cache directory.
    #[arg(long, default_value = "build/nus-cache")]
    cache: PathBuf,

    /// Use only local cache entries.
    #[arg(long)]
    offline: bool,

    /// Boot-area validation policy.
    #[arg(long, value_enum, default_value = "stock")]
    boot_policy: BootPolicyArg,

    /// Override the recipe menu with 4.3E, 4.3U, or 4.3J.
    #[arg(long)]
    menu: Option<Menu>,
}

fn run(cli: Cli) -> Result<()> {
    let boot_policy = match cli.boot_policy {
        BootPolicyArg::Stock => BootPolicy::Stock,
        BootPolicyArg::PreserveSource => BootPolicy::PreserveSource,
    };
    let options = BuildOptions {
        seed: cli.seed,
        recipe: cli.recipe,
        output: cli.output,
        cache: cli.cache,
        offline: cli.offline,
        boot_policy,
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
