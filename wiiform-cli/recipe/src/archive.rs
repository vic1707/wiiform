// SPDX-License-Identifier: GPL-2.0-only
//! The files of a zip archive: on disk for device files, in memory for the small
//! releases read whole.
use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read, Seek},
    path::Path,
};

use anyhow::{Context, Result, ensure};

use crate::blob::Blob;

/// One file of an archive: its path inside it, its index and its size.
struct Listed {
    name: String,
    index: usize,
    size: u64,
}

/// The archive's files. Directories are implied by the paths; macOS metadata
/// (`__MACOSX/`, `._*`) is left out.
fn list<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<Vec<Listed>> {
    let mut files = Vec::new();
    let mut names = HashSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let name = entry.name()?.into_owned();
        let base = name.rsplit('/').next().unwrap_or_default();
        if name.starts_with("__MACOSX/") || base.starts_with("._") || entry.is_dir() {
            continue;
        }
        ensure!(
            entry.is_file() && !entry.is_symlink(),
            "zip entry {name} is not a regular file"
        );
        ensure!(names.insert(name.clone()), "zip lists {name} twice");
        let size = entry.size();
        files.push(Listed { name, index, size });
    }
    ensure!(!files.is_empty(), "zip holds no files");
    Ok(files)
}

/// The archive on disk, as files that are read from it when copied.
pub(crate) fn members(archive: &Path) -> Result<Vec<(String, Blob)>> {
    let file = File::open(archive).with_context(|| format!("cannot open {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("expected a zip archive")?;
    Ok(list(&mut zip)?
        .into_iter()
        .map(|listed| {
            let blob = Blob::Member {
                archive: archive.to_owned(),
                index: listed.index,
                size: listed.size,
            };
            (listed.name, blob)
        })
        .collect())
}

/// The archive's files held in memory, for the small releases read whole.
pub(crate) fn files(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    files_where(bytes, |_| true)
}

/// The archive's files whose path `keep` accepts, held in memory.
pub(crate) fn files_where(
    bytes: &[u8],
    keep: impl Fn(&str) -> bool,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("expected a zip archive")?;
    let mut files = Vec::new();
    for listed in list(&mut zip)?
        .into_iter()
        .filter(|listed| keep(&listed.name))
    {
        let mut contents = Vec::with_capacity(listed.size as usize);
        zip.by_index(listed.index)?.read_to_end(&mut contents)?;
        files.push((listed.name, contents));
    }
    Ok(files)
}
