// SPDX-License-Identifier: GPL-2.0-only
//! The retail WAD of a tagged HBC Reborn release.
use anyhow::{Context, Result};
use semver::Version;

use crate::{
    github,
    wad::{Sha256, Wad},
};

const REPO: &str = "Monsterray/hbc-reborn";

pub(super) fn download(version: &Version, sha256: Option<Sha256>) -> Result<Wad> {
    let names = [
        format!("hbc-reborn-{version}-OHBC.wad"),
        format!("hbc-{version}-OHBC.wad"),
    ];
    let names = names.each_ref().map(String::as_str);
    let release = github::find(REPO, &format!("v{version}"), &names)
        .with_context(|| format!("HBC Reborn v{version} has no retail OHBC WAD"))?;
    let digest = (sha256.or(release.sha256))
        .context("release asset lacks a valid SHA-256 digest; set hbc.reborn.sha256")?;
    Wad::download(&release.url, digest).with_context(|| format!("cannot download {}", release.url))
}
