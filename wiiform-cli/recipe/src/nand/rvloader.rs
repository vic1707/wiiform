// SPDX-License-Identifier: GPL-2.0-only
//! RVLoader as its installer sets it up: the `rvloader` key.
mod dol;
mod ios;

use anyhow::{Context, Result, bail, ensure};
use derive_more::Deref;
use schemars::JsonSchema;
use serde::Deserialize;

#[cfg(test)]
pub(crate) use self::dol::sample as sample_dol;
pub use self::ios::Ios;
use crate::{archive, source::Pinned};

/// RVLoader's installer, in a release archive. It carries the bootloader as data.
const INSTALLER: &str = "apps/RVLoader/installer.dol";
/// The Wi-Fi driver a release archive carries, the one its installer writes.
const WIFI_MODULE: &str = "rvloader/Hiidra/modules/wd.bin";
/// What the bootloader prints when it starts (bootloader/source/main.cpp), which
/// tells it apart from the other DOLs the installer carries.
const BOOTLOADER_MARKER: &[u8] = b"RVLoader bootloader";

/// What RVLoader's installer does, done offline: Priiloader starts RVLoader's
/// bootloader, which loads RVLoader from USB, and the IOS slots 236, 240 and
/// 241 get copies of IOS36 (patched), IOS58 and IOS80 without the stock Wi-Fi
/// modules. The recipe needs a `priiloader` section without `autoboot` and
/// `return_to`. The release itself goes on the USB drive as a bundle.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Rvloader")]
pub struct Release {
    /// The RVLoader release archive, such as RVLoader_v2.1.zip, upstream's or a
    /// fork's. The bootloader comes from the installer inside it,
    /// `apps/RVLoader/installer.dol`, and the Wi-Fi driver from
    /// `rvloader/Hiidra/modules/wd.bin`.
    release: Pinned,
    /// The installer's "Patch out WiFi": IOS36, IOS58 and IOS80 lose their
    /// Wi-Fi modules too. Either way they are reinstalled as the installer does.
    pub patch_out_wifi: bool,
    /// The installer's "Enable VGA". Needs `setup.display.progressive`, and
    /// `pal60` for 4.3E. Component video then stops working.
    pub vga: bool,
}

/// The `install-priiloader` flags for RVLoader.
#[cfg(feature = "cli")]
#[derive(Debug, clap::Args)]
pub struct Arguments {
    // clap still requires the fields of an optional flattened struct, so the
    // command line spells out which flags go together.
    /// Local path or HTTPS URL of an RVLoader release archive, such as
    /// RVLoader_v2.1.zip.
    #[arg(id = "rvloader", long = "rvloader", value_name = "RELEASE")]
    #[arg(required = false, requires = "rvloader_sha256")]
    #[arg(conflicts_with_all = ["autoboot", "return_to"])]
    pub release: std::path::PathBuf,
    /// SHA-256 of that archive.
    #[arg(
        id = "rvloader_sha256",
        long = "rvloader-sha256",
        value_name = "SHA256"
    )]
    #[arg(required = false, requires = "rvloader")]
    pub sha256: crate::Sha256,
    /// The installer's "Patch out WiFi".
    #[arg(long, requires = "rvloader")]
    pub patch_out_wifi: bool,
    /// The installer's "Enable VGA".
    #[arg(long, requires = "rvloader")]
    pub vga: bool,
}

/// One file the RVLoader installer writes, taken from its release.
#[derive(derive_more::Debug, Deref)]
#[debug("{} bytes", _0.len())]
pub struct ReleaseFile(#[deref(forward)] Vec<u8>);

// The release's files and the options. No doc comment: the schema describes `Release`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "Release")]
#[schemars(with = "Release")]
pub struct Rvloader {
    pub bootloader: ReleaseFile,
    pub wifi_module: ReleaseFile,
    pub patch_out_wifi: bool,
    pub vga: bool,
}

/// The one file of the archive at `path`, under the release's top folder or not.
fn member<'a>(files: &'a [(String, Vec<u8>)], path: &str) -> Result<&'a [u8]> {
    let found: Vec<&[u8]> = (files.iter())
        .filter(|(name, _)| matches_path(name, path))
        .map(|(_, bytes)| &bytes[..])
        .collect();
    match found[..] {
        [bytes] => Ok(bytes),
        [] => bail!("the release holds no {path}"),
        _ => bail!("the release holds {} files at {path}", found.len()),
    }
}

fn matches_path(name: &str, path: &str) -> bool {
    name.strip_suffix(path)
        .is_some_and(|folder| folder.is_empty() || folder.ends_with('/'))
}

impl Rvloader {
    /// Takes the installer's files out of a release archive: the bootloader is the
    /// one DOL inside the installer that names itself.
    pub fn from_release(release: &[u8], patch_out_wifi: bool, vga: bool) -> Result<Self> {
        let files = archive::files_where(release, |name| {
            matches_path(name, INSTALLER) || matches_path(name, WIFI_MODULE)
        })?;
        let installer = member(&files, INSTALLER)?;
        let wifi_module = member(&files, WIFI_MODULE)?;
        let contains =
            |bytes: &[u8], wanted: &[u8]| bytes.windows(wanted.len()).any(|at| at == wanted);
        let bootloaders: Vec<&[u8]> = dol::embedded(installer)
            .filter(|dol| contains(dol, BOOTLOADER_MARKER))
            .collect();
        let [bootloader] = bootloaders[..] else {
            bail!(
                "{INSTALLER} carries {} DOLs naming themselves RVLoader's bootloader, not one",
                bootloaders.len()
            );
        };
        ensure!(
            !wifi_module.is_empty() && contains(installer, wifi_module),
            "{WIFI_MODULE} is not the Wi-Fi driver {INSTALLER} carries"
        );
        Ok(Self {
            bootloader: ReleaseFile(bootloader.to_vec()),
            wifi_module: ReleaseFile(wifi_module.to_vec()),
            patch_out_wifi,
            vga,
        })
    }

    /// The IOS the installer writes, in its order.
    pub fn ios(&self) -> [Ios; 6] {
        Ios::installed(self.patch_out_wifi)
    }
}

impl TryFrom<Release> for Rvloader {
    type Error = anyhow::Error;

    fn try_from(release: Release) -> Result<Self> {
        let archive = release.release.read()?;
        Self::from_release(&archive, release.patch_out_wifi, release.vga)
            .context("cannot read the RVLoader release")
    }
}

#[cfg(feature = "cli")]
impl TryFrom<Arguments> for Rvloader {
    type Error = anyhow::Error;

    fn try_from(arguments: Arguments) -> Result<Self> {
        let archive = crate::source::Source::from_command_line(arguments.release)?
            .read_checked(Some(arguments.sha256), crate::source::MEMORY_BOUNDS)?;
        Self::from_release(&archive, arguments.patch_out_wifi, arguments.vga)
            .context("cannot read the RVLoader release")
    }
}
