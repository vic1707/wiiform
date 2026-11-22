// SPDX-License-Identifier: GPL-2.0-only
//! Typed access to the Wiiform C++ library.
//!
//! Everything here translates between recipe types and the bridge; decisions
//! about what to build live in [`crate::workflow`].
use std::{collections::BTreeSet, path::Path};

use anyhow::{Context, Result, ensure};

use crate::ffi::bridge as ffi;
use crate::recipe::{
    Menu, Priiloader, Region, Setup, TitleId, Wad,
    nand::{
        priiloader,
        priiloader::Hack,
        rvloader,
        setup::{self, SensorBar, display::Aspect},
        titles::cios,
    },
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
    setup::Language => ffi::Language { Ja, En, De, Fr, Es, It, Nl }
    priiloader::Autoboot => ffi::Autoboot { Disabled, HomebrewChannel, BootmiiIos, SystemMenu, File }
    priiloader::ReturnTo => ffi::ReturnTo { SystemMenu, Priiloader, Autoboot, File }
    priiloader::ShutdownTo => ffi::ShutdownTo { None, Priiloader, Autoboot }
    priiloader::MountPoint => ffi::MountPoint { Auto, Sd, Usb }
    Hack => ffi::Hack {
        Fix480p, BlockOnlineUpdates, BlockDiscUpdates, BlockDiscAutoboot, AutoPressA,
        RemoveDeflicker, MoveDiscChannel, RemoveNocopySaveProtection, ReplaceHealthScreen,
        Wiimmfi, RegionFreeEverything, ReEnableBannerbomb, ForceDataManagement,
        ForceRecoveryMode, RemoveDiagnosticDiscCheck, NoDeleteHomebrew, UnblockServiceDiscs,
        RemoveIos16DiscError, MarkNetworkTested,
    }
    priiloader::BackgroundMusic => ffi::Hack { NoBackgroundMusic, NoSfxAndBackgroundMusic }
    priiloader::UsbGecko => ffi::Hack { UsbGeckoReport, UsbGeckoReportGeckoOs }
    priiloader::DiscIos => ffi::Hack {
        ForceDiscIos248, ForceDiscIos249, ForceDiscIos250, ForceDiscIos251,
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

impl NusCache {
    fn ffi(&self) -> Result<ffi::NusCache<'_>> {
        Ok(match self {
            Self::Memory => ffi::NusCache {
                directory: "",
                offline: false,
            },
            Self::Directory { path, offline } => {
                let directory = path.to_str().with_context(|| {
                    format!("NUS cache directory must be UTF-8: {}", path.display())
                })?;
                // The bridge spells "no cache" as an empty directory.
                ensure!(!directory.is_empty(), "NUS cache directory is empty");
                ffi::NusCache {
                    directory,
                    offline: *offline,
                }
            }
        })
    }
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

/// Reads a seed from a backup. A footerless backup needs its key sidecar.
pub fn extract_seed(nand: &[u8], sidecar_keys: Option<&[u8; KEYS_BYTES]>) -> Result<ExtractedSeed> {
    let sidecar_keys = sidecar_keys.map_or(&[][..], |keys| keys);
    Ok(ExtractedSeed(ffi::extract_seed(nand, sidecar_keys)?))
}

/// The titles of a factory-fresh console, in installation order.
pub fn stock_titles(menu: Menu) -> Result<Vec<(TitleId, u16)>> {
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

/// Operations of the library on an image. The library itself refuses calls
/// from a second thread.
impl Nand {
    pub fn initialize_image(
        &mut self,
        seed: &StockSeed,
        menu: Menu,
        policy: BootPolicy,
    ) -> Result<StockIdentity> {
        let initialize = match policy {
            BootPolicy::Stock => ffi::initialize_image,
            BootPolicy::PreserveSource => ffi::initialize_image_with_source_boot,
        };
        let identity = match initialize(&mut self.0[..], &seed.as_native(), menu.into()) {
            Ok(identity) => identity,
            // The stock policy refuses BootMii as boot2, which is what the seed records.
            Err(error)
                if matches!(policy, BootPolicy::Stock)
                    && seed.bootmii().boot2 == crate::seed::Boot2::Detected =>
            {
                return Err(anyhow::Error::from(error).context(
                    "the console runs BootMii as boot2, which the stock boot policy refuses: \
                     pass --boot-policy preserve-source to keep it",
                ));
            }
            Err(error) => return Err(error.into()),
        };
        Ok(StockIdentity(identity))
    }

    pub fn verify_stock(&self, seed: &StockSeed, menu: Menu) -> Result<()> {
        Ok(ffi::verify_stock(
            self.as_bytes(),
            &seed.as_native(),
            menu.into(),
        )?)
    }

    pub fn verify_generated_nand(&self, expected: &StockIdentity) -> Result<()> {
        Ok(ffi::verify_generated_nand(self.as_bytes(), &expected.0)?)
    }

    pub fn install_nus_title(
        &mut self,
        cache: &NusCache,
        title_id: TitleId,
        version: u16,
    ) -> Result<InstalledWad> {
        let title = ffi::TitleVersion {
            id: title_id.as_u64(),
            version,
        };
        ffi::install_nus_title(&mut self.0[..], &cache.ffi()?, title)?.try_into()
    }

    /// Installs one IOS of RVLoader's installer, derived from a stock IOS downloaded like
    /// [`Self::install_nus_title`].
    pub fn install_rvloader_ios(
        &mut self,
        cache: &NusCache,
        ios: &rvloader::Ios,
        wifi_module: &[u8],
    ) -> Result<InstalledWad> {
        let ios = ffi::RvloaderIos {
            source: ffi::TitleVersion {
                id: ios.source.as_u64(),
                version: ios.version,
            },
            slot: ios.slot,
            es_patches: ios.es_patches,
            wifi_module: if ios.without_wifi { wifi_module } else { &[] },
        };
        ffi::install_rvloader_ios(&mut self.0[..], &cache.ffi()?, &ios)?.try_into()
    }

    /// Creates or removes the file that makes RVLoader's bootloader enable VGA output.
    pub fn set_rvloader_vga(&mut self, enabled: bool) -> Result<()> {
        Ok(ffi::set_rvloader_vga(&mut self.0[..], enabled)?)
    }

    /// Installs a Nintendo-signed channel or IOS.
    pub fn install_wad(&mut self, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_wad(&mut self.0[..], wad)?.try_into()
    }

    /// Installs a downloadable channel that need not be signed.
    pub fn install_unsigned_channel(&mut self, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_unsigned_channel(&mut self.0[..], wad)?.try_into()
    }

    pub fn install_homebrew_channel(&mut self, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_homebrew_channel(&mut self.0[..], wad)?.try_into()
    }

    pub fn install_cios(&mut self, wad: &Wad, target: &cios::Target) -> Result<InstalledWad> {
        ffi::install_cios(&mut self.0[..], wad, &cios_options(target))?.try_into()
    }

    /// Builds a d2x cIOS from its NUS base IOS and installs it like [`Self::install_cios`].
    pub fn install_built_cios(
        &mut self,
        cache: &NusCache,
        build: &cios::Build,
        target: &cios::Target,
    ) -> Result<InstalledWad> {
        let build = ffi::CiosBuild {
            base_version: build.base_version,
            options: cios_options(target),
            patches: (build.patches.iter())
                .map(|patch| ffi::ContentPatch {
                    cid: patch.cid,
                    offset: patch.offset,
                    original: patch.original.clone(),
                    replacement: patch.replacement.clone(),
                })
                .collect(),
            modules: (build.modules.iter())
                .map(|module| ffi::AddedModule {
                    cid: module.cid,
                    position: module.position.map_or(-1, i32::from),
                    bytes: module.bytes.clone(),
                })
                .collect(),
        };
        ffi::install_built_cios(&mut self.0[..], &cache.ffi()?, &build)?.try_into()
    }

    /// Installs BootMii as IOS from the parts a seed captured.
    pub fn install_bootmii_ios(&mut self, ios: &crate::seed::BootmiiIos) -> Result<InstalledWad> {
        ffi::install_bootmii_ios(&mut self.0[..], &ios.tmd, &ios.ticket, &ios.content)?.try_into()
    }

    /// Installs a Nintendo-signed channel; IOS WADs are refused.
    pub fn install_channel(&mut self, wad: &Wad) -> Result<InstalledWad> {
        ffi::install_channel(&mut self.0[..], wad)?.try_into()
    }

    pub fn remove_title(&mut self, title_id: TitleId) -> Result<()> {
        Ok(ffi::remove_title(&mut self.0[..], title_id.as_u64())?)
    }

    pub fn apply_system_settings(&mut self, setup: &Setup) -> Result<()> {
        let display = setup.display;
        let settings = ffi::SystemSettings {
            nickname: setup.nickname.as_ref(),
            language: setup.language.into(),
            country: setup.country.as_u8(),
            sensor_bar_above: setup.sensor_bar == SensorBar::Above,
            widescreen: display.aspect == Aspect::SixteenNine,
            progressive: display.progressive,
            pal60: display.pal60,
        };
        Ok(ffi::apply_system_settings(&mut self.0[..], &settings)?)
    }

    pub fn install_priiloader(&mut self, app: &[u8]) -> Result<()> {
        Ok(ffi::install_priiloader(&mut self.0[..], app)?)
    }

    /// Stores the executable that Priiloader's file destinations boot.
    pub fn install_priiloader_boot_file(
        &mut self,
        dol: &[u8],
        hardware_access: bool,
    ) -> Result<()> {
        Ok(ffi::install_priiloader_boot_file(
            &mut self.0[..],
            dol,
            hardware_access,
        )?)
    }

    /// Writes the settings. Hacks are a separate file: see [`Self::set_priiloader_hacks`].
    pub fn configure_priiloader(&mut self, config: &Priiloader) -> Result<()> {
        let settings = ffi::PriiloaderSettings {
            autoboot: config.autoboot.unwrap_or_default().into(),
            return_to: config.return_to.unwrap_or_default().into(),
            shutdown_to: config.shutdown_to.into(),
            preferred_mount_point: config.preferred_mount_point.into(),
            stop_disc: config.stop_disc,
            lid_slot_on_error: config.lid_slot_on_error,
            ignore_shutdown_mode: config.ignore_shutdown_mode,
            black_background: !config.white_background,
            dump_gecko_text: config.dump_gecko_text,
            show_rc_updates: config.show_rc_updates,
        };
        Ok(ffi::configure_priiloader(&mut self.0[..], &settings)?)
    }

    /// Enables the hacks the settings list, and no others.
    pub fn set_priiloader_hacks(&mut self, definitions: &[u8], config: &Priiloader) -> Result<()> {
        let standalone: BTreeSet<Hack> = config.hacks.iter().copied().collect();
        let mut enabled: Vec<ffi::Hack> = standalone.into_iter().map(ffi::Hack::from).collect();
        enabled.extend(config.background_music.map(ffi::Hack::from));
        enabled.extend(config.usb_gecko.map(ffi::Hack::from));
        enabled.extend(config.force_disc_ios.map(ffi::Hack::from));
        Ok(ffi::set_priiloader_hacks(
            &mut self.0[..],
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

fn cios_options(target: &cios::Target) -> ffi::CiosOptions {
    ffi::CiosOptions {
        slot: target.slot.into(),
        base: target.base.into(),
        revision: target.revision,
        replace_stock_stub: target.replace_stock_stub,
    }
}
