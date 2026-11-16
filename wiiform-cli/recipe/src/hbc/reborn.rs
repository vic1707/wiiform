// SPDX-License-Identifier: GPL-2.0-only
//! The retail WAD of a tagged HBC Reborn release.
use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;

use crate::wad::{Sha256, Wad, fetch};

const REPO: &str = "Monsterray/hbc-reborn";

#[derive(Deserialize)]
struct Release {
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    digest: Option<String>,
}

pub(super) fn download(version: &Version, sha256: Option<Sha256>) -> Result<Wad> {
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
