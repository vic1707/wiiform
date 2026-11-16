// SPDX-License-Identifier: GPL-2.0-only
//! Typed access to the Wiiform C++ library.
//!
//! Everything here translates between recipe types and the bridge; decisions
//! about what to build live in [`crate::workflow`].
use std::{
    collections::BTreeSet,
    marker::PhantomData,
    path::Path,
    sync::OnceLock,
    thread::{self, ThreadId},
};

use anyhow::{Context, Result, ensure};

use crate::ffi::bridge as ffi;
use crate::recipe::{
    self, Aspect, CiosTarget, Hack, Menu, Priiloader, Region, SensorBar, Setup, TitleId, Wad,
    read_bounded,
};
use crate::seed::{ExtractedSeed, StockSeed};

pub const RAW_NAND_BYTES: usize = 0x2100_0000;
pub const KEYS_BYTES: usize = 0x400;
pub const NAND_BYTES: usize = RAW_NAND_BYTES + KEYS_BYTES;
pub const BOOT_BYTES: usize = 0x10_8000;

/// Recipe enums and their library counterparts carry the same variants.
macro_rules! same_variants {
    ($($from:ty => $to:ty { $($variant:ident),+ $(,)? })+) => {$(
        impl From<$from> for $to {
            fn from(value: $from) -> Self {
                match value {
                    $(<$from>::$variant => Self::$variant,)+
                }
            }
        }
    )+};
}

same_variants! {
    Menu => ffi::Menu { E43, U43, J43 }
    recipe::Language => ffi::Language { Ja, En, De, Fr, Es, It, Nl }
    recipe::Autoboot => ffi::Autoboot { Disabled, HomebrewChannel, BootmiiIos, SystemMenu, File }
    recipe::ReturnTo => ffi::ReturnTo { SystemMenu, Priiloader, Autoboot, File }
    recipe::ShutdownTo => ffi::ShutdownTo { None, Priiloader, Autoboot }
    recipe::MountPoint => ffi::MountPoint { Auto, Sd, Usb }
    Hack => ffi::Hack {
        Fix480p, BlockOnlineUpdates, BlockDiscUpdates, BlockDiscAutoboot, AutoPressA,
        RemoveDeflicker, MoveDiscChannel, RemoveNocopySaveProtection, ReplaceHealthScreen,
        NoBackgroundMusic, Wiimmfi, RegionFreeEverything,
    }
}

/// Which boot area a new image may be built around.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum BootPolicy {
    /// Every boot2 copy is genuinely signed.
    Stock,
    /// Whatever the seed's console has, such as BootMii installed as boot2.
    PreserveSource,
}

/// Where Nintendo titles come from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NusCache {
    /// Download into memory and keep nothing.
    Memory,
    /// Reuse files in a directory. Offline, a missing file is an error.
    Directory {
        path: std::path::PathBuf,
        offline: bool,
    },
}

/// A complete NAND image with its 1024-byte key footer.
pub struct Nand(Box<[u8; NAND_BYTES]>);

impl Nand {
    pub fn zeroed() -> Result<Self> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(NAND_BYTES)
            .context("cannot allocate NAND buffer")?;
        bytes.resize(NAND_BYTES, 0);
        Self::try_from(bytes)
    }

    pub fn read(path: &Path) -> Result<Self> {
        read_bounded(path, NAND_BYTES..=NAND_BYTES)?.try_into()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    /// The key footer, which is also the `keys.bin` sidecar.
    pub fn keys(&self) -> &[u8] {
        &self.0[RAW_NAND_BYTES..]
    }
}

impl TryFrom<Vec<u8>> for Nand {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self> {
        bytes
            .into_boxed_slice()
            .try_into()
            .map(Self)
            .map_err(|_| anyhow::anyhow!("expected a NAND image with its key footer"))
    }
}

/// Boot area and key footer digests of a freshly initialized image.
pub struct StockIdentity(ffi::StockIdentity);

#[cfg(test)]
impl StockIdentity {
    /// An identity no image matches, for tests that fail before verification.
    pub(crate) fn unverifiable() -> Self {
        Self(ffi::StockIdentity {
            boot_area: [0; 32],
            footer: [0; 32],
        })
    }
}

/// A title a WAD installed, or that was already installed identically.
#[derive(Clone, Copy, Debug)]
pub struct InstalledWad {
    pub title_id: TitleId,
    /// `None` for a region code the Wii does not define.
    pub region: Option<Region>,
    pub version: u16,
    pub already_present: bool,
}

impl TryFrom<ffi::InstalledWad> for InstalledWad {
    type Error = anyhow::Error;

    fn try_from(wad: ffi::InstalledWad) -> Result<Self> {
        Ok(Self {
            title_id: TitleId::new(wad.title_id).context("installed title has no ID")?,
            region: Region::from_code(wad.region),
            version: wad.version,
            already_present: wad.already_present,
        })
    }
}

/// The right to call the library, which works on a single thread for the
/// whole process: the one that makes the first call.
pub struct Session(PhantomData<*const ()>);

impl Session {
    pub fn acquire() -> Result<Self> {
        static OWNER: OnceLock<ThreadId> = OnceLock::new();
        let current = thread::current().id();
        ensure!(
            *OWNER.get_or_init(|| current) == current,
            "the Wiiform library is already in use on another thread"
        );
        Ok(Self(PhantomData))
    }

    /// Reads a seed from a backup. A footerless backup needs its key sidecar.
    pub fn extract_seed(
        &self,
        nand: &[u8],
        sidecar_keys: Option<&[u8; KEYS_BYTES]>,
    ) -> Result<ExtractedSeed> {
        let sidecar_keys = sidecar_keys.map_or(&[][..], |keys| keys);
        Ok(ExtractedSeed(ffi::extract_seed(nand, sidecar_keys)?))
    }

