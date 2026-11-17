// SPDX-License-Identifier: GPL-2.0-only
//! Anything placed from the device's root.
use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, de::IgnoredAny};

use super::{CardPath, archive, extract::Extract, read};
use crate::{source::Source, wad::Sha256};

/// Where a bundle's file goes.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Placement {
    /// Unpack the whole archive into this directory.
    Unzip(CardPath),
    /// Unpack the chosen archive files: see `Extract`.
    Extract(Extract),
    /// Copy the file to this path.
    To(CardPath),
}

/// A file or archive placed from the device's root.
#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub(super) struct Bundle {
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

impl Bundle {
    pub(super) fn files(self) -> Result<Vec<(CardPath, Vec<u8>)>> {
        if let Some(key) = self.unknown.keys().next() {
            bail!("unknown field `{key}`");
        }
        let bytes = read(self.source, self.sha256)?;
        match self.placement {
            Placement::To(path) => {
                ensure!(!path.is_root(), "`to` needs a file path, not the root");
                Ok(vec![(path, bytes)])
            }
            Placement::Unzip(directory) => (archive::files(&bytes)?.into_iter())
                .map(|(name, bytes)| Ok((directory.join(&name)?, bytes)))
                .collect(),
            Placement::Extract(extract) => {
                extract.place(archive::files(&bytes)?, &CardPath::default())
            }
        }
    }
}
