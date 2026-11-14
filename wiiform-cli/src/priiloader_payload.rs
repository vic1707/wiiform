// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};

use crate::recipe::read_bounded;

pub const APP_BYTES: usize = 747_968;
const DEFINITIONS_LIMIT: usize = 64 * 1024;

pub fn load() -> Result<(Vec<u8>, Vec<u8>)> {
    let executable = std::env::current_exe()?;
    let bundled = executable
        .parent()
        .context("executable has no parent directory")?
        .join("../share/wiiform/priiloader");
    let app = read_bounded(&bundled.join("priiloader.app"), APP_BYTES..=APP_BYTES)
        .context("cannot read Priiloader app")?;
    let definitions = read_bounded(&bundled.join("hacks_hash.ini"), 1..=DEFINITIONS_LIMIT)
        .context("cannot read pinned Priiloader hack definitions")?;
    Ok((app, definitions))
}
