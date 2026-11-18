// SPDX-License-Identifier: GPL-2.0-only
//! Nintendo IOS from NUS: the `titles.ios` key.
use nnn::nnn;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::TitleId;

#[nnn(
    attrs(doc = "IOS slot, 3 through 253.", schemars(extend("minimum" = 3, "maximum" = 253))),
    derive(Clone, Copy, Debug, derive_more::Display, Eq, PartialEq, Hash, Deserialize, JsonSchema),
    nnn_derive(TryFrom, Into),
    validators(min_or_eq = 3, max_or_eq = 253)
)]
pub struct IosSlot(u8);

/// A Nintendo IOS downloaded from NUS. An identical installed IOS is only
/// verified; another version in the slot stops the build.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ios {
    pub slot: IosSlot,
    /// Title version, such as 1034 for IOS9.
    pub version: u16,
}

impl Ios {
    pub fn title_id(&self) -> TitleId {
        let slot: u8 = self.slot.into();
        TitleId::known(0x0000_0001_0000_0000 | u64::from(slot))
    }
}
