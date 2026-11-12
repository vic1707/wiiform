// SPDX-License-Identifier: GPL-2.0-only
use std::{env, fs::OpenOptions, io::Write, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("wiiform-recipe-schema: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let output = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("Usage: wiiform-recipe-schema [NEW_SCHEMA.json]".into());
    }
    let mut document = wiiform::recipe::json_schema()?;
    document.push('\n');
    if let Some(path) = output {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(document.as_bytes())?;
        file.sync_all()?;
    } else {
        print!("{document}");
    }
    Ok(())
}
