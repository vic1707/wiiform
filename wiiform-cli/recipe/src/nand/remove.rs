// SPDX-License-Identifier: GPL-2.0-only
//! Stock channels to take off the image: the `remove` key.
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Menu, TitleId};

/// A stock channel that `remove` can take off the image.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RemovableChannel {
    /// The menu region's Forecast Channel and its common stub.
    ForecastChannel,
    /// The menu region's News Channel and its common stub.
    NewsChannel,
    /// Wii Shop Channel (HABA).
    WiiShopChannel,
    /// Photo Channel versions HAAA and HAYA.
    PhotoChannel,
}

impl RemovableChannel {
    /// The titles that make up this channel on a console of the menu's region.
    pub fn title_ids(self, menu: Menu) -> Vec<TitleId> {
        let regional = |base: u64| {
            [menu.channel_suffix(), b'A']
                .map(|suffix| TitleId::known(base | u64::from(suffix)))
                .to_vec()
        };
        match self {
            Self::ForecastChannel => regional(0x0001_0002_4841_4600),
            Self::NewsChannel => regional(0x0001_0002_4841_4700),
            Self::WiiShopChannel => vec![TitleId::known(0x0001_0002_4841_4241)],
            Self::PhotoChannel => vec![
                TitleId::known(0x0001_0002_4841_4141),
                TitleId::known(0x0001_0002_4841_5941),
            ],
        }
    }
}
