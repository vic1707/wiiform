// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result};
use schemars::JsonSchema;
use semver::Version;
use serde::Deserialize;

use crate::wad::{Sha256, Wad, WadFile, fetch};

const REPO: &str = "Monsterray/hbc-reborn";

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
                reborn(&version, sha256).map_err(|error| format!("{error:#}"))
            }
            HbcFile::Custom(wad) => wad.try_into(),
        }
        .map(Self)
    }
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

fn reborn(version: &Version, sha256: Option<Sha256>) -> Result<Wad> {
    let release = format!("https://api.github.com/repos/{REPO}/releases/tags/v{version}");
    let metadata = fetch(&release, 1024 * 1024)
        .with_context(|| format!("cannot fetch HBC Reborn release v{version}"))?;
    let release: Release =
        serde_json::from_slice(&metadata).context("invalid HBC Reborn release metadata")?;
    let names = [
        format!("hbc-reborn-{version}-OHBC.wad"),
        format!("hbc-{version}-OHBC.wad"),
    ];
    let asset = (release.assets.into_iter())
        .find(|asset| names.contains(&asset.name))
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
    Wad::download(&url, digest).with_context(|| format!("cannot download {}", asset.name))
}
