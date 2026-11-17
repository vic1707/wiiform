// SPDX-License-Identifier: GPL-2.0-only
//! Assets of tagged GitHub releases.
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::wad::{Sha256, fetch};

/// An asset of a tagged GitHub release.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "GithubAsset")]
pub(crate) struct Asset {
    /// Repository as owner/name, such as wiidev/usbloadergx.
    pub repo: String,
    /// Release tag, such as v4.0-r1283.
    pub tag: String,
    /// Asset file name, such as usbloadergx_r1283.zip.
    pub asset: String,
}

/// Where a release asset downloads from, and the SHA-256 GitHub states for it.
pub(crate) struct Release {
    pub url: String,
    /// GitHub records digests for assets uploaded since June 2025.
    pub sha256: Option<Sha256>,
}

#[derive(Deserialize)]
struct ReleaseMetadata {
    assets: Vec<AssetMetadata>,
}

#[derive(Deserialize)]
struct AssetMetadata {
    name: String,
    digest: Option<String>,
}

// Names go into URLs unescaped, so they keep to what release names commonly use.
fn plain(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

impl Asset {
    pub(crate) fn find(&self) -> Result<Release> {
        find(&self.repo, &self.tag, &[self.asset.as_str()])
    }
}

/// Finds the first of `names` among the assets of `repo`'s release `tag`.
pub(crate) fn find(repo: &str, tag: &str, names: &[&str]) -> Result<Release> {
    let valid_repo = repo
        .split_once('/')
        .is_some_and(|(owner, name)| plain(owner) && plain(name));
    ensure!(valid_repo, "GitHub repository must be owner/name: {repo}");
    ensure!(plain(tag), "unsupported GitHub release tag: {tag}");
    ensure!(
        names.iter().all(|name| plain(name)),
        "unsupported GitHub asset name"
    );

    let metadata_url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let metadata = fetch(&metadata_url, 1024 * 1024)
        .with_context(|| format!("cannot fetch the {repo} release {tag}"))?;
    let release: ReleaseMetadata = serde_json::from_slice(&metadata)
        .with_context(|| format!("invalid metadata for the {repo} release {tag}"))?;
    let asset = (names.iter())
        .find_map(|name| release.assets.iter().find(|asset| asset.name == *name))
        .with_context(|| {
            format!(
                "the {repo} release {tag} has no asset {}",
                names.join(" or ")
            )
        })?;
    let sha256 = (asset.digest.as_deref())
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .and_then(|hex| hex.parse().ok());
    Ok(Release {
        url: format!(
            "https://github.com/{repo}/releases/download/{tag}/{}",
            asset.name
        ),
        sha256,
    })
}
