// SPDX-License-Identifier: GPL-2.0-only
//! The `nand` key: the image Wiiform builds. Each of its keys has a module of
//! the same name.
pub mod cios;
pub mod hbc;
pub mod menu;
pub mod priiloader;
pub mod remove;
pub mod rvloader;
pub mod setup;
pub mod titles;

use std::collections::{BTreeSet, HashSet};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use self::{
    cios::Cios, hbc::Hbc, menu::Menu, priiloader::Priiloader, remove::RemovableChannel,
    rvloader::Rvloader, setup::Setup, titles::Title,
};

/// The NAND image: stock menu, then everything installed on top of it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Nand {
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

impl Nand {
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
        if let Some(rvloader) = &self.rvloader {
            let priiloader = (self.priiloader.as_ref())
                .context("rvloader requires a priiloader section: Priiloader starts it")?;
            ensure!(
                priiloader.autoboot.is_none() && priiloader.return_to.is_none(),
                "rvloader sets Priiloader's autoboot and return_to: remove them from priiloader"
            );
            if rvloader.vga {
                let display = self.setup.as_ref().map(|setup| &setup.display);
                ensure!(
                    display.is_some_and(|display| display.progressive),
                    "rvloader.vga requires setup.display.progressive: true"
                );
                ensure!(
                    menu != Menu::E43 || display.is_some_and(|display| display.pal60),
                    "rvloader.vga on menu {menu} requires setup.display.pal60: true"
                );
            }
        }
        let mut slots = HashSet::new();
        for cios in &self.cios {
            let slot = cios.target.slot;
            ensure!(slots.insert(slot), "nand lists cIOS slot {slot} twice");
        }
        Ok(())
    }
}
