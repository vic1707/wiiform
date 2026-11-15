// SPDX-License-Identifier: GPL-2.0-only
//! Declarative recipe types, validation, and editor schema.

mod data;
mod files;
mod hbc;
mod model;
pub mod priiloader;
mod schema;
pub mod system;
pub mod titles;
pub mod yaml;

pub use data::Data;
pub use files::{ResolvedPath, open_regular_file, read_bounded};
pub use hbc::Hbc;
pub use model::Recipe;
pub use priiloader::{
    Autoboot, HackSettings, MountPoint, Priiloader, PriiloaderHack, PriiloaderSettings, ReturnTo,
    ShutdownTo,
};
pub use schema::json_schema;
pub use system::{Aspect, CountryCode, Display, Language, Menu, Nickname, SensorBar, Setup};
pub use titles::{
    Cios, CiosOptions, CiosTarget, RemovalInput, RemovalName, Sha256, SignaturePolicy, Title,
    TitleId, TitleOptions, Wad,
};
