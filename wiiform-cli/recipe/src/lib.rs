// SPDX-License-Identifier: GPL-2.0-only
//! The recipe a NAND is built from, as serde reads it.
//!
//! Each top-level key has a module of the same name. Deserializing a
//! [`Recipe`] reads, downloads and checks every WAD it names.
use std::{
    collections::{BTreeSet, HashSet},
    io::Read,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

pub mod cios;
mod files;
mod hbc;
mod menu;
pub mod priiloader;
mod region;
mod remove;
pub mod rvloader;
pub mod setup;
#[cfg(test)]
mod tests;
mod title_id;
mod titles;
mod wad;
pub mod yaml;

pub use self::{
    cios::Cios,
    files::{ResolvedPath, open_regular_file, read_bounded},
    hbc::Hbc,
    menu::Menu,
    priiloader::Priiloader,
    region::Region,
    remove::RemovableChannel,
    rvloader::Rvloader,
    setup::Setup,
    title_id::TitleId,
    titles::Title,
    wad::{Sha256, Wad},
};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Stock System Menu target (4.3E, 4.3U, or 4.3J).
    pub menu: Menu,
    /// Stock channels to remove.
    #[serde(default)]
    pub remove: BTreeSet<RemovableChannel>,
    /// Supply a mapping to complete first-run setup with these settings and defaults.
    /// Omit this section or use null to run the Wii setup wizard.
    #[serde(default)]
    pub setup: Option<Setup>,
    /// WADs to install from a local file, inline bytes, or an HTTPS URL.
    #[serde(default)]
    pub titles: Vec<Title>,
    /// Prebuilt, hash-pinned Wii d2x cIOS WADs.
    #[serde(default)]
    pub cios: Vec<Cios>,
    /// Optional Homebrew Channel from a tagged Reborn release or a custom WAD.
    #[serde(default)]
    pub hbc: Option<Hbc>,
    /// Optional pinned Priiloader install with its settings and hacks.
    #[serde(default)]
    pub priiloader: Option<Priiloader>,
    /// Optional RVLoader boot hook. Requires `priiloader`.
    #[serde(default)]
    pub rvloader: Option<Rvloader>,
}

/// Reads a recipe file. Its WADs are read, downloaded and checked on the way,
/// with local paths relative to the recipe's directory.
impl TryFrom<&ResolvedPath> for Recipe {
    type Error = anyhow::Error;

    fn try_from(path: &ResolvedPath) -> Result<Self> {
        let path = path.as_ref();
        let directory = path.parent().context("recipe path has no parent")?;
        Self::read(open_regular_file(path)?.0, directory)
    }
}

impl Recipe {
    fn read(input: impl Read, directory: &Path) -> Result<Self> {
        let recipe: Self = crate::wad::relative_to(directory.to_owned(), || {
            serde_saphyr::from_reader_with_options(input, crate::yaml::options())
        })
        .context("invalid recipe")?;
        recipe.validate()?;
        Ok(recipe)
    }

    /// Checks what the types cannot express. Call it again after changing a field.
    pub fn validate(&self) -> Result<()> {
        let menu = self.menu;
        if let Some(setup) = &self.setup {
            ensure!(
                menu.allows_language(setup.language_for(menu)),
                "setup language is not supported by menu {menu}"
            );
            let country = setup.country_for(menu);
            ensure!(
                menu.allows_country(country),
                "setup country {} is not supported by menu {menu}",
                country.as_alpha2()
            );
        }
        if self.rvloader.is_some() {
            let priiloader = (self.priiloader.as_ref())
                .context("rvloader requires a priiloader section: Priiloader starts it")?;
            ensure!(
                priiloader.autoboot.is_none() && priiloader.return_to.is_none(),
                "rvloader sets Priiloader's autoboot and return_to: remove them from priiloader"
            );
        }
        let mut slots = HashSet::new();
        for cios in &self.cios {
            let slot = cios.target.slot;
            ensure!(slots.insert(slot), "recipe lists cIOS slot {slot} twice");
        }
        Ok(())
    }
}

/// The JSON Schema of a recipe, for editors.
///
/// Menu-dependent rules and file checks are beyond what a schema can express;
/// reading a [`Recipe`] enforces those.
pub fn json_schema() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&schemars::schema_for!(Recipe))
}
