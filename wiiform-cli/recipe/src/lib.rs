// SPDX-License-Identifier: GPL-2.0-only
//! Declarative recipe types, validation, and editor schema.

mod model;
mod parser;
pub mod priiloader;
mod schema;
pub mod system;
pub mod titles;
pub mod yaml;

pub use model::{Recipe, RecipeFile};
pub use parser::{RecipeError, load_recipe, open_regular_file, parse_recipe, resolve_regular_file};
pub use priiloader::{
    Autoboot, HackSettings, MountPoint, Priiloader, PriiloaderFile, PriiloaderHack,
    PriiloaderSettings, ReturnTo, ShutdownTo,
};
pub use schema::json_schema;
pub use system::{Aspect, CountryCode, Display, Language, Menu, Nickname, SensorBar, Setup};
pub use titles::{
    RemovalInput, RemovalName, Sha256, SignaturePolicy, Title, TitleFile, TitleId, WadSource,
};
