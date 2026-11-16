// SPDX-License-Identifier: GPL-2.0-only
use anyhow::Result;

fn main() -> Result<()> {
    let schema = wiiform_recipe::json_schema()?;
    println!("{schema}");
    Ok(())
}
