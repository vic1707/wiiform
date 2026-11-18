// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Screen aspect ratio.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Aspect {
    #[default]
    #[serde(rename = "4:3")]
    FourThree,
    #[serde(rename = "16:9")]
    SixteenNine,
}

/// Display configuration applied during first-run setup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Display {
    /// Set 4:3 or widescreen output (defaults to 4:3).
    #[serde(default = "defaults::aspect")]
    pub aspect: Aspect,
    /// Enable progressive 480p output when supported (defaults to off).
    #[serde(default = "defaults::progressive")]
    pub progressive: bool,
    /// Enable PAL 60 Hz mode (defaults to off).
    #[serde(default = "defaults::pal60")]
    pub pal60: bool,
}

/// Serde defaults for the keys a recipe leaves out, noted for the build log.
pub(super) mod defaults {
    use super::{Aspect, Display};
    use crate::defaults::recorded;

    recorded! {
        aspect: Aspect = "nand.setup.display.aspect";
        progressive: bool = "nand.setup.display.progressive";
        pal60: bool = "nand.setup.display.pal60";
    }

    /// The whole mapping left out: each of its keys is.
    pub(in crate::nand::setup) fn display() -> Display {
        Display {
            aspect: aspect(),
            progressive: progressive(),
            pal60: pal60(),
        }
    }
}
