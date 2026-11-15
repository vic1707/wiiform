// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Menu;

use super::TitleId;

/// Channel shortcuts supported by `remove`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RemovalName {
    /// Expand to the forecast channel title for the selected menu and its common stub.
    ForecastChannel,
    /// Expand to the news channel title for the selected menu and its common stub.
    NewsChannel,
    /// Wii Shop Channel (HABA).
    WiiShopChannel,
    /// Photo Channel versions HAAA and HAYA.
    PhotoChannel,
}

/// One removal entry as written in a recipe: a known name or quoted title ID.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RemovalInput {
    /// Supported channel name.
    Name(RemovalName),
    /// A quoted title ID from the removal allowlist.
    Id(TitleId),
}

const FORECAST: u64 = 0x0001000248414600;
const NEWS: u64 = 0x0001000248414700;
const SHOP: u64 = 0x0001000248414241;
const PHOTO: u64 = 0x0001000248414141;
const PHOTO_11: u64 = 0x0001000248415941;
const COMMON: u64 = b'A' as u64;

impl RemovalInput {
    pub(crate) fn expand(&self, menu: Menu) -> anyhow::Result<Vec<TitleId>> {
        Ok(match *self {
            Self::Name(RemovalName::ForecastChannel) => regional(FORECAST, menu),
            Self::Name(RemovalName::NewsChannel) => regional(NEWS, menu),
            Self::Name(RemovalName::WiiShopChannel) => vec![TitleId::known(SHOP)],
            Self::Name(RemovalName::PhotoChannel) => {
                vec![TitleId::known(PHOTO), TitleId::known(PHOTO_11)]
            }
            Self::Id(id) if removable(id.as_u64()) => vec![id],
            Self::Id(_) => {
                anyhow::bail!("remove contains a title ID outside the supported channel allowlist")
            }
        })
    }
}

fn regional(base: u64, menu: Menu) -> Vec<TitleId> {
    vec![
        TitleId::known(base | u64::from(menu.channel_suffix())),
        TitleId::known(base | COMMON),
    ]
}

fn removable(id: u64) -> bool {
    matches!(id, SHOP | PHOTO | PHOTO_11)
        || (matches!(id & !0xff, FORECAST | NEWS) && matches!(id as u8, b'A' | b'E' | b'J' | b'P'))
}
