// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use clap::Parser;
use std::{io, process::ExitCode};
use wiiform::{
    native::{KEYS_BYTES, Nand, RAW_NAND_BYTES, Session},
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
    let session = Session::acquire()?;
    let seed = match cli.keys {
        Some(keys) => {
            let nand = read_bounded(cli.backup.as_ref(), RAW_NAND_BYTES..=RAW_NAND_BYTES)
                .context("--keys requires a footerless NAND backup")?;
            let keys: Box<[u8; KEYS_BYTES]> = read_bounded(keys.as_ref(), KEYS_BYTES..=KEYS_BYTES)
                .context("keys footer must be exactly 1024 bytes")?
                .into_boxed_slice()
                .try_into()
                .expect("the read is bounded to the footer size");
            session.extract_seed(&nand, Some(&keys))?
        }
        None => {
            let nand = Nand::read(cli.backup.as_ref())
                .context("expected an embedded footer; for a footerless backup pass --keys FILE")?;
            session.extract_seed(nand.as_bytes(), None)?
        }
    };
    seed.write(io::stdout().lock())
}
