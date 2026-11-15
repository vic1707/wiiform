// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, path::Path};

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;

use super::Sha256;
use crate::{Data, ResolvedPath, yaml::Binary};

/// One WAD source and the SHA-256 of its exact bytes.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "{Options}")]
pub struct Wad<Options> {
    pub(crate) source: Data<Binary>,
    /// SHA-256 of the exact WAD bytes.
    sha256: Sha256,
    // Keep source as a direct field so serde-saphyr can read the !!binary tag.
    #[serde(flatten)]
    pub options: Options,
}

impl<Options> Wad<Options> {
    pub fn from_path(path: &ResolvedPath, sha256: Sha256, options: Options) -> Self {
        Self {
            source: Data::Path(path.as_ref().to_owned()),
            sha256,
            options,
        }
    }

    pub(crate) fn rebase(&mut self, directory: &Path) {
        self.source.rebase(directory);
    }

    pub fn resolve(&self) -> Result<(Cow<'_, [u8]>, &[u8; 32])> {
        Ok((
            self.source.resolve(0x40..=0x2100_0000)?,
            self.sha256.as_bytes(),
        ))
    }
}
