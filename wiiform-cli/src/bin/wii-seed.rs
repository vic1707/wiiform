// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use clap::{Parser, builder::TypedValueParser};
use std::{
    io::{self, Write},
    path::PathBuf,
};
use wiiform::{native, recipe::resolve_regular_file, workflow::read_bounded};

#[derive(Debug, Parser)]
#[command(
    name = "wii-seed",
    version,
    about = "Extract a plaintext console seed to stdout"
)]
struct Cli {
    #[arg(value_parser = clap::builder::PathBufValueParser::new().try_map(|path| resolve_regular_file(&path)))]
    backup: PathBuf,

    /// Explicit matching 1024-byte footer for a footerless backup.
    #[arg(long, value_name = "FILE", value_parser = clap::builder::PathBufValueParser::new().try_map(|path| resolve_regular_file(&path)))]
    keys: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (nand, sidecar) = match cli.keys {
        Some(keys) => (
            read_bounded(&cli.backup, native::RAW_NAND_BYTES, native::RAW_NAND_BYTES)
                .context("--keys requires a footerless NAND backup")?,
            read_bounded(&keys, 0x400, 0x400).context("keys footer must be exactly 1024 bytes")?,
        ),
        None => (
            read_bounded(&cli.backup, native::NAND_BYTES, native::NAND_BYTES)
                .context("expected an embedded footer; for a footerless backup pass --keys FILE")?,
            Vec::new(),
        ),
    };
    let seed = native::extract_seed(&nand, &sidecar)?;
    io::stdout().lock().write_all(seed.as_bytes())?;
    Ok(())
}