    pub fn stock_titles(&self, menu: Menu) -> Result<Vec<(TitleId, u16)>> {
        ffi::stock_titles(menu.into())?
            .into_iter()
            .map(|title| {
                Ok((
                    TitleId::new(title.id).context("stock title has no ID")?,
                    title.version,
                ))
            })
            .collect()
    }

    pub fn initialize_image(
        &self,
        nand: &mut Nand,
        seed: &StockSeed,
        menu: Menu,
        policy: BootPolicy,
    ) -> Result<StockIdentity> {
        let initialize = match policy {
            BootPolicy::Stock => ffi::initialize_image,
            BootPolicy::PreserveSource => ffi::initialize_image_with_source_boot,
        };
        let identity = initialize(&mut nand.0[..], &seed.as_native(), menu.into())?;
        Ok(StockIdentity(identity))
    }

    pub fn verify_stock(&self, nand: &Nand, seed: &StockSeed, menu: Menu) -> Result<()> {
        Ok(ffi::verify_stock(
            nand.as_bytes(),
            &seed.as_native(),
            menu.into(),
        )?)
    }

    pub fn verify_generated_nand(&self, nand: &Nand, expected: &StockIdentity) -> Result<()> {
        Ok(ffi::verify_generated_nand(nand.as_bytes(), &expected.0)?)
    }

    pub fn install_nus_title(
        &self,
        nand: &mut Nand,
        cache: &NusCache,
        title_id: TitleId,
        version: u16,
    ) -> Result<()> {
        let title = ffi::TitleVersion {
            id: title_id.as_u64(),
            version,
        };
        match cache {
            NusCache::Memory => ffi::install_nus_title(&mut nand.0[..], title)?,
            NusCache::Directory { path, offline } => {
                let directory = path.to_str().with_context(|| {
                    format!("NUS cache directory must be UTF-8: {}", path.display())
                })?;
                let cache = ffi::NusCache {
                    directory,
                    offline: *offline,
                };
                ffi::install_cached_nus_title(&mut nand.0[..], &cache, title)?;
            }
        }
        Ok(())
    }

    /// Installs a Nintendo-signed channel or IOS.
    pub fn install_wad(&self, nand: &mut Nand, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_wad(&mut nand.0[..], wad)?.try_into()
    }

    /// Installs a downloadable channel that need not be signed.
    pub fn install_unsigned_channel(&self, nand: &mut Nand, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_unsigned_channel(&mut nand.0[..], wad)?.try_into()
    }

    pub fn install_homebrew_channel(&self, nand: &mut Nand, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_homebrew_channel(&mut nand.0[..], wad)?.try_into()
    }

    pub fn install_cios(
        &self,
        nand: &mut Nand,
        wad: &Wad,
        target: &CiosTarget,
    ) -> Result<InstalledWad> {
        let options = ffi::CiosOptions {
            slot: target.slot.into(),
            base: target.base.into(),
            revision: target.revision,
            replace_stock_stub: target.replace_stock_stub,
        };
        ffi::install_cios(&mut nand.0[..], wad, &options)?.try_into()
    }

    pub fn remove_title(&self, nand: &mut Nand, title_id: TitleId) -> Result<()> {
        Ok(ffi::remove_title(&mut nand.0[..], title_id.as_u64())?)
    }

    pub fn apply_system_settings(&self, nand: &mut Nand, menu: Menu, setup: &Setup) -> Result<()> {
        let display = setup.display;
        let settings = ffi::SystemSettings {
            nickname: setup.nickname.as_ref(),
            language: setup.language_for(menu).into(),
            country: setup.country_for(menu).as_u8(),
            sensor_bar_above: setup.sensor_bar == SensorBar::Above,
            widescreen: display.aspect == Aspect::SixteenNine,
            progressive: display.progressive,
            pal60: display.pal60,
        };
        Ok(ffi::apply_system_settings(&mut nand.0[..], &settings)?)
    }

    pub fn install_priiloader(&self, nand: &mut Nand, app: &[u8]) -> Result<()> {
        Ok(ffi::install_priiloader(&mut nand.0[..], app)?)
    }

    /// Writes the settings. Hacks are a separate file: see [`Self::set_priiloader_hacks`].
    pub fn configure_priiloader(&self, nand: &mut Nand, config: &Priiloader) -> Result<()> {
        let settings = ffi::PriiloaderSettings {
            autoboot: config.autoboot.into(),
            return_to: config.return_to.into(),
            shutdown_to: config.shutdown_to.into(),
            preferred_mount_point: config.preferred_mount_point.into(),
            stop_disc: config.stop_disc,
            lid_slot_on_error: config.lid_slot_on_error,
            ignore_shutdown_mode: config.ignore_shutdown_mode,
            black_background: !config.white_background,
            dump_gecko_text: config.dump_gecko_text,
            show_rc_updates: config.show_rc_updates,
        };
        Ok(ffi::configure_priiloader(&mut nand.0[..], &settings)?)
    }

    pub fn set_priiloader_hacks(
        &self,
        nand: &mut Nand,
        definitions: &[u8],
        enabled: &[Hack],
    ) -> Result<()> {
        let enabled: BTreeSet<Hack> = enabled.iter().copied().collect();
        let enabled: Vec<ffi::Hack> = enabled.into_iter().map(Into::into).collect();
        Ok(ffi::set_priiloader_hacks(
            &mut nand.0[..],
            definitions,
            &enabled,
        )?)
    }
}

/// Replaces a key footer's text header with the canonical BackupMii one.
/// Returns whether it changed.
pub(crate) fn write_backupmii_header(keys: &mut [u8; KEYS_BYTES]) -> Result<bool> {
    Ok(ffi::write_backupmii_header(keys)?)
}
