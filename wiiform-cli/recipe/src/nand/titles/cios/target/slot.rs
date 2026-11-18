// SPDX-License-Identifier: GPL-2.0-only
use derive_more::Display;
use nnn::nnn;
use schemars::JsonSchema;
use serde::Deserialize;

#[nnn(
    attrs(doc = "IOS slot a cIOS may be installed to.", schemars(extend("enum" = [248, 249, 250, 251]))),
    derive(Clone, Copy, Debug, Display, Eq, PartialEq, Hash, Deserialize, JsonSchema),
    nnn_derive(TryFrom, FromStr, Into),
    validators(min_or_eq = 248, max_or_eq = 251)
)]
pub struct Slot(u8);
