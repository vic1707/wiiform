// SPDX-License-Identifier: GPL-2.0-only
//! Process setup shared by the three binaries.
use std::process::ExitCode;

use rustix::{fs::Mode, process::umask};

/// Runs a command with owner-only file creation and one error format.
///
/// Seeds, keys, NAND images and the NUS cache are private, so every file a
/// command creates is restricted to the owner.
pub fn run(name: &str, command: impl FnOnce() -> anyhow::Result<()>) -> ExitCode {
    umask(Mode::RWXG | Mode::RWXO);
    match command() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{name}: {error:#}");
            ExitCode::FAILURE
        }
    }
}
