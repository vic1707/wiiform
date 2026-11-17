// SPDX-License-Identifier: GPL-2.0-only
//! Where a recipe gets a file from, shared by WADs and SD card files.
use std::{cell::RefCell, ops::RangeInclusive, path::PathBuf};

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

use crate::{
    github, read_bounded,
    wad::{Sha256, fetch},
};

thread_local! {
    /// Directory of the recipe being read; its relative paths start there.
    static DIRECTORY: RefCell<PathBuf> = const { RefCell::new(PathBuf::new()) };
}

/// Runs `read` with relative paths resolved against `directory`.
pub(crate) fn relative_to<T>(directory: PathBuf, read: impl FnOnce() -> T) -> T {
    let previous = DIRECTORY.replace(directory);
    let result = read();
    DIRECTORY.set(previous);
    result
}

/// Where a recipe gets a file from.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    /// Local path, relative to the recipe.
    Path(PathBuf),
    /// HTTPS URL of the raw file.
    Url(String),
    /// The file itself, base64-encoded.
    Inline(
        #[serde(deserialize_with = "base64")]
        #[schemars(with = "String", extend("contentEncoding" = "base64"))]
        Vec<u8>,
    ),
    /// An asset of a tagged GitHub release.
    Github(github::Asset),
}

/// A file's bytes, and the SHA-256 its publisher states for them, if any.
pub(crate) struct Fetched {
    pub bytes: Vec<u8>,
    pub published_sha256: Option<Sha256>,
}

impl Source {
    /// Reads the file, refusing one whose size is outside `bounds`.
    pub(crate) fn read(self, bounds: RangeInclusive<usize>) -> Result<Fetched> {
        let (bytes, published_sha256) = match self {
            Self::Path(path) => {
                let path = DIRECTORY.with_borrow(|directory| directory.join(path));
                (read_bounded(&path, bounds.clone())?, None)
            }
            Self::Url(url) => {
                let bytes =
                    fetch(&url, *bounds.end()).with_context(|| format!("cannot download {url}"))?;
                (bytes, None)
            }
            Self::Inline(bytes) => (bytes, None),
            Self::Github(asset) => {
                let release = asset.find()?;
                let bytes = fetch(&release.url, *bounds.end())
                    .with_context(|| format!("cannot download {}", release.url))?;
                (bytes, release.sha256)
            }
        };
        ensure!(
            bounds.contains(&bytes.len()),
            "file has an unsupported size"
        );
        Ok(Fetched {
            bytes,
            published_sha256,
        })
    }
}

pub(crate) fn base64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = String::deserialize(deserializer)?;
    let compact: String = text.split_ascii_whitespace().collect();
    STANDARD.decode(compact).map_err(serde::de::Error::custom)
}
