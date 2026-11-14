// SPDX-License-Identifier: GPL-2.0-only
//! WAD requests, title identifiers, and channel removals.

mod removal;
mod sha256;
mod title_id;

pub use removal::{RemovalInput, RemovalName};
pub use sha256::Sha256;
pub use title_id::TitleId;

pub(crate) use removal::allowlisted_title_ids;

/// Maximum raw title payload size accepted by the native Wii title installer.
const MAX_WAD_BYTES: usize = 0x2100_0000;

use std::{borrow::Cow, io::Read, path::PathBuf};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;

use crate::yaml::Binary;

/// WAD signature requirements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
pub enum SignaturePolicy {
    /// Require Nintendo signatures.
    #[default]
    #[serde(rename = "nintendo")]
    Nintendo,
    /// Permit an unsigned community channel WAD after content validation.
    #[serde(rename = "allow-unsigned")]
    AllowUnsigned,
}

/// Exact WAD bytes provided inline in YAML or loaded from a local file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WadSource {
    File(PathBuf),
    Inline(Vec<u8>),
}

impl WadSource {
    /// Borrow inline data or read the selected file when the title is installed.
    pub fn resolve(&self) -> anyhow::Result<Cow<'_, [u8]>> {
        match self {
            Self::File(path) => {
                let file = crate::open_regular_file(path)?;
                let mut bytes = Vec::with_capacity(0x40);
                file.take((MAX_WAD_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)?;
                anyhow::ensure!(
                    bytes.len() >= 0x40,
                    "WAD file is shorter than the 0x40-byte header"
                );
                anyhow::ensure!(
                    bytes.len() <= MAX_WAD_BYTES,
                    "WAD file exceeds the maximum size of {MAX_WAD_BYTES} bytes"
                );
                Ok(Cow::Owned(bytes))
            }
            Self::Inline(bytes) => Ok(Cow::Borrowed(bytes)),
        }
    }

    pub fn file_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Inline(_) => None,
        }
    }
}

/// A WAD installation selected by a recipe.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "TitleFileWire")]
pub struct TitleFile {
    pub source: WadSource,
    /// Required SHA-256 of the exact WAD file bytes.
    pub sha256: Sha256,
    /// Signature policy; defaults to `nintendo`.
    pub signature_policy: SignaturePolicy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TitleFileWire {
    #[serde(default, deserialize_with = "crate::schema::non_null_option")]
    path: Option<PathBuf>,
    #[serde(default, deserialize_with = "crate::schema::non_null_option")]
    inline: Option<Binary>,
    sha256: Sha256,
    #[serde(default)]
    signature_policy: SignaturePolicy,
}

impl TryFrom<TitleFileWire> for TitleFile {
    type Error = &'static str;

    fn try_from(wire: TitleFileWire) -> Result<Self, Self::Error> {
        let source = match (wire.path, wire.inline) {
            (Some(path), None) => WadSource::File(path),
            (None, Some(inline)) => WadSource::Inline(inline.0),
            _ => return Err("a title must specify exactly one of `path` or `inline`"),
        };
        Ok(Self {
            source,
            sha256: wire.sha256,
            signature_policy: wire.signature_policy,
        })
    }
}

impl JsonSchema for TitleFile {
    fn schema_name() -> Cow<'static, str> {
        "TitleFile".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::TitleFile").into()
    }

    fn inline_schema() -> bool {
        false
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let sha256 = generator.subschema_for::<Sha256>();
        let signature_policy = generator.subschema_for::<SignaturePolicy>();
        schemars::json_schema!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Local WAD path, relative to the recipe file."},
                        "sha256": sha256.clone(),
                        "signature_policy": signature_policy.clone()
                    },
                    "required": ["path", "sha256"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "inline": {"type": "string", "contentEncoding": "base64", "contentMediaType": "application/octet-stream", "description": "Exact WAD bytes encoded as a YAML !!binary scalar."},
                        "sha256": sha256,
                        "signature_policy": signature_policy
                    },
                    "required": ["inline", "sha256"],
                    "additionalProperties": false
                }
            ],
            "description": "A WAD supplied by exactly one local path or inline !!binary data, with a SHA-256 of the exact bytes."
        })
    }
}

/// A resolved title request passed from recipe parsing to the workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Title {
    pub source: WadSource,
    pub sha256: Sha256,
    pub signature_policy: SignaturePolicy,
}
