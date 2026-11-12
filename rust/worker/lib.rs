// SPDX-License-Identifier: GPL-2.0-only
//! Client for the bundled read-only native worker.
//!
//! The worker is resolved next to the installed binaries at
//! `../libexec/wiiform-readonly`; it is never searched for in `PATH`.

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::Value;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Clone, Debug)]
pub struct Worker {
    executable: PathBuf,
}

#[derive(Default)]
pub struct DisplaySource {
    pub sysconf: Vec<u8>,
    pub keys_data_sha256: String,
    pub settings_sha256: String,
}

#[derive(Deserialize)]
struct DisplaySourceReport {
    sysconf: String,
    keys_data_sha256: String,
    settings_sha256: String,
}

impl Worker {
    pub fn bundled() -> Result<Self> {
        let current = std::env::current_exe().context("cannot locate the current executable")?;
        let bin_dir = current
            .parent()
            .context("current executable has no parent directory")?;
        let requested = bin_dir.join("../libexec/wiiform-readonly");
        let executable = requested.canonicalize().with_context(|| {
            format!(
                "bundled read-only worker is missing at {}",
                requested.display()
            )
        })?;
        if !executable.is_file() {
            bail!("bundled read-only worker is not a regular file");
        }
        Ok(Self { executable })
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            executable: path.into(),
        }
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    fn run<I>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = OsString>,
    {
        self.run_with_stdin(args, false)
    }

    fn run_with_stdin<I>(&self, args: I, inherit_stdin: bool) -> Result<Output>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut command = Command::new(&self.executable);
        command.args(args);
        if inherit_stdin {
            command.stdin(Stdio::inherit());
        }
        command
            .output()
            .with_context(|| format!("could not start {}", self.executable.display()))
    }

    fn checked<I>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = OsString>,
    {
        let output = self.run(args)?;
        if !output.status.success() {
            let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if diagnostic.is_empty() {
                bail!("read-only worker failed with {}", output.status);
            }
            bail!("{diagnostic}");
        }
        Ok(output)
    }

    fn json<I>(&self, args: I) -> Result<Value>
    where
        I: IntoIterator<Item = OsString>,
    {
        let output = self.checked(args)?;
        serde_json::from_slice(&output.stdout).context("read-only worker returned invalid JSON")
    }

    /// Run an old-compatible worker command, preserving its report file and
    /// exit-code behavior for scripts migrating from `wii-nand`.
    pub fn passthrough<I>(&self, args: I) -> Result<i32>
    where
        I: IntoIterator<Item = OsString>,
    {
        let output = self.run(args)?;
        io::stdout()
            .lock()
            .write_all(&output.stdout)
            .context("cannot forward worker output")?;
        io::stderr()
            .lock()
            .write_all(&output.stderr)
            .context("cannot forward worker diagnostics")?;
        Ok(output.status.code().unwrap_or(1))
    }

    pub fn export_seed(&self, backup: &Path, output: &Path) -> Result<()> {
        self.checked([
            OsString::from("--seed-export"),
            backup.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ])?;
        Ok(())
    }

    pub fn unpack_seed(&self, seed: &Path, directory: &Path) -> Result<()> {
        self.checked([
            OsString::from("--seed-unpack"),
            seed.as_os_str().to_owned(),
            directory.as_os_str().to_owned(),
        ])?;
        Ok(())
    }

    pub fn inventory(&self, nand: &Path) -> Result<Value> {
        self.json([
            OsString::from("--inventory-json"),
            nand.as_os_str().to_owned(),
        ])
    }

    pub fn preflight(
        &self,
        seed: &Path,
        reference: Option<&Path>,
        candidate: &Path,
        boot_policy: &str,
        settings: &[(String, String)],
    ) -> Result<Value> {
        if boot_policy != "stock" && boot_policy != "preserve-source" {
            bail!("boot policy must be stock or preserve-source");
        }
        let mut args = vec![OsString::from("--preflight-json")];
        args.push(OsString::from("--boot-policy"));
        args.push(OsString::from(boot_policy));
        for (key, value) in settings {
            args.push(OsString::from("--setting-override"));
            args.push(OsString::from(format!("{key}={value}")));
        }
        if reference.is_none() {
            args.push(OsString::from("--candidate-only"));
        }
        args.push(seed.as_os_str().to_owned());
        if let Some(reference) = reference {
            args.push(reference.as_os_str().to_owned());
        }
        args.push(candidate.as_os_str().to_owned());
        self.json(args)
    }

    pub fn inspect_wad(
        &self,
        wad: &Path,
        expected_sha256: Option<&str>,
        allow_unverified_signatures: bool,
    ) -> Result<Value> {
        let mut args = vec![OsString::from("--inspect-wad")];
        if allow_unverified_signatures {
            args.push(OsString::from("--allow-unverified-signatures"));
        }
        if let Some(hash) = expected_sha256 {
            args.push(OsString::from("--sha256"));
            args.push(OsString::from(hash));
        }
        args.push(wad.as_os_str().to_owned());
        let output = self
            .run_with_stdin(args, wad == Path::new("-"))
            .context("cannot run WAD inspection")?;
        if !output.status.success() {
            let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if diagnostic.is_empty() {
                bail!("read-only worker failed with {}", output.status);
            }
            bail!("{diagnostic}");
        }
        serde_json::from_slice(&output.stdout).context("read-only worker returned invalid WAD JSON")
    }

    pub fn stock_titles(&self, area: &str) -> Result<Value> {
        self.json([OsString::from("--stock-titles"), OsString::from(area)])
    }

    pub fn inspect_display(&self, nand: &Path) -> Result<Value> {
        self.json([
            OsString::from("--inspect-display"),
            nand.as_os_str().to_owned(),
        ])
    }

    pub fn inspect_sysconf(&self, nand: &Path) -> Result<Value> {
        self.json([
            OsString::from("--inspect-sysconf"),
            nand.as_os_str().to_owned(),
        ])
    }

    pub fn inspect_priiloader(&self, nand: &Path) -> Result<Value> {
        self.json([
            OsString::from("--inspect-priiloader"),
            nand.as_os_str().to_owned(),
        ])
    }

    pub fn display_source(&self, nand: &Path) -> Result<DisplaySource> {
        let output = self.checked([
            OsString::from("--read-display-source"),
            nand.as_os_str().to_owned(),
        ])?;
        let report: DisplaySourceReport = serde_json::from_slice(&output.stdout)
            .context("worker returned invalid display source JSON")?;
        let sysconf = STANDARD
            .decode(report.sysconf)
            .context("worker returned invalid base64 SYSCONF")?;
        if sysconf.len() != 0x4000
            || report.keys_data_sha256.len() != 64
            || report.settings_sha256.len() != 64
        {
            bail!("worker returned invalid authenticated display source");
        }
        Ok(DisplaySource {
            sysconf,
            keys_data_sha256: report.keys_data_sha256,
            settings_sha256: report.settings_sha256,
        })
    }

    pub fn export_priiloader_app(&self, nand: &Path, output: &Path) -> Result<i32> {
        self.passthrough([
            OsString::from("--export-priiloader-app"),
            nand.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ])
    }
}
