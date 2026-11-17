// SPDX-License-Identifier: GPL-2.0-only
use std::{fmt, path::PathBuf};

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

/// A path on the SD card, relative to its root. A leading / is accepted and
/// means the root; empty, `.` and `..` components are not.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, JsonSchema)]
#[serde(try_from = "String")]
#[schemars(with = "String")]
pub struct CardPath(Vec<String>);

impl CardPath {
    /// The root of the card.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// `relative` (a path inside a zip) below this directory.
    pub(crate) fn join(&self, relative: &str) -> Result<Self> {
        let mut joined = self.clone();
        joined.0.extend(Self::parse(relative, false)?.0);
        Ok(joined)
    }

    /// `other` below this directory.
    pub(crate) fn join_path(&self, other: &CardPath) -> Result<Self> {
        let mut joined = self.clone();
        joined.0.extend(other.0.iter().cloned());
        Ok(joined)
    }

    /// The path as FAT compares it: ignoring case.
    pub fn folded(&self) -> String {
        self.to_string().to_lowercase()
    }

    /// This path below `root`.
    pub fn under(&self, root: &std::path::Path) -> PathBuf {
        self.0
            .iter()
            .fold(root.to_owned(), |path, part| path.join(part))
    }

    fn parse(text: &str, allow_leading_slash: bool) -> Result<Self> {
        let text = match text.strip_prefix('/') {
            Some(rest) if allow_leading_slash => rest,
            Some(_) => bail!("{text} is absolute"),
            None => text,
        };
        let text = text.strip_suffix('/').unwrap_or(text);
        if text.is_empty() {
            return Ok(Self::default());
        }
        let parts: Vec<String> = text.split('/').map(str::to_owned).collect();
        for part in &parts {
            ensure!(
                !matches!(part.as_str(), "" | "." | ".."),
                "{text} has an empty, . or .. component"
            );
            // Characters FAT cannot store, and the path separators of other systems.
            ensure!(
                !part
                    .chars()
                    .any(|ch| ch.is_control() || "\\:*?\"<>|".contains(ch)),
                "{text} has a character an SD card cannot store"
            );
        }
        Ok(Self(parts))
    }
}

impl TryFrom<String> for CardPath {
    type Error = anyhow::Error;

    fn try_from(text: String) -> Result<Self> {
        Self::parse(&text, true)
    }
}

impl fmt::Display for CardPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", self.0.join("/"))
    }
}
