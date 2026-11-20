// SPDX-License-Identifier: GPL-2.0-only
//! Files for a storage device: the `sd` and `usb` keys.
mod app;
mod bootmii;
mod bundle;
mod card_path;
mod extract;
mod rvloader;

use std::{
    collections::{BTreeMap, HashSet, btree_map::Entry},
    fmt,
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};

pub use self::card_path::CardPath;
use self::{app::App, bootmii::BootmiiIni, bundle::Bundle, rvloader::RvloaderSettings};
use crate::{blob::Blob, source};

/// What happens when two entries write the same path. Paths are compared ignoring
/// case, as FAT does. The entries are taken in recipe order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Collisions {
    /// The later entry replaces the earlier one.
    #[default]
    Overwrite,
    /// The earlier entry stays and the later one is ignored.
    KeepFirst,
    /// Two entries writing different files to one path stop the build.
    Error,
}

/// The files of an SD card or a USB drive.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Device", deny_unknown_fields)]
#[allow(dead_code)] // Only the schema uses it.
struct DeviceFiles {
    /// Homebrew apps. Each key is the folder apps/<key>/ the app goes in.
    #[serde(default)]
    apps: BTreeMap<String, App>,
    /// Anything else, placed from the device's root: packages spanning several
    /// root folders, single files, covers.
    #[serde(default)]
    bundles: Vec<Bundle>,
    /// BootMii's settings, written to /bootmii/bootmii.ini. BootMii reads that
    /// file from the SD card only, so this goes under `sd`.
    #[serde(default)]
    bootmii: Option<BootmiiIni>,
    /// RVLoader's settings for games, written to /rvloader/configs/<ID>.cfg.
    /// RVLoader reads them from the USB drive, so this goes under `usb`.
    #[serde(default)]
    rvloader: Option<RvloaderSettings>,
}

// Every file the entries put on the device, read and checked. Each app and
// bundle is read as the recipe reaches it, so downloads and extraction follow
// the recipe's order, and collisions are settled in that order too. No doc
// comment: the schema describes `DeviceFiles`.
#[derive(Debug, JsonSchema)]
#[schemars(with = "DeviceFiles", rename = "Device")]
pub struct Device {
    files: BTreeMap<CardPath, Blob>,
    notices: Vec<String>,
    bootmii: bool,
    rvloader: bool,
}

impl Device {
    /// Every file, by path from the device's root.
    pub fn files(&self) -> &BTreeMap<CardPath, Blob> {
        &self.files
    }

    /// One line per file that entries wrote more than once, naming the entry
    /// that won and the ones it displaced (or those ignored).
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// Whether the recipe sets BootMii's `bootmii.ini` on this device.
    pub fn has_bootmii(&self) -> bool {
        self.bootmii
    }

    /// Whether the recipe sets RVLoader's game settings on this device.
    pub fn has_rvloader(&self) -> bool {
        self.rvloader
    }
}

impl<'de> Deserialize<'de> for Device {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(DeviceVisitor)
    }
}

struct DeviceVisitor;

impl<'de> Visitor<'de> for DeviceVisitor {
    type Value = Device;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a device with apps and bundles")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Device, A::Error> {
        let mut entries = Vec::new();
        let mut keys = HashSet::new();
        let (mut bootmii, mut rvloader) = (false, false);
        while let Some(key) = map.next_key::<String>()? {
            ensure_custom::<A::Error>(keys.insert(key.clone()), || {
                format!("{key} is listed twice")
            })?;
            match key.as_str() {
                "apps" => map.next_value_seed(Apps(&mut entries))?,
                "bundles" => map.next_value_seed(Bundles(&mut entries))?,
                "bootmii" => {
                    let settings: BootmiiIni = map.next_value()?;
                    let file = settings.file().map_err(fail)?;
                    push(
                        &mut entries,
                        "bootmii settings",
                        generated(vec![file]).map_err(fail)?,
                    );
                    bootmii = true;
                }
                "rvloader" => {
                    let settings: RvloaderSettings = map.next_value()?;
                    let files = settings.files().map_err(fail)?;
                    push(
                        &mut entries,
                        "rvloader settings",
                        generated(files).map_err(fail)?,
                    );
                    rvloader = true;
                }
                _ => {
                    return Err(de::Error::unknown_field(
                        &key,
                        &["apps", "bundles", "bootmii", "rvloader"],
                    ));
                }
            }
        }
        let (files, notices) = resolve(entries, source::collisions()).map_err(fail)?;
        Ok(Device {
            files,
            notices,
            bootmii,
            rvloader,
        })
    }
}

