// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;
use std::io::{self, Write};

fn main() -> Result<()> {
    let schema = wiiform_recipe::json_schema()?;
    io::stdout().lock().write_all(schema.as_bytes())?;
    println!();
    Ok(())
}
