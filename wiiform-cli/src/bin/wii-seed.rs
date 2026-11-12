// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use wiiform::native;

#[derive(Debug, Parser)]
#[command(
    name = "wii-seed",
    version,
    about = "Export and unpack decrypted Wii console seeds"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Export the console inputs from a full NAND backup into a plaintext seed.
    Export {
        nand_backup: PathBuf,
        new_seed: PathBuf,
    },
    /// Unpack a decrypted seed into individual plaintext files.
    Unpack {
        seed: PathBuf,
        new_directory: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Export {
            nand_backup,
            new_seed,
        } => native::export_seed(&nand_backup, &new_seed)?,
        Command::Unpack {
            seed,
            new_directory,
        } => native::unpack_seed(&seed, &new_directory)?,
    }
    println!("Wrote plaintext output. Keep console key material private.");
    Ok(())
}
