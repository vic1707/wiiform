// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_saphyr::MergeKeyPolicy;

pub const MAX_EVENTS: usize = 20_000;
pub const MAX_DEPTH: usize = 24;

/// Bytes stored as a YAML `!!binary` scalar.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Binary(
    #[serde(
        serialize_with = "serialize_binary",
        deserialize_with = "deserialize_binary"
    )]
    #[schemars(with = "String", extend("contentEncoding" = "base64"))]
    pub Vec<u8>,
);

impl AsRef<[u8]> for Binary {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn serialize_binary<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(bytes)
}

fn deserialize_binary<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let serde_saphyr::Tagged(bytes, tag) = serde_saphyr::Tagged::deserialize(deserializer)?;
    if tag.as_deref() != Some("tag:yaml.org,2002:binary") {
        return Err(serde::de::Error::custom("expected a YAML !!binary scalar"));
    }
    Ok(bytes)
}

pub fn options() -> serde_saphyr::Options {
    serde_saphyr::options! {
        merge_keys: MergeKeyPolicy::Error,
        reject_unsupported_tags: true,
        strict_booleans: true,
        no_schema: true,
        with_snippet: false,
        budget: serde_saphyr::budget! {
            max_events: MAX_EVENTS,
            max_aliases: 0,
            max_anchors: 0,
            max_depth: MAX_DEPTH,
            max_documents: 1,
            max_nodes: 8_192,
            max_reader_input_bytes: None,
            max_total_scalar_bytes: usize::MAX,
            flow_nesting_limit: MAX_DEPTH,
        },
    }
}
