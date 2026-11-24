// SPDX-License-Identifier: GPL-2.0-only
//! d2x cIOS: the `titles.cios` key.
pub mod d2x;
pub mod target;

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, de::IgnoredAny};

pub use self::{d2x::Build, target::Target};
use crate::{
    Wad, github,
    source::{Pinned, Source},
    wad::Sha256,
};

/// Where a cIOS comes from: a prebuilt WAD, or a d2x release to build from.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Package {
    /// Local prebuilt cIOS WAD, relative to the recipe.
    Path(PathBuf),
    /// HTTPS URL of a prebuilt cIOS WAD.
    Url(String),
    /// A prebuilt cIOS WAD, base64-encoded.
    Inline(
        #[serde(deserialize_with = "crate::source::base64")]
        #[schemars(with = "String", extend("contentEncoding" = "base64"))]
        Vec<u8>,
    ),
    /// A prebuilt cIOS WAD attached to a tagged GitHub release.
    Github(github::Asset),
    /// A d2x release archive (installer, modules and ciosmaps.xml): the cIOS is
    /// built from it on the base IOS its map names, downloaded from NUS.
    D2x(Pinned),
}

/// A d2x cIOS and the slot it fills.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "Cios", deny_unknown_fields, transform = crate::schema::sha256_for_url)]
struct CiosFile {
    #[serde(flatten)]
    package: Package,
    /// SHA-256 of a prebuilt WAD, checked when given. Required for a url, and
    /// for a GitHub release asset whose release states none. A d2x release
    /// gives its own inside `d2x`.
    sha256: Option<Sha256>,
    target: Target,
    // Serde cannot reject unknown keys next to flattened fields; they land here.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: BTreeMap<String, IgnoredAny>,
}

/// What wiiform installs for a cIOS entry.
#[derive(Debug)]
pub enum Kind {
    Prebuilt(Wad),
    Built(Build),
}

// A cIOS entry, read and checked. No doc comment: the schema describes `CiosFile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "CiosFile")]
#[schemars(with = "CiosFile", rename = "Cios")]
pub struct Cios {
    pub kind: Kind,
    pub target: Target,
}

impl TryFrom<CiosFile> for Cios {
    type Error = String;

    fn try_from(file: CiosFile) -> Result<Self, String> {
        file.read().map_err(|error| format!("{error:#}"))
    }
}

impl CiosFile {
    fn read(self) -> Result<Cios> {
        if let Some(key) = self.unknown.keys().next() {
            bail!("unknown field `{key}`");
        }
        let prebuilt = |source: Source| -> Result<Kind> {
            Ok(Kind::Prebuilt(Wad::from_source(source, self.sha256)?))
        };
        let kind = match self.package {
            Package::Path(path) => prebuilt(Source::Path(path))?,
            Package::Url(url) => prebuilt(Source::Url(url))?,
            Package::Inline(bytes) => prebuilt(Source::Inline(bytes))?,
            Package::Github(asset) => prebuilt(Source::Github(asset))?,
            Package::D2x(release) => {
                ensure!(
                    self.sha256.is_none(),
                    "give the d2x release's sha256 inside `d2x`"
                );
                Kind::Built(d2x::build(&release.read()?, self.target.base.into())?)
            }
        };
        Ok(Cios {
            kind,
            target: self.target,
        })
    }
}
