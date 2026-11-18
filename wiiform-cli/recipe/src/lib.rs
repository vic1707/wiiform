// SPDX-License-Identifier: GPL-2.0-only
//! The recipe Wiiform works from, as serde reads it.
//!
//! Each top-level key has a module of the same name. Deserializing a
//! [`Recipe`] reads, downloads and checks every file it names.
use std::{io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

mod archive;
mod defaults;
pub mod device;
mod files;
mod github;
pub mod nand;
mod region;
mod schema;
mod source;
#[cfg(test)]
mod tests;
mod title_id;
mod wad;
pub mod yaml;

pub use self::{
    defaults::Defaulted,
    device::Device,
    files::{ResolvedPath, open_regular_file, read_bounded},
    nand::{
        Nand,
        hbc::Hbc,
        menu::Menu,
        priiloader::Priiloader,
        remove::RemovableChannel,
        rvloader::Rvloader,
        setup::Setup,
        titles::{Channel, Cios, Ios},
    },
    region::Region,
    title_id::TitleId,
    wad::{Sha256, Wad},
};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// The NAND image to build.
    pub nand: Nand,
    /// Files for the SD card, published in sd/ next to nand.bin and keys.bin.
    pub sd: Option<Device>,
    /// Files for a USB drive, published in usb/.
    pub usb: Option<Device>,
    /// The keys the recipe left out that Wiiform filled in, sorted by key.
    #[serde(skip)]
    #[schemars(skip)]
    pub defaulted: Vec<Defaulted>,
}

impl Recipe {
    /// Reads a recipe whose local paths are relative to `directory`.
    pub fn read(input: impl Read, directory: &Path) -> Result<Self> {
        let (recipe, defaulted) = crate::defaults::collect(|| {
            crate::source::relative_to(directory.to_owned(), || {
                serde_saphyr::from_reader_with_options(input, crate::yaml::options())
            })
        });
        let mut recipe: Self = recipe.context("invalid recipe")?;
        recipe.validate()?;
        // rvloader sets Priiloader's autoboot and return_to, which it requires left out.
        recipe.defaulted = (defaulted.into_iter())
            .filter(|defaulted| {
                recipe.nand.rvloader.is_none()
                    || !["nand.priiloader.autoboot", "nand.priiloader.return_to"]
                        .contains(&defaulted.key)
            })
            .collect();
        recipe.defaulted.sort_by_key(|defaulted| defaulted.key);
        Ok(recipe)
    }

    /// Checks what the types cannot express. Call it again after changing a field.
    pub fn validate(&self) -> Result<()> {
        self.nand.validate()?;
        // The card's root also holds the NAND and its keys.
        for path in self.sd.iter().flat_map(|sd| sd.files().keys()) {
            ensure!(
                !["/nand.bin", "/keys.bin"].contains(&path.folded().as_str()),
                "sd: {path} is where wiiform puts the NAND and its keys"
            );
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
