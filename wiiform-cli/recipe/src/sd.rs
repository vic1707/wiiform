// SPDX-License-Identifier: GPL-2.0-only
//! Files for the SD card: the `sd` key.
mod card_path;

use std::{
    collections::{BTreeMap, btree_map::Entry},
    io::{Cursor, Read},
    ops::RangeInclusive,
};

use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, de::IgnoredAny};

pub use self::card_path::CardPath;
use crate::{source::Source, wad::Sha256};

/// Largest single file, downloaded or unpacked: the size of a NAND.
const FILE_BOUNDS: RangeInclusive<usize> = 1..=0x2100_0000;

/// Where a file goes on the card.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Placement {
    /// Unpack the file, a zip, into this card directory. Homebrew zips usually
    /// hold apps/<name>/, so / is the common choice.
    Unzip(CardPath),
    /// Copy the file to this card path.
    To(CardPath),
}

/// A file for the SD card: one source, where it goes, and its SHA-256.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "SdFile", deny_unknown_fields)]
struct SdFile {
    #[serde(flatten)]
    source: Source,
    /// SHA-256 of the exact file. Optional for a GitHub release asset whose
    /// release states one; required otherwise.
    sha256: Option<Sha256>,
    #[serde(flatten)]
    placement: Placement,
    // Serde cannot reject unknown keys next to flattened fields; they land here.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: BTreeMap<String, IgnoredAny>,
}

// The card files one `sd` entry produces, read and checked. No doc comment:
// the schema describes `SdFile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "SdFile")]
#[schemars(with = "SdFile", rename = "SdFile")]
pub struct File(BTreeMap<CardPath, Vec<u8>>);

impl File {
    /// Every file this entry writes, by card path.
    pub fn files(&self) -> &BTreeMap<CardPath, Vec<u8>> {
        &self.0
    }
}

impl TryFrom<SdFile> for File {
    type Error = String;

    fn try_from(entry: SdFile) -> Result<Self, String> {
        entry.read().map(Self).map_err(|error| format!("{error:#}"))
    }
}

impl SdFile {
    fn read(self) -> Result<BTreeMap<CardPath, Vec<u8>>> {
        if let Some(key) = self.unknown.keys().next() {
            bail!("unknown field `{key}`");
        }
        let fetched = self.source.read(FILE_BOUNDS)?;
        let expected = (self.sha256.or(fetched.published_sha256))
            .context("sha256 is required: only GitHub release assets may come with one")?;
        ensure!(
            Sha256::of(&fetched.bytes) == expected,
            "file does not match its SHA-256"
        );
        match self.placement {
            Placement::To(path) => {
                ensure!(!path.is_root(), "`to` needs a file path, not the card root");
                Ok(BTreeMap::from([(path, fetched.bytes)]))
            }
            Placement::Unzip(directory) => unzip(&fetched.bytes, &directory),
        }
    }
}

fn unzip(bytes: &[u8], directory: &CardPath) -> Result<BTreeMap<CardPath, Vec<u8>>> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).context("`unzip` needs a zip file")?;
    let mut files = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        // macOS metadata that archivers on that system add.
        if name.starts_with("__MACOSX/")
            || name
                .rsplit('/')
                .next()
                .is_some_and(|base| base.starts_with("._"))
        {
            continue;
        }
        if entry.is_dir() {
            continue;
        }
        ensure!(
            entry.is_file() && !entry.is_symlink(),
            "zip entry {name} is not a regular file"
        );
        ensure!(
            entry.size() <= *FILE_BOUNDS.end() as u64,
            "zip entry {name} is too large"
        );
        let path = directory
            .join(&name)
            .with_context(|| format!("zip entry {name}"))?;
        let mut contents = Vec::with_capacity(entry.size() as usize);
        (&mut entry)
            .take(*FILE_BOUNDS.end() as u64)
            .read_to_end(&mut contents)?;
        ensure!(
            files.insert(path, contents).is_none(),
            "zip lists {name} twice"
        );
    }
    ensure!(!files.is_empty(), "zip holds no files");
    Ok(files)
}

/// Refuses entries that would collide on the card: FAT ignores case, the root
/// already holds the NAND and its keys, and a path cannot be both a file and
/// a directory.
pub(crate) fn check_conflicts(entries: &[File]) -> Result<()> {
    let folded = |path: &CardPath| path.to_string().to_lowercase();
    let mut planned: BTreeMap<String, &[u8]> = BTreeMap::new();
    for (path, bytes) in entries.iter().flat_map(|entry| entry.files()) {
        let key = folded(path);
        ensure!(
            key != "/nand.bin" && key != "/keys.bin",
            "{path} is where wiiform puts the NAND and its keys"
        );
        match planned.entry(key) {
            Entry::Vacant(slot) => {
                slot.insert(bytes);
            }
            Entry::Occupied(existing) => ensure!(
                *existing.get() == bytes.as_slice(),
                "sd entries write different files to {path}"
            ),
        }
    }
    for path in planned.keys() {
        for (index, _) in path.match_indices('/').skip(1) {
            let parent = &path[..index];
            ensure!(
                !planned.contains_key(parent),
                "sd entries use {parent} as both a file and a directory"
            );
        }
    }
    Ok(())
}
