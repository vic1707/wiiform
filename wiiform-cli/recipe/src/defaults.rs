// SPDX-License-Identifier: GPL-2.0-only
//! Values a recipe leaves out and Wiiform fills in, so a build can say so.
//!
//! Each such key has its own serde default function, which serde calls only
//! when the key is missing; it notes the key and the value it returns.
//! Absent sections, empty lists and safety switches left off are not noted.
use std::cell::RefCell;

use serde::Serialize;
use serde_json::Value;

/// A key the recipe left out, and the value Wiiform used for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Defaulted {
    /// Its path in the recipe, such as `nand.setup.nickname`.
    pub key: &'static str,
    /// As the recipe would spell it.
    pub value: String,
}

thread_local! {
    static DEFAULTED: RefCell<Option<Vec<Defaulted>>> = const { RefCell::new(None) };
}

/// Runs `read`, also returning the keys it defaulted.
pub(crate) fn collect<T>(read: impl FnOnce() -> T) -> (T, Vec<Defaulted>) {
    let outer = DEFAULTED.replace(Some(Vec::new()));
    let result = read();
    let defaulted = DEFAULTED.replace(outer).unwrap_or_default();
    (result, defaulted)
}

/// Notes that the recipe left `key` out, and returns `value`.
pub(crate) fn record<T: Serialize>(key: &'static str, value: T) -> T {
    DEFAULTED.with_borrow_mut(|defaulted| {
        if let Some(defaulted) = defaulted {
            let value = match serde_json::to_value(&value) {
                Ok(Value::String(text)) => text,
                Ok(other) => other.to_string(),
                Err(error) => format!("<{error}>"),
            };
            defaulted.push(Defaulted { key, value });
        }
    });
    value
}

/// Declares serde default functions that note their key.
macro_rules! recorded {
    ($($name:ident: $type:ty = $key:literal;)+) => {$(
        pub(super) fn $name() -> $type {
            $crate::defaults::record($key, <$type>::default())
        }
    )+};
}
pub(crate) use recorded;
