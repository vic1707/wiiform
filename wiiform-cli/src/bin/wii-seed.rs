// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use clap::Parser;
use std::{io, process::ExitCode};
use wiiform::{
    native,
    recipe::{ResolvedPath, read_bounded},
};

#[derive(Debug, Parser)]
#[command(
    name = "wii-seed",
    version,
    about = "Extract a plaintext console seed to stdout"
)]
struct Cli {
    backup: ResolvedPath,

    /// Explicit matching 1024-byte footer for a footerless backup.
    #[arg(long, value_name = "FILE")]
    keys: Option<ResolvedPath>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    wiiform::cli::run("wii-seed", || run(cli))
}

fn run(cli: Cli) -> Result<()> {
    let (nand, sidecar) = match cli.keys {
        Some(keys) => (
            read_bounded(
                cli.backup.as_ref(),
                native::RAW_NAND_BYTES..=native::RAW_NAND_BYTES,
            )
            .context("--keys requires a footerless NAND backup")?,
            read_bounded(keys.as_ref(), 0x400..=0x400)
                .context("keys footer must be exactly 1024 bytes")?,
        ),
        None => (
            read_bounded(cli.backup.as_ref(), native::NAND_BYTES..=native::NAND_BYTES)
                .context("expected an embedded footer; for a footerless backup pass --keys FILE")?,
            Vec::new(),
        ),
    };
    native::extract_seed(&nand, &sidecar, io::stdout().lock())?;
    Ok(())
}
