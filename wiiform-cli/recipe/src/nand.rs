// SPDX-License-Identifier: GPL-2.0-only
//! The `nand` key: the image Wiiform builds. Each of its keys has a module of
//! the same name.
pub mod bootmii;
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
    bootmii::Bootmii, hbc::Hbc, menu::Menu, priiloader::Priiloader, remove::RemovableChannel,
    rvloader::Rvloader, setup::Setup, titles::Titles,
};

/// The NAND image: stock menu, then everything installed on top of it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Nand {
    /// Stock System Menu target (4.3E, 4.3U, or 4.3J).
    pub menu: Menu,
    /// Stock channels to remove.
    #[serde(default)]
    #[schemars(transform = crate::schema::list_values::<RemovableChannel>)]
    pub remove: BTreeSet<RemovableChannel>,
    /// Supply a mapping to complete first-run setup with these settings and defaults.
    /// Omit this section or use null to run the Wii setup wizard.
    #[serde(default)]
    pub setup: Option<Setup>,
    /// Titles to install: channels, IOS and cIOS.
    #[serde(default)]
    pub titles: Titles,
    /// Optional BootMii as IOS, taken from the seed.
    #[serde(default)]
    pub bootmii: Option<Bootmii>,
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
        if let Some(priiloader) = &self.priiloader {
            priiloader.validate()?;
            ensure!(
                priiloader.autoboot != Some(priiloader::Autoboot::BootmiiIos)
                    || self.bootmii.is_some(),
                "priiloader.autoboot: bootmii_ios requires the bootmii key"
            );
        }
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
        // One title per IOS slot: an IOS and a cIOS, or two of either, cannot share one.
        let mut slots = HashSet::new();
        let ios = self.titles.ios.iter().map(|ios| ios.title_id());
        let cios = self.titles.cios.iter().map(|cios| cios.target.title_id());
        for title_id in ios.chain(cios) {
            ensure!(
                slots.insert(title_id),
                "titles list IOS slot {} twice",
                title_id.as_u64() as u8
            );
        }
        Ok(())
    }
}
