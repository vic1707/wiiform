// SPDX-License-Identifier: GPL-2.0-only
//! RVLoader's boot hook: the `rvloader` key.
mod version;

use anyhow::{Context, Result, ensure};
use derive_more::Deref;
use schemars::JsonSchema;
use serde::Deserialize;

pub use self::version::Version;
use crate::wad::{Sha256, fetch};

/// Where RVLoader keeps the bootloader its Wii-side installer writes to the NAND.
/// The release archives do not carry it.
const REPOSITORY: &str = "https://raw.githubusercontent.com/Aurelio92/RVLoader";
const SIZE_LIMIT: usize = 4 * 1024 * 1024;

/// Boot RVLoader from USB when the console starts. Priiloader starts it, so
/// the recipe needs a `priiloader` section that leaves `autoboot` and
/// `return_to` unset. The RVLoader release itself goes on the USB drive.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Rvloader")]
struct Release {
    /// RVLoader release, such as "2.1".
    version: Version,
    /// SHA-256 of that release's `installer/data/bootloader.dol`.
    sha256: Sha256,
}

// The verified RVLoader bootloader. No doc comment: the schema describes `Release`.
#[derive(derive_more::Debug, Deref, Deserialize, JsonSchema)]
#[debug("Rvloader({} bytes)", _0.len())]
#[serde(try_from = "Release")]
pub struct Rvloader(#[deref(forward)] Vec<u8>);

impl Rvloader {
    /// Downloads the bootloader of a tagged release and checks it.
    pub fn download(version: &Version, sha256: Sha256) -> Result<Self> {
        let url = format!("{REPOSITORY}/v{version}/installer/data/bootloader.dol");
        let bytes = fetch(&url, SIZE_LIMIT)
            .with_context(|| format!("cannot download the RVLoader {version} bootloader"))?;
        ensure!(
            Sha256::of(&bytes) == sha256,
            "RVLoader {version} bootloader does not match its SHA-256"
        );
        Ok(Self(bytes))
    }
}

impl TryFrom<Release> for Rvloader {
    type Error = String;

    fn try_from(release: Release) -> Result<Self, String> {
        Self::download(&release.version, release.sha256).map_err(|error| format!("{error:#}"))
    }
}
