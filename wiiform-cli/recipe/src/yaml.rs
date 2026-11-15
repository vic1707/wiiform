// SPDX-License-Identifier: GPL-2.0-only
use serde::{Deserialize, Serialize, Serializer};
use serde_saphyr::MergeKeyPolicy;

pub const MAX_EVENTS: usize = 20_000;
pub const MAX_DEPTH: usize = 24;

/// Bytes stored as a YAML `!!binary` scalar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binary(pub Vec<u8>);

impl AsRef<[u8]> for Binary {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl schemars::JsonSchema for Binary {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Binary".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "contentEncoding": "base64",
            "description": "Bytes encoded as a YAML !!binary scalar."
        })
    }
}

impl Serialize for Binary {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Binary {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let tagged = serde_saphyr::Tagged::<Vec<u8>>::deserialize(deserializer)?;
        if tagged.1.as_deref() != Some("tag:yaml.org,2002:binary") {
            return Err(serde::de::Error::custom("expected a YAML !!binary scalar"));
        }
        Ok(Self(tagged.0))
    }
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
