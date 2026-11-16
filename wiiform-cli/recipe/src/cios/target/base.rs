// SPDX-License-Identifier: GPL-2.0-only
use derive_more::Display;
use nnn::nnn;
use schemars::JsonSchema;
use serde::Deserialize;

#[nnn(
    attrs(doc = "Stock IOS a d2x cIOS is built on.", schemars(extend("enum" = [38, 56, 57, 58]))),
    derive(Clone, Copy, Debug, Display, Eq, PartialEq, Hash, Deserialize, JsonSchema),
    nnn_derive(TryFrom, FromStr, Into),
    validators(predicate(with = |base: &u8| [38, 56, 57, 58].contains(base), error_name = NotAD2xBase))
)]
pub struct Base(u8);

// clap needs this to show the message; nnn 1.2.7 does not implement it.
impl std::error::Error for BaseParseError {}
