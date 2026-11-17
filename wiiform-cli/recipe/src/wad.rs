// SPDX-License-Identifier: GPL-2.0-only
//! WAD payloads, read and checked while a recipe is deserialized.
use std::{ops::RangeInclusive, path::PathBuf};

use anyhow::{Result, ensure};
use derive_more::Deref;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::ResolvedPath;

const WAD_BOUNDS: RangeInclusive<usize> = 0x40..=0x2100_0000;

mod file;
#[cfg_attr(test, allow(dead_code))] // Tests never download.
mod https;
mod sha256;

pub(crate) use self::file::WadFile;
pub use self::sha256::Sha256;
use crate::source::Source;

#[cfg(not(test))]
pub(crate) use self::https::fetch;
#[cfg(test)]
pub(crate) use tests::fetch;

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
        (file.source.read(WAD_BOUNDS))
            .and_then(|fetched| Self::checked(fetched.bytes, file.sha256))
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
    use std::cell::RefCell;

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
