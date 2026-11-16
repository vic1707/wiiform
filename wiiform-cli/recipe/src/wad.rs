// SPDX-License-Identifier: GPL-2.0-only
//! WAD payloads, read and checked while a recipe is deserialized.
use std::{cell::RefCell, ops::RangeInclusive, path::PathBuf};

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use derive_more::Deref;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::Digest;

use crate::{ResolvedPath, read_bounded};

const WAD_BOUNDS: RangeInclusive<usize> = 0x40..=0x2100_0000;

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

#[cfg_attr(test, allow(dead_code))] // Tests never download.
mod https;

#[cfg(not(test))]
pub(crate) use self::https::fetch;
#[cfg(test)]
pub(crate) use tests::fetch;

/// SHA-256 of the exact binary payload, encoded as 64 hex digits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Sha256(
    #[serde(with = "hex")]
    #[schemars(
        with = "String",
        length(equal = 64),
        regex(pattern = "^[0-9a-fA-F]{64}$")
    )]
    pub [u8; 32],
);

impl std::str::FromStr for Sha256 {
    type Err = hex::FromHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        hex::FromHex::from_hex(value).map(Self)
    }
}

impl Sha256 {
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha2::Sha256::digest(bytes).into())
    }
}

/// Where a recipe gets a file from.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Source {
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
}

fn base64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = String::deserialize(deserializer)?;
    let compact: String = text.split_ascii_whitespace().collect();
    STANDARD.decode(compact).map_err(serde::de::Error::custom)
}

/// A WAD from one source, with the SHA-256 of its exact bytes.
// When a WAD is flattened into an entry, this is where that entry's unknown
// keys are rejected: it sees every key the entry does not name itself, and
// serde cannot make the same check on the outer struct.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Wad")]
pub(crate) struct WadFile {
    #[serde(flatten)]
    source: Source,
    /// SHA-256 of the exact WAD bytes.
    sha256: Sha256,
}

/// The bytes of a WAD whose SHA-256 matched the one its recipe or command line gave.
#[derive(derive_more::Debug, Deref, Deserialize, JsonSchema)]
#[debug("Wad({} bytes)", _0.len())]
#[serde(try_from = "WadFile")]
pub struct Wad(#[deref(forward)] Vec<u8>);

impl Wad {
    pub(crate) fn checked(bytes: Vec<u8>, expected: Sha256) -> Result<Self> {
        ensure!(
            WAD_BOUNDS.contains(&bytes.len()),
            "WAD has an unsupported size"
        );
        ensure!(
            Sha256::of(&bytes) == expected,
            "WAD does not match its SHA-256"
        );
        Ok(Self(bytes))
    }

    pub(crate) fn download(url: &str, expected: Sha256) -> Result<Self> {
        Self::checked(fetch(url, *WAD_BOUNDS.end())?, expected)
    }
}

impl TryFrom<WadFile> for Wad {
    type Error = String;

    fn try_from(file: WadFile) -> Result<Self, String> {
        match file.source {
            Source::Path(path) => {
                let path = DIRECTORY.with_borrow(|directory| directory.join(path));
                read_bounded(&path, WAD_BOUNDS).and_then(|bytes| Self::checked(bytes, file.sha256))
            }
            Source::Url(url) => {
                Self::download(&url, file.sha256).with_context(|| format!("cannot download {url}"))
            }
            Source::Inline(bytes) => Self::checked(bytes, file.sha256),
        }
        .map_err(|error| format!("{error:#}"))
    }
}

/// A command-line WAD: a URL or the path of an existing file, with its expected SHA-256.
impl TryFrom<(PathBuf, Sha256)> for Wad {
    type Error = anyhow::Error;

    fn try_from((input, sha256): (PathBuf, Sha256)) -> Result<Self> {
        let source = match input.to_str().filter(|value| value.contains("://")) {
            Some(url) => Source::Url(url.to_owned()),
            None => Source::Path(ResolvedPath::try_from(input)?.as_ref().to_owned()),
        };
        Self::try_from(WadFile { source, sha256 }).map_err(anyhow::Error::msg)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    type Fetch = Box<dyn Fn(&str, usize) -> Result<Vec<u8>>>;

    thread_local! {
        static FETCH: RefCell<Option<Fetch>> = const { RefCell::new(None) };
    }

    /// Stands in for HTTPS in tests. Nothing may download unless a test said how.
    pub(crate) fn fetch(url: &str, limit: usize) -> Result<Vec<u8>> {
        FETCH.with_borrow(|fetch| match fetch {
            Some(fetch) => fetch(url, limit),
            None => panic!("unexpected download of {url}"),
        })
    }

    pub(crate) fn with_fetch<T>(
        fetch: impl Fn(&str, usize) -> Result<Vec<u8>> + 'static,
        read: impl FnOnce() -> T,
    ) -> T {
        FETCH.set(Some(Box::new(fetch)));
        let result = read();
        FETCH.set(None);
        result
    }
}
