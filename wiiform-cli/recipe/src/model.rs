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
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<Setup>")]
    pub setup: Option<Setup>,
    /// Optional display settings. An empty mapping uses the defaults.
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<Display>")]
    pub display: Option<Display>,
    /// Optional WAD titles. Every title requires a path and SHA-256.
    #[serde(default)]
    pub titles: Vec<TitleFile>,
    /// Optional Priiloader install/settings/hacks.
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<PriiloaderFile>")]
    pub priiloader: Option<PriiloaderFile>,
}

/// Fully validated recipe consumed by the build workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub(crate) menu: Menu,
    /// Canonical allowlisted IDs after alias expansion for `menu`.
    pub(crate) remove: Vec<TitleId>,
    pub(crate) setup: Option<Setup>,
    pub(crate) display: Option<Display>,
    pub(crate) titles: Vec<Title>,
    pub(crate) priiloader: Option<Priiloader>,
    pub(crate) remove_inputs: Vec<RemovalInput>,
}

impl Recipe {
    pub fn menu(&self) -> Menu {
        self.menu
    }

    pub fn remove(&self) -> &[TitleId] {
        &self.remove
    }

    pub fn setup(&self) -> Option<&Setup> {
        self.setup.as_ref()
    }

    pub fn display(&self) -> Option<&Display> {
        self.display.as_ref()
    }

    pub fn titles(&self) -> &[Title] {
        &self.titles
    }

    pub fn priiloader(&self) -> Option<&Priiloader> {
        self.priiloader.as_ref()
    }

    /// Retarget menu-dependent recipe settings and aliases, then check coherence again.
    pub fn with_menu(self, menu: Menu) -> Result<Self, crate::RecipeError> {
        crate::parser::retarget_recipe(self, menu)
    }
}
