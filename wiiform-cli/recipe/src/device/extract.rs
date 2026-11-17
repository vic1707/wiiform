// SPDX-License-Identifier: GPL-2.0-only
//! Glob → directory pairs choosing what an archive puts where.
use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use globset::{Glob, GlobMatcher};
use schemars::JsonSchema;
use serde::Deserialize;

use super::CardPath;

/// Which archive files go where. Each key is a glob over paths inside the
/// archive (`*`, `**`, `?`, `[…]`, `{a,b}`); each value a directory. A matching
/// file keeps its path below the glob's fixed leading folders: with
/// `"pkg/data/**": /data/`, `pkg/data/x/y` lands at `/data/x/y`. Files no glob
/// matches are left out; a glob matching nothing, or a file matching two, is
/// an error.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(try_from = "BTreeMap<String, CardPath>")]
#[schemars(with = "BTreeMap<String, CardPath>")]
pub(super) struct Extract(Vec<Pair>);

#[derive(Debug)]
struct Pair {
    glob: String,
    matcher: GlobMatcher,
    /// The leading folders without wildcards, stripped from matching paths.
    prefix: String,
    directory: CardPath,
}

impl TryFrom<BTreeMap<String, CardPath>> for Extract {
    type Error = anyhow::Error;

    fn try_from(pairs: BTreeMap<String, CardPath>) -> Result<Self> {
        ensure!(!pairs.is_empty(), "extract needs at least one glob");
        let pairs = pairs.into_iter().map(|(glob, directory)| {
            let matcher = globset::GlobBuilder::new(&glob)
                .literal_separator(true)
                .build()
                .map(|built: Glob| built.compile_matcher())?;
            let parts: Vec<&str> = glob.split('/').collect();
            let fixed = parts
                .iter()
                .take_while(|part| !part.contains(['*', '?', '[', '{']))
                .count();
            // A glob without wildcards names one file: keep only its name.
            let fixed = fixed.min(parts.len() - 1);
            let prefix = parts[..fixed]
                .iter()
                .map(|part| format!("{part}/"))
                .collect();
            Ok(Pair {
                glob,
                matcher,
                prefix,
                directory,
            })
        });
        Ok(Self(pairs.collect::<Result<_>>()?))
    }
}

impl Extract {
    /// Places the archive's files below `base`.
    pub(super) fn place(
        &self,
        files: Vec<(String, Vec<u8>)>,
        base: &CardPath,
    ) -> Result<Vec<(CardPath, Vec<u8>)>> {
        let mut used = vec![false; self.0.len()];
        let mut placed = Vec::new();
        for (name, bytes) in files {
            let matching: Vec<usize> = (0..self.0.len())
                .filter(|&index| self.0[index].matcher.is_match(&name))
                .collect();
            let index = match matching.as_slice() {
                [] => continue,
                [index] => *index,
                [first, second, ..] => bail!(
                    "{name} matches both {} and {}",
                    self.0[*first].glob,
                    self.0[*second].glob
                ),
            };
            used[index] = true;
            let pair = &self.0[index];
            let relative = name.strip_prefix(&pair.prefix).unwrap_or(&name);
            placed.push((base.join_path(&pair.directory)?.join(relative)?, bytes));
        }
        if let Some(index) = used.iter().position(|used| !used) {
            bail!("{} matches no file in the archive", self.0[index].glob);
        }
        Ok(placed)
    }
}
