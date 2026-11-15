// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, path::Path};

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Data, Sha256, titles::WAD_BOUNDS, yaml::Binary};

/// Homebrew Channel to install before Priiloader is configured.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "HbcInput")]
#[schemars(!try_from, tag = "variant", rename_all = "snake_case", deny_unknown_fields)]
pub enum Hbc {
    /// Download a pinned HBC Reborn release with the OHBC title ID.
    Reborn {
        version: RebornVersion,
        /// Optional confirmation of the built-in release digest.
        sha256: Option<Sha256>,
    },
    /// Install a user-supplied, hash-pinned Homebrew Channel WAD.
    Custom {
        source: Data<Binary>,
        sha256: Sha256,
    },
}

// Deserialize the source directly so serde-saphyr can read its !!binary tag.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HbcInput {
    variant: Variant,
    version: Option<RebornVersion>,
    source: Option<Data<Binary>>,
    sha256: Option<Sha256>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Variant {
    Reborn,
    Custom,
}

impl TryFrom<HbcInput> for Hbc {
    type Error = anyhow::Error;

    fn try_from(input: HbcInput) -> Result<Self> {
        match (input.variant, input.version, input.source, input.sha256) {
            (Variant::Reborn, Some(version), None, sha256) => {
                if let Some(digest) = sha256 {
                    ensure!(
                        digest == version.release().2,
                        "hbc.sha256 disagrees with the pinned HBC Reborn release checksum"
                    );
                }
                Ok(Self::Reborn { version, sha256 })
            }
            (Variant::Reborn, ..) => {
                bail!("hbc variant reborn requires a version and accepts an optional sha256")
            }
            (Variant::Custom, None, Some(source), Some(sha256)) => {
                Ok(Self::Custom { source, sha256 })
            }
            (Variant::Custom, ..) => bail!("hbc variant custom requires source and sha256"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
pub enum RebornVersion {
    #[serde(rename = "1.12.1")]
    V1_12_1,
    #[serde(rename = "1.13.0")]
    V1_13_0,
    #[serde(rename = "1.14.0")]
    V1_14_0,
    #[serde(rename = "1.15.0")]
    V1_15_0,
    #[serde(rename = "1.16.0")]
    V1_16_0,
}

impl RebornVersion {
    // Retail OHBC assets and SHA256SUMS from upstream's tagged releases.
    fn release(self) -> (&'static str, &'static str, Sha256) {
        let (version, file, sha256) = match self {
            Self::V1_12_1 => (
                "1.12.1",
                "hbc-1.12.1-OHBC.wad",
                "aef6127160d07226dd421f14a365756bbe555ae8fd1fcc88a6e28dea22c1100f",
            ),
            Self::V1_13_0 => (
                "1.13.0",
                "hbc-reborn-1.13.0-OHBC.wad",
                "df47e513302fe3abfee494d8740eb6dd2e883428a525b2a954e2a59d51ba6d86",
            ),
            Self::V1_14_0 => (
                "1.14.0",
                "hbc-reborn-1.14.0-OHBC.wad",
                "cb7dfb6c4a67de6d3903e3a416437c515aca677d9b8680240f7432a82ca9eac4",
            ),
            Self::V1_15_0 => (
                "1.15.0",
                "hbc-reborn-1.15.0-OHBC.wad",
                "f15147aa0e7a74638c81614ac1142962a565b19978ca6fd39783bc2863983f41",
            ),
            Self::V1_16_0 => (
                "1.16.0",
                "hbc-reborn-1.16.0-OHBC.wad",
                "81ed7e395f586a3e4c20a84c86de99d074ba8cbb55dd6b479f259631a71f4850",
            ),
        };
        (
            version,
            file,
            sha256.parse().expect("invalid pinned HBC release digest"),
        )
    }
}

impl Hbc {
    pub(crate) fn rebase(&mut self, directory: &Path) {
        if let Self::Custom { source, .. } = self {
            source.rebase(directory);
        }
    }

    pub fn resolve(
        &self,
        fetch: impl FnOnce(&str, usize) -> Result<Vec<u8>>,
    ) -> Result<(Cow<'_, [u8]>, Sha256)> {
        match self {
            Self::Reborn { version, .. } => {
                let (version, file, sha256) = version.release();
                let url = format!(
                    "https://github.com/Monsterray/hbc-reborn/releases/download/v{version}/{file}"
                );
                Ok((Cow::Owned(fetch(&url, *WAD_BOUNDS.end())?), sha256))
            }
            Self::Custom { source, sha256 } => Ok((source.resolve(WAD_BOUNDS, fetch)?, *sha256)),
        }
    }
}
