// SPDX-License-Identifier: GPL-2.0-only
//! System Menu, first-run setup, and display settings.

mod country;
mod display;
mod language;
mod menu;
mod nickname;
mod setup;

pub use country::CountryCode;
pub use display::{Aspect, Display};
pub use language::Language;
pub use menu::Menu;
pub use nickname::Nickname;
pub use setup::{SensorBar, Setup};
