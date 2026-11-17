// SPDX-License-Identifier: GPL-2.0-only
//! RVLoader as its installer sets it up: the `rvloader` key.
mod ios;
mod version;

use anyhow::{Context, Result, ensure};
use derive_more::Deref;
use schemars::JsonSchema;
use serde::Deserialize;

pub use self::{ios::Ios, version::Version};
use crate::wad::{Sha256, fetch};

/// Where RVLoader keeps the files its Wii-side installer writes to the NAND.
/// The release archives do not carry them.
const REPOSITORY: &str = "https://raw.githubusercontent.com/Aurelio92/RVLoader";
const SIZE_LIMIT: usize = 4 * 1024 * 1024;

/// What RVLoader's installer does, done offline: Priiloader starts RVLoader's
/// bootloader, which loads RVLoader from USB, and the IOS slots 236, 240 and
/// 241 get copies of IOS36 (patched), IOS58 and IOS80 without the stock Wi-Fi
/// modules. The recipe needs a `priiloader` section without `autoboot` and
/// `return_to`. The RVLoader release itself goes on the USB drive.
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Rvloader")]
pub struct Release {
    // clap still requires the fields of an optional flattened struct, so the
    // command line spells out which flags go together.
    /// RVLoader release, such as "2.1".
    #[cfg_attr(
        feature = "cli",
        arg(id = "rvloader", long = "rvloader", value_name = "VERSION")
    )]
    #[cfg_attr(feature = "cli", arg(required = false, requires_all = ["rvloader_sha256", "rvloader_wd_sha256"]))]
    #[cfg_attr(feature = "cli", arg(conflicts_with_all = ["autoboot", "return_to"]))]
    pub version: Version,
    /// SHA-256 of that release's `installer/data/bootloader.dol`.
    #[cfg_attr(
        feature = "cli",
        arg(
            id = "rvloader_sha256",
            long = "rvloader-sha256",
            value_name = "SHA256"
        )
    )]
    #[cfg_attr(feature = "cli", arg(required = false, requires = "rvloader"))]
    pub sha256: Sha256,
    /// SHA-256 of that release's `installer/data/wd.bin`, the Wi-Fi driver
    /// replacing the stock Wi-Fi modules.
    #[cfg_attr(
        feature = "cli",
        arg(
            id = "rvloader_wd_sha256",
            long = "rvloader-wd-sha256",
            value_name = "SHA256"
        )
    )]
    #[cfg_attr(feature = "cli", arg(required = false, requires = "rvloader"))]
    pub wd_sha256: Sha256,
    /// The installer's "Patch out WiFi": IOS36, IOS58 and IOS80 lose their
    /// Wi-Fi modules too. Either way they are reinstalled as the installer does.
    #[cfg_attr(feature = "cli", arg(long, requires = "rvloader"))]
    pub patch_out_wifi: bool,
    /// The installer's "Enable VGA". Needs `setup.display.progressive`, and
    /// `pal60` for 4.3E. Component video then stops working.
    #[cfg_attr(feature = "cli", arg(long, requires = "rvloader"))]
    pub vga: bool,
}

/// One file of an RVLoader release, downloaded and checked.
#[derive(derive_more::Debug, Deref)]
#[debug("{} bytes", _0.len())]
pub struct ReleaseFile(#[deref(forward)] Vec<u8>);

impl ReleaseFile {
    fn download(version: &Version, name: &str, sha256: Sha256) -> Result<Self> {
        let url = format!("{REPOSITORY}/v{version}/installer/data/{name}");
        let bytes = fetch(&url, SIZE_LIMIT)
            .with_context(|| format!("cannot download RVLoader {version} {name}"))?;
        ensure!(
            Sha256::of(&bytes) == sha256,
            "RVLoader {version} {name} does not match its SHA-256"
        );
        Ok(Self(bytes))
    }
}

// The verified RVLoader files and options. No doc comment: the schema describes `Release`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "Release")]
#[schemars(with = "Release")]
pub struct Rvloader {
    pub bootloader: ReleaseFile,
    pub wifi_module: ReleaseFile,
    pub patch_out_wifi: bool,
    pub vga: bool,
}

impl Rvloader {
    /// The IOS the installer writes, in its order.
    pub fn ios(&self) -> [Ios; 6] {
        Ios::installed(self.patch_out_wifi)
    }
}

impl TryFrom<Release> for Rvloader {
    type Error = anyhow::Error;

    fn try_from(release: Release) -> Result<Self> {
        let version = &release.version;
        Ok(Self {
            bootloader: ReleaseFile::download(version, "bootloader.dol", release.sha256)?,
            wifi_module: ReleaseFile::download(version, "wd.bin", release.wd_sha256)?,
            patch_out_wifi: release.patch_out_wifi,
            vga: release.vga,
        })
    }
}
