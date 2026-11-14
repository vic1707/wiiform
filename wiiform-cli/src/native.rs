// SPDX-License-Identifier: GPL-2.0-only
use std::path::Path;

use anyhow::{Context, Result};

use crate::ffi::bridge as ffi;
use crate::recipe::{
    Aspect, Display, HackSettings, Menu, PriiloaderSettings, SensorBar, Setup, Sha256,
};
use crate::seed::StockSeed;

pub use ffi::{InstalledWad, StockIdentity, WadPolicy};

/// Boot-area validation policy for stock NAND creation.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[repr(u8)]
pub enum BootPolicy {
    Stock = 0,
    PreserveSource = 1,
}

pub const RAW_NAND_BYTES: usize = 0x2100_0000;
pub const NAND_BYTES: usize = RAW_NAND_BYTES + 0x400;
pub const WAD_MAX_BYTES: usize = RAW_NAND_BYTES;

fn cache_dir(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("NUS cache directory must be UTF-8: {}", path.display()))
}

pub fn extract_seed(nand: &[u8], sidecar_keys: &[u8]) -> Result<String> {
    Ok(ffi::extract_seed(nand, sidecar_keys)?)
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
        seed.keys(),
        seed.boot_blocks(),
        seed.bad_blocks(),
        seed.setting_txt(),
        nand,
        cache_dir(cache)?,
        &ffi::StockOptions {
            menu: menu.version(),
            boot_policy: policy as u8,
            offline,
        },
    )?)
}

pub fn apply_system_settings(
    nand: &mut [u8],
    menu: Menu,
    setup: Option<&Setup>,
    display: Option<&Display>,
) -> Result<()> {
    let default_setup;
    let setup = match setup {
        Some(setup) => setup,
        None => {
            default_setup = Setup::default();
            &default_setup
        }
    };
    let display = display.copied().unwrap_or_default();
    Ok(ffi::apply_system_settings(
        nand,
        &ffi::SystemSettings {
            nickname: setup.nickname.as_str(),
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

pub fn install_wad(
    nand: &mut [u8],
    wad: &[u8],
    sha256: &Sha256,
    policy: WadPolicy,
) -> Result<InstalledWad> {
    let mut digest = [0; 32];
    for (byte, pair) in digest
        .iter_mut()
        .zip(sha256.as_str().as_bytes().as_chunks::<2>().0)
    {
        *byte = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
    }
    Ok(ffi::install_wad(nand, wad, &digest, policy)?)
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
