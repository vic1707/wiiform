// SPDX-License-Identifier: GPL-2.0-only
use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Menu;

use super::TitleId;

/// Channel shortcuts supported by `remove`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum RemovalName {
    /// Expand to the forecast channel title for the selected menu and its common stub.
    #[serde(rename = "forecast-channel")]
    ForecastChannel,
    /// Expand to the news channel title for the selected menu and its common stub.
    #[serde(rename = "news-channel")]
    NewsChannel,
    /// Wii Shop Channel (HABA).
    #[serde(rename = "wii-shop-channel")]
    WiiShopChannel,
    /// Photo Channel versions HAAA and HAYA.
    #[serde(rename = "photo-channel")]
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

const REGIONAL_CHANNELS: [(&str, &str); 2] = [
    ("forecast-channel", "00010002484146"),
    ("news-channel", "00010002484147"),
];
const COMMON_CHANNELS: [(&str, &[&str]); 2] = [
    ("wii-shop-channel", &["0001000248414241"]),
    ("photo-channel", &["0001000248414141", "0001000248415941"]),
];

impl RemovalInput {
    pub(crate) fn expand(&self, menu: Menu) -> anyhow::Result<Vec<TitleId>> {
        Ok(match self {
            Self::Name(RemovalName::ForecastChannel) => {
                vec![
                    title_id(&format!("{}{}", REGIONAL_CHANNELS[0].1, menu.suffix())),
                    title_id(&format!("{}41", REGIONAL_CHANNELS[0].1)),
                ]
            }
            Self::Name(RemovalName::NewsChannel) => {
                vec![
                    title_id(&format!("{}{}", REGIONAL_CHANNELS[1].1, menu.suffix())),
                    title_id(&format!("{}41", REGIONAL_CHANNELS[1].1)),
                ]
            }
            Self::Name(RemovalName::WiiShopChannel) => vec![title_id(COMMON_CHANNELS[0].1[0])],
            Self::Name(RemovalName::PhotoChannel) => {
                COMMON_CHANNELS[1].1.iter().map(|id| title_id(id)).collect()
            }
            Self::Id(id) => {
                anyhow::ensure!(
                    allowlisted_title_ids().contains(id.as_str()),
                    "remove contains a title ID outside the supported channel allowlist"
                );
                vec![id.clone()]
            }
        })
    }
}

fn allowlisted_title_ids() -> BTreeSet<&'static str> {
    let mut result = BTreeSet::new();
    for (_, base) in REGIONAL_CHANNELS {
        for suffix in ["50", "45", "4a", "41"] {
            result.insert(match suffix {
                "50" => {
                    if base.ends_with("46") {
                        "0001000248414650"
                    } else {
                        "0001000248414750"
                    }
                }
                "45" => {
                    if base.ends_with("46") {
                        "0001000248414645"
                    } else {
                        "0001000248414745"
                    }
                }
                "4a" => {
                    if base.ends_with("46") {
                        "000100024841464a"
                    } else {
                        "000100024841474a"
                    }
                }
                _ => {
                    if base.ends_with("46") {
                        "0001000248414641"
                    } else {
                        "0001000248414741"
                    }
                }
            });
        }
    }
    for (_, ids) in COMMON_CHANNELS {
        result.extend(ids.iter().copied());
    }
    result
}

fn title_id(value: &str) -> TitleId {
    value
        .parse()
        .expect("internal allowlisted title ID must be valid")
}
