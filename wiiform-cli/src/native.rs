// SPDX-License-Identifier: GPL-2.0-only
use std::{io::Write, path::Path};

use anyhow::{Context, Result};

use crate::ffi::bridge as ffi;
use crate::recipe::{
    Aspect, Cios, HackSettings, Hbc, Menu, PriiloaderSettings, SensorBar, Setup, SignaturePolicy,
    Title,
};
use crate::seed::StockSeed;

pub use ffi::{InstalledWad, StockIdentity};

impl InstalledWad {
    pub fn warn_if_already_present(&self) {
        if self.already_present {
            eprintln!(
                "warning: title {:016x} v{} is already installed and verified",
                self.title_id, self.version
            );
        }
    }
}

/// Boot-area validation policy for stock NAND creation.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[repr(u8)]
pub enum BootPolicy {
    Stock = 0,
    PreserveSource = 1,
}

pub const RAW_NAND_BYTES: usize = 0x2100_0000;
pub const NAND_BYTES: usize = RAW_NAND_BYTES + 0x400;

fn cache_dir(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("NUS cache directory must be UTF-8: {}", path.display()))
}

pub fn extract_seed<W: Write>(nand: &[u8], sidecar_keys: &[u8], writer: W) -> Result<()> {
    let parts = ffi::extract_seed(nand, sidecar_keys)?;
    crate::seed::write_seed(writer, parts)
}

pub fn create_stock(
    seed: &StockSeed,
    nand: &mut [u8],
    cache: &Path,
    menu: Menu,
    policy: BootPolicy,
    offline: bool,
) -> Result<StockIdentity> {
    Ok(ffi::create_stock(
        &seed.as_native(),
        nand,
        cache_dir(cache)?,
        &ffi::StockOptions {
            menu: menu.version(),
            boot_policy: policy as u8,
            offline,
        },
    )?)
}

pub fn apply_system_settings(nand: &mut [u8], menu: Menu, setup: &Setup) -> Result<()> {
    let display = setup.display;
    Ok(ffi::apply_system_settings(
        nand,
        &ffi::SystemSettings {
            nickname: setup.nickname.as_ref(),
            language: setup.language_for(menu).as_u8(),
            country: setup.country_for(menu).as_u8(),
            sensor_bar_above: setup.sensor_bar == SensorBar::Above,
            widescreen: display.aspect == Aspect::SixteenNine,
            progressive: display.progressive,
            pal60: display.pal60,
        },
    )?)
}

pub fn install_nus_title(
    nand: &mut [u8],
    cache: &Path,
    title_id: u64,
    version: u16,
    offline: bool,
) -> Result<()> {
    Ok(ffi::install_nus_title(
        nand,
        cache_dir(cache)?,
        title_id,
        version,
        offline,
    )?)
}

fn fetch_https(url: &str, limit: usize) -> Result<Vec<u8>> {
    Ok(ffi::fetch_https(url, limit)?)
}

pub fn install_wad(nand: &mut [u8], title: &Title) -> Result<InstalledWad> {
    let (wad, sha256) = title.resolve(fetch_https)?;
    let policy = match title.options.signature_policy {
        SignaturePolicy::Nintendo => ffi::WadPolicy::Standard,
        SignaturePolicy::AllowUnsigned => ffi::WadPolicy::LegacyChannel,
    };
    Ok(ffi::install_wad(nand, &wad, sha256, policy)?)
}

pub fn install_hbc(nand: &mut [u8], hbc: &Hbc) -> Result<InstalledWad> {
    let (wad, digest) = hbc.resolve(fetch_https)?;
    Ok(ffi::install_wad(
        nand,
        &wad,
        digest.as_bytes(),
        ffi::WadPolicy::HomebrewChannel,
    )?)
}

pub fn install_cios(nand: &mut [u8], cios: &Cios) -> Result<InstalledWad> {
    let (wad, sha256) = cios.resolve(fetch_https)?;
    let target = &cios.options.target;
    Ok(ffi::install_cios(
        nand,
        &wad,
        sha256,
        &ffi::CiosOptions {
            slot: target.slot,
            base: target.base,
            revision: target.revision,
            replace_stock_stub: target.replace_stock_stub,
        },
    )?)
}

pub fn remove_title(nand: &mut [u8], title_id: u64) -> Result<()> {
    Ok(ffi::remove_title(nand, title_id)?)
}

pub fn apply_priiloader(
    nand: &mut [u8],
    app: &[u8],
    definitions: &[u8],
    settings: &PriiloaderSettings,
    hacks: &HackSettings,
) -> Result<()> {
    let settings = ffi::PriiloaderSettings {
        autoboot: settings.autoboot as u8,
        return_to: settings.return_to as u8,
        shutdown_to: settings.shutdown_to as u8,
        preferred_mount_point: settings.preferred_mount_point as u8,
        stop_disc: settings.stop_disc,
        lid_slot_on_error: settings.lid_slot_on_error,
        ignore_shutdown_mode: settings.ignore_shutdown_mode,
        black_background: settings.black_background,
        dump_gecko_text: settings.dump_gecko_text,
        show_rc_updates: settings.show_rc_updates,
    };
    let enabled_hacks: Vec<_> = hacks.enabled().map(|hack| hack as u8).collect();
    Ok(ffi::apply_priiloader(
        nand,
        app,
        definitions,
        &settings,
        &enabled_hacks,
    )?)
}

pub fn prepare_backupmii(nand: &mut [u8], expected: &StockIdentity) -> Result<Vec<u8>> {
    Ok(ffi::prepare_backupmii(nand, expected)?)
}