/// Puts generated contents in the store, as device files at their paths.
fn generated(files: Vec<(CardPath, Vec<u8>)>) -> Result<Vec<(CardPath, Blob)>> {
    files
        .into_iter()
        .map(|(path, bytes)| {
            let staged = source::store_bytes(&bytes)?;
            Ok((path, Blob::file(staged)?))
        })
        .collect()
}

/// One file an entry writes, and the app or bundle it came from.
struct FileEntry {
    path: CardPath,
    blob: Blob,
    source: String,
}

fn push(entries: &mut Vec<FileEntry>, source: &str, files: Vec<(CardPath, Blob)>) {
    entries.extend(files.into_iter().map(|(path, blob)| FileEntry {
        path,
        blob,
        source: source.to_owned(),
    }));
}

/// The `apps` mapping, its apps read in the order listed.
struct Apps<'a>(&'a mut Vec<FileEntry>);

impl<'de> DeserializeSeed<'de> for Apps<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Apps<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("apps, each keyed by its folder name")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut names = HashSet::new();
        while let Some(name) = map.next_key::<String>()? {
            ensure_custom::<A::Error>(names.insert(name.clone()), || {
                format!("app {name} is listed twice")
            })?;
            let app = map.next_value::<App>()?;
            let files = app
                .files(&name)
                .with_context(|| format!("app {name}"))
                .map_err(fail)?;
            push(self.0, &format!("app {name}"), files);
        }
        Ok(())
    }
}

/// The `bundles` list, its entries read in the order listed.
struct Bundles<'a>(&'a mut Vec<FileEntry>);

impl<'de> DeserializeSeed<'de> for Bundles<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Bundles<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("bundles, a list")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut index = 0;
        while let Some(bundle) = seq.next_element::<Bundle>()? {
            let files = bundle
                .files()
                .with_context(|| format!("bundles[{index}]"))
                .map_err(fail)?;
            push(self.0, &format!("bundles[{index}]"), files);
            index += 1;
        }
        Ok(())
    }
}

fn fail<E: de::Error>(error: anyhow::Error) -> E {
    E::custom(format!("{error:#}"))
}

fn ensure_custom<E: de::Error>(condition: bool, message: impl FnOnce() -> String) -> Result<(), E> {
    if condition {
        Ok(())
    } else {
        Err(E::custom(message()))
    }
}

/// The largest file FAT32 holds. The Wii's SD cards and USB drives use FAT32, so
/// no device file may be larger.
const FAT32_FILE_LIMIT: u64 = u32::MAX as u64;

/// Applies the collision policy to the entries, in recipe order, then checks the
/// result: a path cannot be both a file and a directory, and FAT32 cannot hold
/// a file of 4 GiB or more. Returns the files and one notice per path that more
/// than one entry wrote (none under `error`, which stops on a difference).
fn resolve(
    entries: Vec<FileEntry>,
    collisions: Collisions,
) -> Result<(BTreeMap<CardPath, Blob>, Vec<String>)> {
    let mut folded: BTreeMap<String, usize> = BTreeMap::new();
    let mut merged: Vec<FileEntry> = Vec::new();
    // Every source that wrote each merged path, in recipe order.
    let mut sources: Vec<Vec<String>> = Vec::new();
    for entry in entries {
        match folded.entry(entry.path.folded()) {
            Entry::Vacant(slot) => {
                slot.insert(merged.len());
                sources.push(vec![entry.source.clone()]);
                merged.push(entry);
            }
            Entry::Occupied(existing) => {
                let index = *existing.get();
                sources[index].push(entry.source.clone());
                match collisions {
                    Collisions::Overwrite => merged[index] = entry,
                    Collisions::KeepFirst => {}
                    Collisions::Error => {
                        let first = &merged[index].blob;
                        ensure!(
                            first.size() == entry.blob.size()
                                && first.sha256()? == entry.blob.sha256()?,
                            "two entries write different files to {}",
                            entry.path
                        );
                    }
                }
            }
        }
    }
    let mut notices = Vec::new();
    if collisions != Collisions::Error {
        for (entry, sources) in merged.iter().zip(&sources) {
            if sources.len() < 2 {
                continue;
            }
            notices.push(match collisions {
                Collisions::KeepFirst => format!(
                    "{}: kept from {}; {} ignored",
                    entry.path,
                    sources[0],
                    sources[1..].join(", ")
                ),
                _ => format!(
                    "{}: from {} replaces {}",
                    entry.path,
                    sources[sources.len() - 1],
                    sources[..sources.len() - 1].join(", ")
                ),
            });
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
    let files: BTreeMap<CardPath, Blob> = merged
        .into_iter()
        .map(|entry| (entry.path, entry.blob))
        .collect();
    check_fat32(&files)?;
    Ok((files, notices))
}

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
