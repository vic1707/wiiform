// SPDX-License-Identifier: GPL-2.0-only
use std::{
    borrow::Cow,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ResolvedPath, read_bounded};

/// Data supplied by one local file or inline in a recipe.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Data<T> {
    /// Local path, relative to the recipe.
    Path(PathBuf),
    /// Data carried in the recipe itself.
    Inline(T),
}

impl<T> Data<T> {
    pub(crate) fn resolve_path(&mut self, directory: &Path) -> Result<()> {
        if let Self::Path(path) = self {
            *path = ResolvedPath::try_from(directory.join(&*path))?.into_path();
        }
        Ok(())
    }
}

impl<T: AsRef<[u8]>> Data<T> {
    pub fn resolve(&self, bounds: RangeInclusive<usize>) -> Result<Cow<'_, [u8]>> {
        match self {
            Self::Path(path) => read_bounded(path, bounds).map(Cow::Owned),
            Self::Inline(data) => Ok(Cow::Borrowed(data.as_ref())),
        }
    }
}
