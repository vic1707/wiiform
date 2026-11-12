// SPDX-License-Identifier: GPL-2.0-only
use crate::ffi::bridge as ffi;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

pub use ffi::{BootPolicy, DisplaySource, Inspection, SettingOverride};

pub fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("native operations require a UTF-8 path: {}", path.display()))
}

fn report(text: String) -> Result<Value> {
    serde_json::from_str(&text).context("native core returned invalid JSON")
}

pub fn export_seed(nand: &Path, seed: &Path) -> Result<()> {
    Ok(ffi::export_seed(path_text(nand)?, path_text(seed)?)?)
}

pub fn unpack_seed(seed: &Path, directory: &Path) -> Result<()> {
    Ok(ffi::unpack_seed(path_text(seed)?, path_text(directory)?)?)
}

pub fn inventory(nand: &Path) -> Result<Value> {
    report(ffi::inventory(path_text(nand)?)?)
}

pub fn preflight(
    seed: &Path,
    reference: Option<&Path>,
    candidate: &Path,
    policy: BootPolicy,
    settings: &[SettingOverride],
) -> Result<Value> {
    let reference = reference.map(path_text).transpose()?.unwrap_or_default();
    report(ffi::preflight(
        path_text(seed)?,
        reference,
        path_text(candidate)?,
        policy,
        settings,
    )?)
}

pub fn inspect(nand: &Path, scope: Inspection) -> Result<Value> {
    report(ffi::inspect(path_text(nand)?, scope)?)
}

pub fn display_source(nand: &Path) -> Result<DisplaySource> {
    Ok(ffi::display_source(path_text(nand)?)?)
}

pub fn priiloader_app(nand: &Path) -> Result<Vec<u8>> {
    Ok(ffi::priiloader_app(path_text(nand)?)?)
}

pub fn inspect_wad(
    wad: &Path,
    expected_sha256: Option<&str>,
    allow_unverified_signatures: bool,
) -> Result<Value> {
    let policy = if allow_unverified_signatures {
        ffi::WadPolicy::LegacyChannel
    } else {
        ffi::WadPolicy::Standard
    };
    report(ffi::inspect_wad(
        path_text(wad)?,
        expected_sha256.unwrap_or_default(),
        policy,
    )?)
}

pub fn stock_titles(area: &str) -> Result<Vec<(u64, u16)>> {
    Ok(ffi::stock_titles(area)?
        .into_iter()
        .map(|title| (title.title_id, title.version))
        .collect())
}

pub fn create(
    seed: &Path,
    output: &Path,
    policy: BootPolicy,
    settings: &[SettingOverride],
) -> Result<()> {
    Ok(ffi::create(
        path_text(seed)?,
        path_text(output)?,
        policy,
        settings,
    )?)
}

pub fn install_nus_title(
    image: &Path,
    cache: &Path,
    title: u64,
    version: u16,
    offline: bool,
) -> Result<()> {
    Ok(ffi::install_nus_title(
        path_text(image)?,
        path_text(cache)?,
        title,
        version,
        offline,
    )?)
}

pub fn install_wad(image: &Path, package: &Path, digest: &str, allow_unsigned: bool) -> Result<()> {
    Ok(ffi::install_wad(
        path_text(image)?,
        path_text(package)?,
        digest,
        allow_unsigned,
    )?)
}

pub fn remove_title(image: &Path, title: u64) -> Result<()> {
    Ok(ffi::remove_title(path_text(image)?, title)?)
}

pub fn set_display(
    image: &Path,
    source: DisplaySource,
    settings: &[SettingOverride],
) -> Result<Value> {
    report(ffi::set_display(path_text(image)?, &source, settings)?)
}

pub fn set_sysconf(
    image: &Path,
    source: DisplaySource,
    settings: &[SettingOverride],
) -> Result<Value> {
    report(ffi::set_sysconf(path_text(image)?, &source, settings)?)
}

pub fn install_priiloader(
    image: &Path,
    app: &Path,
    output: &Path,
    settings: &[SettingOverride],
) -> Result<Value> {
    report(ffi::install_priiloader(
        path_text(image)?,
        path_text(app)?,
        path_text(output)?,
        settings,
    )?)
}

pub fn set_priiloader_settings(
    image: &Path,
    output: &Path,
    settings: &[SettingOverride],
    definitions: Option<&Path>,
    hacks: &[SettingOverride],
) -> Result<Value> {
    let definitions = definitions.map(path_text).transpose()?.unwrap_or_default();
    report(ffi::set_priiloader_settings(
        path_text(image)?,
        path_text(output)?,
        settings,
        definitions,
        hacks,
    )?)
}

pub fn normalize_backupmii_footer(image: &Path, expected_sha256: &str) -> Result<String> {
    Ok(ffi::normalize_backupmii_footer(
        path_text(image)?,
        expected_sha256,
    )?)
}

pub fn prepare_backupmii_keys(
    image: &Path,
    output: &Path,
    expected_sha256: &str,
) -> Result<String> {
    Ok(ffi::prepare_backupmii_keys(
        path_text(image)?,
        path_text(output)?,
        expected_sha256,
    )?)
}
