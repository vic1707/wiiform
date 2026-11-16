// SPDX-License-Identifier: GPL-2.0-only
use std::borrow::Cow;

use anyhow::{Context, Result};
use schemars::JsonSchema;
use semver::Version;
use serde::Deserialize;

use crate::{Data, Sha256};

const REPO: &str = "Monsterray/hbc-reborn";

/// Homebrew Channel to install before Priiloader is configured.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Hbc {
    /// Download the retail OHBC WAD from a tagged HBC Reborn release.
    Reborn {
        /// Semantic version without the leading v.
        #[schemars(with = "String")]
        version: Version,
        /// Optional WAD checksum. Defaults to the GitHub release asset digest.
        sha256: Option<Sha256>,
    },
    /// Install a user-supplied, hash-pinned Homebrew Channel WAD.
    Custom { source: Data, sha256: Sha256 },
}

#[derive(Deserialize)]
struct Release {
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    digest: Option<String>,
}

impl Hbc {
    pub fn resolve(
        &self,
        mut fetch: impl FnMut(&str, usize) -> Result<Vec<u8>>,
    ) -> Result<(Cow<'_, [u8]>, Sha256)> {
        match self {
            Self::Reborn { version, sha256 } => {
                let release =
                    format!("https://api.github.com/repos/{REPO}/releases/tags/v{version}");
                let metadata = fetch(&release, 1024 * 1024)
                    .with_context(|| format!("cannot fetch HBC Reborn release v{version}"))?;
                let release: Release = serde_json::from_slice(&metadata)
                    .context("invalid HBC Reborn release metadata")?;
                let current_name = format!("hbc-reborn-{version}-OHBC.wad");
                let older_name = format!("hbc-{version}-OHBC.wad");
                let asset = release
                    .assets
                    .into_iter()
                    .find(|asset| asset.name == current_name || asset.name == older_name)
                    .with_context(|| format!("HBC Reborn v{version} has no retail OHBC WAD"))?;
                let digest = sha256
                    .or_else(|| {
                        asset
                            .digest
                            .as_deref()?
                            .strip_prefix("sha256:")?
                            .parse()
                            .ok()
                    })
                    .context("release asset lacks a valid SHA-256 digest; set hbc.reborn.sha256")?;
                let url = format!(
                    "https://github.com/{REPO}/releases/download/v{version}/{}",
                    asset.name
                );
                let wad = fetch(&url, *Data::WAD_BOUNDS.end())
                    .with_context(|| format!("cannot download {}", asset.name))?;
                Ok((Cow::Owned(wad), digest))
            }
            Self::Custom { source, sha256 } => Ok((source.resolve_wad(fetch)?, *sha256)),
        }
    }
}
