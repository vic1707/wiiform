// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{
    io::{self, Write},
    path::PathBuf,
};
use wiiform::{native, workflow::read_bounded};

#[derive(Debug, Parser)]
#[command(
    name = "wii-seed",
    version,
    about = "Extract a plaintext console seed to stdout"
)]
struct Cli {
    backup: PathBuf,

    /// Matching keys.bin for a footerless backup. Defaults to keys.bin beside BACKUP.
    #[arg(long, value_name = "FILE")]
    keys: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let nand = read_bounded(&cli.backup, native::RAW_NAND_BYTES, native::NAND_BYTES)
        .context("cannot read NAND backup")?;
    ensure!(
        [native::RAW_NAND_BYTES, native::NAND_BYTES].contains(&nand.len()),
        "unsupported NAND backup size"
    );
    let sidecar = if nand.len() == native::RAW_NAND_BYTES {
        let keys = cli
            .keys
            .unwrap_or_else(|| cli.backup.with_file_name("keys.bin"));
        read_bounded(&keys, 0x400, 0x400)?
    } else {
        ensure!(cli.keys.is_none(), "--keys is only for a footerless backup");
        Vec::new()
    };
    let seed = native::extract_seed(&nand, &sidecar)?;
    io::stdout().lock().write_all(seed.as_bytes())?;
    Ok(())
}
