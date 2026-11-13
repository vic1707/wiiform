// SPDX-License-Identifier: GPL-2.0-only
use std::path::Path;

use anyhow::{Context, Result};

use crate::ffi::bridge as ffi;
use crate::recipe::{
    Aspect, Autoboot, Display, HackSettings, Language, Menu, MountPoint, PriiloaderSettings,
    ReturnTo, SensorBar, Setup, Sha256, ShutdownTo,
};
use crate::seed::StockSeed;

pub use ffi::{BootPolicy, CacheMode, InstalledWad, StockIdentity, WadPolicy};

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
    cache_mode: CacheMode,
) -> Result<StockIdentity> {
    let menu = match menu {
        Menu::E43 => ffi::Menu::E43,
        Menu::U43 => ffi::Menu::U43,
        Menu::J43 => ffi::Menu::J43,
    };
    Ok(ffi::create_stock(
        seed.keys(),
        seed.boot_blocks(),
        seed.bad_blocks(),
        seed.setting_txt(),
        nand,
        cache_dir(cache)?,
        &ffi::StockOptions {
            menu,
            boot_policy: policy,
            cache_mode,
        },
    )?)
}

fn toggle(value: Option<bool>) -> ffi::Toggle {
    match value {
        None => ffi::Toggle::Unset,
        Some(false) => ffi::Toggle::Off,
        Some(true) => ffi::Toggle::On,
    }
}

pub fn apply_system_settings(
    nand: &mut [u8],
    menu: Menu,
    setup: Option<&Setup>,
    display: Option<&Display>,
) -> Result<()> {
    let language = match setup.and_then(|setup| setup.language) {
        None => ffi::Language::Unset,
        Some(Language::Ja) => ffi::Language::Ja,
        Some(Language::En) => ffi::Language::En,
        Some(Language::De) => ffi::Language::De,
        Some(Language::Fr) => ffi::Language::Fr,
        Some(Language::Es) => ffi::Language::Es,
        Some(Language::It) => ffi::Language::It,
        Some(Language::Nl) => ffi::Language::Nl,
    };
    Ok(ffi::apply_system_settings(
        nand,
        &ffi::SystemSettings {
            nickname: setup
                .and_then(|setup| setup.nickname.as_ref())
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default(),
            language,
            country: setup.map_or(0, |setup| setup.country_for(menu).as_u8()),
            sensor_bar_above: toggle(setup.map(|setup| setup.sensor_bar_for() == SensorBar::Above)),
            widescreen: toggle(
                display
                    .and_then(|display| display.aspect)
                    .map(|aspect| aspect == Aspect::SixteenNine),
            ),
            progressive: toggle(display.and_then(|display| display.progressive)),
            pal60: toggle(display.and_then(|display| display.pal60)),
        },
    )?)
}

pub fn install_nus_title(
    nand: &mut [u8],
    cache: &Path,
    title_id: u64,
    version: u16,
    mode: CacheMode,
) -> Result<()> {
    Ok(ffi::install_nus_title(
        nand,
        cache_dir(cache)?,
        title_id,
        version,
        mode,
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
    hacks: Option<&HackSettings>,
) -> Result<()> {
    let settings = ffi::PriiloaderSettings {
        autoboot: match settings.autoboot {
            None => ffi::Autoboot::Unset,
            Some(Autoboot::Disabled) => ffi::Autoboot::Disabled,
            Some(Autoboot::HomebrewChannel) => ffi::Autoboot::HomebrewChannel,
            Some(Autoboot::BootmiiIos) => ffi::Autoboot::BootmiiIos,
            Some(Autoboot::SystemMenu) => ffi::Autoboot::SystemMenu,
            Some(Autoboot::File) => ffi::Autoboot::File,
        },
        return_to: match settings.return_to {
            None => ffi::ReturnTo::Unset,
            Some(ReturnTo::SystemMenu) => ffi::ReturnTo::SystemMenu,
            Some(ReturnTo::Priiloader) => ffi::ReturnTo::Priiloader,
            Some(ReturnTo::Autoboot) => ffi::ReturnTo::Autoboot,
            Some(ReturnTo::File) => ffi::ReturnTo::File,
        },
        shutdown_to: match settings.shutdown_to {
            None => ffi::ShutdownTo::Unset,
            Some(ShutdownTo::None) => ffi::ShutdownTo::None,
            Some(ShutdownTo::Priiloader) => ffi::ShutdownTo::Priiloader,
            Some(ShutdownTo::Autoboot) => ffi::ShutdownTo::Autoboot,
        },
        preferred_mount_point: match settings.preferred_mount_point {
            None => ffi::MountPoint::Unset,
            Some(MountPoint::Auto) => ffi::MountPoint::Auto,
            Some(MountPoint::Sd) => ffi::MountPoint::Sd,
            Some(MountPoint::Usb) => ffi::MountPoint::Usb,
        },
        stop_disc: toggle(settings.stop_disc),
        lid_slot_on_error: toggle(settings.lid_slot_on_error),
        ignore_shutdown_mode: toggle(settings.ignore_shutdown_mode),
        black_background: toggle(settings.black_background),
        dump_gecko_text: toggle(settings.dump_gecko_text),
        show_rc_updates: toggle(settings.show_rc_updates),
    };
    use ffi::Hack;
    let overrides: Vec<_> = hacks
        .into_iter()
        .flat_map(|hacks| {
            [
                (Hack::Fix480p, hacks.fix_480p),
                (Hack::BlockOnlineUpdates, hacks.block_online_updates),
                (Hack::BlockDiscUpdates, hacks.block_disc_updates),
                (Hack::BlockDiscAutoboot, hacks.block_disc_autoboot),
                (Hack::AutoPressA, hacks.auto_press_a),
                (Hack::RemoveDeflicker, hacks.remove_deflicker),
                (Hack::MoveDiscChannel, hacks.move_disc_channel),
                (
                    Hack::RemoveNocopySaveProtection,
                    hacks.remove_nocopy_save_protection,
                ),
                (Hack::ReplaceHealthScreen, hacks.replace_health_screen),
                (Hack::NoBackgroundMusic, hacks.no_background_music),
                (Hack::Wiimmfi, hacks.wiimmfi),
                (Hack::RegionFreeEverything, hacks.region_free_everything),
            ]
        })
        .filter_map(|(hack, value)| value.map(|enabled| ffi::HackOverride { hack, enabled }))
        .collect();
    Ok(ffi::apply_priiloader(
        nand,
        app,
        definitions,
        &settings,
        &overrides,
    )?)
}

pub fn prepare_backupmii(nand: &mut [u8], expected: &StockIdentity) -> Result<Vec<u8>> {
    Ok(ffi::prepare_backupmii(nand, expected)?)
}
