// SPDX-License-Identifier: GPL-2.0-only
//! Files for a storage device: the `sd` and `usb` keys.
mod app;
mod archive;
mod bundle;
mod card_path;
mod extract;

use std::{
    collections::{BTreeMap, btree_map::Entry},
    ops::RangeInclusive,
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

pub use self::card_path::CardPath;
use self::{app::App, bundle::Bundle};
use crate::{source::Source, wad::Sha256};

/// Largest single file, downloaded or unpacked: the size of a NAND.
const FILE_BOUNDS: RangeInclusive<usize> = 1..=0x2100_0000;

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
pub struct Device(BTreeMap<CardPath, Vec<u8>>);

impl Device {
    /// Every file, by path from the device's root.
    pub fn files(&self) -> &BTreeMap<CardPath, Vec<u8>> {
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
    fn read(self) -> Result<BTreeMap<CardPath, Vec<u8>>> {
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
        merge(files)
    }
}

/// Reads a pinned file: `sha256` is required unless a GitHub release states one.
fn read(source: Source, sha256: Option<Sha256>) -> Result<Vec<u8>> {
    let fetched = source.read(FILE_BOUNDS)?;
    let expected = (sha256.or(fetched.published_sha256))
        .context("sha256 is required: only GitHub release assets may come with one")?;
    ensure!(
        Sha256::of(&fetched.bytes) == expected,
        "file does not match its SHA-256"
    );
    Ok(fetched.bytes)
}

/// Merges the entries' files, refusing collisions: FAT ignores case, and a
/// path cannot be both a file and a directory.
fn merge(files: Vec<(CardPath, Vec<u8>)>) -> Result<BTreeMap<CardPath, Vec<u8>>> {
    let mut folded: BTreeMap<String, usize> = BTreeMap::new();
    let mut merged: Vec<(CardPath, Vec<u8>)> = Vec::new();
    for (path, bytes) in files {
        match folded.entry(path.folded()) {
            Entry::Vacant(slot) => {
                slot.insert(merged.len());
                merged.push((path, bytes));
            }
            Entry::Occupied(existing) => ensure!(
                merged[*existing.get()].1 == bytes,
                "two entries write different files to {path}"
            ),
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
