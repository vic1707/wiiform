// SPDX-License-Identifier: GPL-2.0-only
//! Anything placed from the device's root.
use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, de::IgnoredAny};

use super::{CardPath, extract::Extract};
use crate::archive;
use crate::{blob::Blob, source::Source, wad::Sha256};

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
#[schemars(deny_unknown_fields, transform = crate::schema::sha256_for_url)]
pub(super) struct Bundle {
    #[serde(flatten)]
    source: Source,
    /// SHA-256 of the exact file, checked when given. Required for a url, and
    /// for a GitHub release asset whose release states none.
    sha256: Option<Sha256>,
    #[serde(flatten)]
    placement: Placement,
    // Serde cannot reject unknown keys next to flattened fields; they land here.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: BTreeMap<String, IgnoredAny>,
}

impl Bundle {
    /// Names the bundle by its source in errors and collision notices.
    pub(super) fn label(&self) -> String {
        format!("bundle {}", self.source)
    }

    pub(super) fn files(self) -> Result<Vec<(CardPath, Blob)>> {
        if let Some(key) = self.unknown.keys().next() {
            bail!("unknown field `{key}`");
        }
        let staged = self.source.stage(self.sha256)?;
        match self.placement {
            Placement::To(target) => {
                ensure!(!target.is_root(), "`to` needs a file path, not the root");
                Ok(vec![(target, Blob::file(staged)?)])
            }
            Placement::Unzip(directory) => (archive::members(&staged)?.into_iter())
                .map(|(name, blob)| Ok((directory.join(&name)?, blob)))
                .collect(),
            Placement::Extract(extract) => {
                extract.place(archive::members(&staged)?, &CardPath::default())
            }
        }
    }
}
