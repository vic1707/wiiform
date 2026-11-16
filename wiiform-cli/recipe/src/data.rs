// SPDX-License-Identifier: GPL-2.0-only
use std::{
    borrow::Cow,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ResolvedPath, read_bounded, yaml::Binary};

/// Data supplied by one local file, inline in a recipe, or over HTTPS.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Data {
    /// Local path, relative to the recipe.
    Path(PathBuf),
    /// Data carried in the recipe itself.
    Inline(Binary),
    /// HTTPS URL for the raw file.
    Url(String),
}

/// A command-line argument: a URL, or the path of an existing file.
impl TryFrom<PathBuf> for Data {
    type Error = anyhow::Error;

    fn try_from(input: PathBuf) -> Result<Self> {
        match input.to_str().filter(|value| value.contains("://")) {
            Some(url) => Ok(Self::Url(url.to_owned())),
            None => Ok(Self::Path(
                ResolvedPath::try_from(input)?.as_ref().to_owned(),
            )),
        }
    }
}

impl Data {
    pub(crate) const WAD_BOUNDS: RangeInclusive<usize> = 0x40..=0x2100_0000;

    /// Reads a WAD, which `fetch` downloads when the source is a URL.
    pub fn resolve_wad(
        &self,
        fetch: impl FnOnce(&str, usize) -> Result<Vec<u8>>,
    ) -> Result<Cow<'_, [u8]>> {
        self.resolve(Self::WAD_BOUNDS, fetch)
    }

    pub(crate) fn rebase(&mut self, directory: &Path) {
        if let Self::Path(path) = self {
            *path = directory.join(&*path);
        }
    }

    pub fn resolve(
        &self,
        bounds: RangeInclusive<usize>,
        fetch: impl FnOnce(&str, usize) -> Result<Vec<u8>>,
    ) -> Result<Cow<'_, [u8]>> {
        let bytes = match self {
            Self::Path(path) => return read_bounded(path, bounds).map(Cow::Owned),
            Self::Inline(data) => Cow::Borrowed(data.as_ref()),
            Self::Url(url) => Cow::Owned(fetch(url, *bounds.end())?),
        };
        ensure!(
            bounds.contains(&bytes.len()),
            "payload has an unsupported size"
        );
        Ok(bytes)
    }
}
