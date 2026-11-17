// SPDX-License-Identifier: GPL-2.0-only
//! The files of a zip archive.
use std::io::{Cursor, Read};

use anyhow::{Context, Result, ensure};

use super::FILE_BOUNDS;

/// The archive's files, by their path inside it. Directories are implied by
/// the paths; macOS metadata (`__MACOSX/`, `._*`) is left out.
pub(super) fn files(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("expected a zip archive")?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        let base = name.rsplit('/').next().unwrap_or_default();
        if name.starts_with("__MACOSX/") || base.starts_with("._") || entry.is_dir() {
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
        ensure!(
            !files.iter().any(|(existing, _)| *existing == name),
            "zip lists {name} twice"
        );
        let mut contents = Vec::with_capacity(entry.size() as usize);
        (&mut entry)
            .take(*FILE_BOUNDS.end() as u64)
            .read_to_end(&mut contents)?;
        files.push((name, contents));
    }
    ensure!(!files.is_empty(), "zip holds no files");
    Ok(files)
}
