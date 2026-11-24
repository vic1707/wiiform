// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::Deserialize;

use super::Sha256;
use crate::source::Source;

/// A WAD from one source, with the SHA-256 of its exact bytes where the source
/// needs one.
// When a WAD is flattened into an entry, this is where that entry's unknown
// keys are rejected: it sees every key the entry does not name itself, and
// serde cannot make the same check on the outer struct.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Wad", transform = crate::schema::sha256_for_url)]
pub(crate) struct WadFile {
    #[serde(flatten)]
    pub(super) source: Source,
    /// SHA-256 of the exact WAD bytes, checked when given. Required for a url,
    /// and for a GitHub release asset whose release states none.
    pub(super) sha256: Option<Sha256>,
}
