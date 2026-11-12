use std::{env, path::PathBuf, process::Command};

fn main() {
    if let Err(error) = run() {
        eprintln!("wiiform-interop-smoke: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os().nth(1).map(PathBuf::from).ok_or(
        "Usage: wiiform-interop-smoke NEW_SYNTHETIC_FILE (must not already exist)",
    )?;
    let path_text = path.to_str().ok_or("path must be valid UTF-8 for this bridge spike")?;

    wiiform_interop_smoke::write_synthetic_nand(path_text)?;

    let exe = env::current_exe()?;
    let sibling_worker = exe
        .parent()
        .ok_or("executable has no parent directory")?
        .join("../libexec/wiiform-readonly");
    let output = Command::new(sibling_worker)
        .arg("smoke-inspect")
        .arg(&path)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned().into());
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
