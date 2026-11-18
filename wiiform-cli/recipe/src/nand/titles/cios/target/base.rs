// SPDX-License-Identifier: GPL-2.0-only
use derive_more::Display;
use nnn::nnn;
use schemars::JsonSchema;
use serde::Deserialize;

#[nnn(
    attrs(doc = "Stock IOS a d2x cIOS is built on.", schemars(extend("enum" = [38, 56, 57, 58]))),
    derive(Clone, Copy, Debug, Display, Eq, PartialEq, Hash, Deserialize, JsonSchema),
    nnn_derive(TryFrom, FromStr, Into),
    validators(one_of = [38, 56, 57, 58])
)]
pub struct Base(u8);
