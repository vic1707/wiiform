// SPDX-License-Identifier: GPL-2.0-only
mod ffi;
pub mod native;

pub mod prepared_verify;
pub mod priiloader_payload;
pub use wiiform_recipe as recipe;
pub mod stock_verify;

pub mod workflow;

fn sha256_hex(digest: sha2::digest::Output<sha2::Sha256>) -> String {
    use std::fmt::Write as _;

    let mut text = String::with_capacity(64);
    for byte in digest {
        write!(text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}
