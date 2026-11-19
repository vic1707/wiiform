// SPDX-License-Identifier: GPL-2.0-only
//! Files for a storage device: the `sd` and `usb` keys.
mod app;
mod bundle;
mod card_path;
mod extract;

use std::collections::{BTreeMap, btree_map::Entry};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

pub use self::card_path::CardPath;
use self::{app::App, bundle::Bundle};
use crate::blob::Blob;

/// The files of an SD card or a USB drive.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Device")]
struct DeviceFiles {
    /// Homebrew apps. Each key is the folder apps/<key>/ the app goes in.
    #[serde(default)]
    apps: BTreeMap<String, App>,
    /// Anything else, placed from the device's root: packages spanning several
    /// root folders, single files, covers.
    #[serde(default)]
    bundles: Vec<Bundle>,
}

// Every file the entries put on the device, read and checked. No doc comment:
// the schema describes `DeviceFiles`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "DeviceFiles")]
#[schemars(with = "DeviceFiles", rename = "Device")]
pub struct Device(BTreeMap<CardPath, Blob>);

impl Device {
    /// Every file, by path from the device's root.
    pub fn files(&self) -> &BTreeMap<CardPath, Blob> {
        &self.0
    }
}

impl TryFrom<DeviceFiles> for Device {
    type Error = String;

    fn try_from(device: DeviceFiles) -> Result<Self, String> {
        device
            .read()
            .map(Self)
            .map_err(|error| format!("{error:#}"))
    }
}

impl DeviceFiles {
    fn read(self) -> Result<BTreeMap<CardPath, Blob>> {
        let mut files = Vec::new();
        for (name, app) in self.apps {
            files.extend(app.files(&name).with_context(|| format!("app {name}"))?);
        }
        for (index, bundle) in self.bundles.into_iter().enumerate() {
            files.extend(
                bundle
                    .files()
                    .with_context(|| format!("bundles[{index}]"))?,
            );
        }
        let merged = merge(files)?;
        check_fat32(&merged)?;
        Ok(merged)
    }
}

/// The largest file FAT32 holds. The Wii's SD cards and USB drives use FAT32, so
/// no device file may be larger.
const FAT32_FILE_LIMIT: u64 = u32::MAX as u64;

/// Refuses a file FAT32 cannot hold.
pub(crate) fn check_fat32(files: &BTreeMap<CardPath, Blob>) -> Result<()> {
    for (path, blob) in files {
        ensure!(
            blob.size() <= FAT32_FILE_LIMIT,
            "{path} is larger than 4 GiB: FAT32, which the Wii reads, cannot hold it"
        );
    }
    Ok(())
}

/// Merges the entries' files, refusing collisions: FAT ignores case, and a
/// path cannot be both a file and a directory. Contents are compared by
/// SHA-256, and only when two entries share a path.
fn merge(files: Vec<(CardPath, Blob)>) -> Result<BTreeMap<CardPath, Blob>> {
    let mut folded: BTreeMap<String, usize> = BTreeMap::new();
    let mut merged: Vec<(CardPath, Blob)> = Vec::new();
    for (path, blob) in files {
        match folded.entry(path.folded()) {
            Entry::Vacant(slot) => {
                slot.insert(merged.len());
                merged.push((path, blob));
            }
            Entry::Occupied(existing) => {
                let first = &merged[*existing.get()].1;
                ensure!(
                    first.size() == blob.size() && first.sha256()? == blob.sha256()?,
                    "two entries write different files to {path}"
                );
            }
        }
    }
    for path in folded.keys() {
        for (index, _) in path.match_indices('/').skip(1) {
            let parent = &path[..index];
            ensure!(
                !folded.contains_key(parent),
                "{parent} is both a file and a directory"
            );
        }
    }
    Ok(merged.into_iter().collect())
}
