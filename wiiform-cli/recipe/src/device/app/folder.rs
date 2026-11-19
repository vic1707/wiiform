// SPDX-License-Identifier: GPL-2.0-only
//! The archive folder that becomes an app's folder: the `folder` key.
use anyhow::{Result, bail};
use globset::{Glob, GlobMatcher};
use schemars::JsonSchema;
use serde::Deserialize;

use super::CardPath;

/// A glob over the archive's folders (`*`, `**`, `?`, `[…]`, `{a,b}`; `*`
/// stops at `/`) that must match exactly one, such as `"*/wii/gcmm-dark"`.
/// That folder's files become apps/<name>/; the rest of the archive is left out.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "String")]
#[schemars(with = "String")]
pub(super) struct Folder {
    glob: String,
    matcher: GlobMatcher,
}

impl TryFrom<String> for Folder {
    type Error = anyhow::Error;

    fn try_from(glob: String) -> Result<Self> {
        let trimmed = glob.strip_suffix('/').unwrap_or(&glob);
        if trimmed.is_empty() || trimmed.starts_with('/') {
            bail!("folder {glob:?} must name a folder inside the archive, without a leading /");
        }
        let matcher = (globset::GlobBuilder::new(trimmed)
            .literal_separator(true)
            .build())
        .map(|built: Glob| built.compile_matcher())?;
        Ok(Self {
            glob: trimmed.to_owned(),
            matcher,
        })
    }
}

impl Folder {
    /// Places the files of the one folder the glob matches in `app`.
    pub(super) fn place<T>(
        &self,
        files: Vec<(String, T)>,
        app: &CardPath,
    ) -> Result<Vec<(CardPath, T)>> {
        // Every folder of the archive: the parents of its files.
        let mut folders: Vec<&str> = (files.iter())
            .flat_map(|(name, _)| name.match_indices('/').map(|(end, _)| &name[..end]))
            .filter(|folder| self.matcher.is_match(folder))
            .collect();
        folders.sort_unstable();
        folders.dedup();
        let root = match folders.as_slice() {
            [root] => format!("{root}/"),
            [] => bail!("folder {} matches no folder in the archive", self.glob),
            several => bail!(
                "folder {} matches several folders ({}): narrow it to one",
                self.glob,
                several.join(", ")
            ),
        };
        (files.into_iter())
            .filter_map(|(name, payload)| {
                let relative = name.strip_prefix(&root)?;
                Some(app.join(relative).map(|path| (path, payload)))
            })
            .collect()
    }
}
