// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    Display, Menu, Priiloader, PriiloaderFile, RemovalInput, Setup, Title, TitleFile, TitleId,
};

/// Recipe file shape, before resolving local paths and channel aliases.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeFile {
    /// Stock System Menu target (4.3E, 4.3U, or 4.3J).
    pub menu: Menu,
    /// Channels to remove; names expand using the effective menu's region.
    #[serde(default)]
    pub remove: Vec<RemovalInput>,
    /// Optional first-run setup values.
    #[schemars(with = "crate::schema::NonNull<Setup>")]
    pub setup: Option<Setup>,
    /// Optional display settings; at least one field is required when present.
    #[schemars(with = "crate::schema::NonNull<Display>")]
    pub display: Option<Display>,
    /// Optional WAD titles. Every title requires a path and SHA-256.
    #[serde(default)]
    pub titles: Vec<TitleFile>,
    /// Optional Priiloader install/settings/hacks.
    #[schemars(with = "crate::schema::NonNull<PriiloaderFile>")]
    pub priiloader: Option<PriiloaderFile>,
}

/// Fully validated recipe consumed by the build workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub menu: Menu,
    /// Canonical allowlisted IDs after alias expansion for `menu`.
    pub remove: Vec<TitleId>,
    pub setup: Option<Setup>,
    pub display: Option<Display>,
    pub titles: Vec<Title>,
    pub priiloader: Option<Priiloader>,
}
