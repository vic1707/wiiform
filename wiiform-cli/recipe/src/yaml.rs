// SPDX-License-Identifier: GPL-2.0-only
use serde::{Deserialize, Serialize, Serializer};
use serde_saphyr::{DuplicateKeyPolicy, MergeKeyPolicy, Options};

pub const MAX_EVENTS: usize = 20_000;
pub const MAX_DEPTH: usize = 24;

/// Bytes stored as a YAML `!!binary` scalar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binary(pub Vec<u8>);

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

pub fn options() -> Options {
    let mut options = Options::default();
    options.duplicate_keys = DuplicateKeyPolicy::Error;
    options.merge_keys = MergeKeyPolicy::Error;
    options.reject_unsupported_tags = true;
    options.strict_booleans = true;
    options.no_schema = true;
    options.with_snippet = false;
    options.crop_radius = 0;

    let mut budget = serde_saphyr::Budget::default();
    budget.max_events = MAX_EVENTS;
    budget.max_aliases = 0;
    budget.max_anchors = 0;
    budget.max_depth = MAX_DEPTH;
    budget.max_documents = 1;
    budget.max_nodes = 8_192;
    budget.max_reader_input_bytes = None;
    budget.max_total_scalar_bytes = usize::MAX;
    budget.flow_nesting_limit = MAX_DEPTH;
    options.budget = Some(budget);
    options
}
