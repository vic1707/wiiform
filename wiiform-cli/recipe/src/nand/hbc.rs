// SPDX-License-Identifier: GPL-2.0-only
mod reborn;

use schemars::JsonSchema;
use semver::Version;
use serde::Deserialize;

use crate::wad::{Sha256, Wad, WadFile};

/// Homebrew Channel to install before Priiloader is configured.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename = "Hbc")]
enum HbcFile {
    /// Download the retail OHBC WAD from a tagged HBC Reborn release.
    Reborn {
        /// Semantic version without the leading v.
        #[schemars(with = "String")]
        version: Version,
        /// Optional WAD checksum. Defaults to the GitHub release asset digest.
        sha256: Option<Sha256>,
    },
    /// Install a user-supplied, hash-pinned Homebrew Channel WAD.
    Custom(WadFile),
}

// The Homebrew Channel WAD a recipe asked for. No doc comment: the schema describes `HbcFile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "HbcFile")]
pub struct Hbc(pub Wad);

impl TryFrom<HbcFile> for Hbc {
    type Error = String;

    fn try_from(file: HbcFile) -> Result<Self, String> {
        match file {
            HbcFile::Reborn { version, sha256 } => {
                reborn::download(&version, sha256).map_err(|error| format!("{error:#}"))
            }
            HbcFile::Custom(wad) => wad.try_into(),
        }
        .map(Self)
    }
}
