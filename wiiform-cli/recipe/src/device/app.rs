// SPDX-License-Identifier: GPL-2.0-only
//! A Homebrew Channel app: the folder apps/<name>/.
use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, de::IgnoredAny};

use super::{CardPath, extract::Extract, read};
use crate::archive;
use crate::{github, source::Source, wad::Sha256};

/// What an app is made of: an archive (one source) or loose files.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Content {
    /// Local archive, relative to the recipe.
    Path(PathBuf),
    /// HTTPS URL of the archive.
    Url(String),
    /// The archive itself, base64-encoded.
    Inline(
        #[serde(deserialize_with = "crate::source::base64")]
        #[schemars(with = "String", extend("contentEncoding" = "base64"))]
        Vec<u8>,
    ),
    /// An archive attached to a tagged GitHub release.
    Github(github::Asset),
    /// Loose files instead of an archive, by path inside the app folder.
    Files(BTreeMap<String, LooseFile>),
}

/// One loose file of an app.
#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct LooseFile {
    #[serde(flatten)]
    source: Source,
    /// SHA-256 of the exact file. Optional for a GitHub release asset whose
    /// release states one; required otherwise.
    sha256: Option<Sha256>,
    // Serde cannot reject unknown keys next to flattened fields; they land here.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: BTreeMap<String, IgnoredAny>,
}

/// A Homebrew Channel app. From an archive, its folder holding boot.dol or
/// boot.elf becomes apps/<name>/, unless `extract` says what goes where.
#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub(super) struct App {
    #[serde(flatten)]
    content: Content,
    /// SHA-256 of the exact archive. Optional for a GitHub release asset whose
    /// release states one; required otherwise.
    sha256: Option<Sha256>,
    /// Which archive files go where, relative to the app folder (/ is the folder
    /// itself). Without it, the archive must hold exactly one app folder.
    extract: Option<Extract>,
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: BTreeMap<String, IgnoredAny>,
}

impl App {
    /// The app's files on the device.
    pub(super) fn files(self, name: &str) -> Result<Vec<(CardPath, Vec<u8>)>> {
        if let Some(key) = self.unknown.keys().next() {
            bail!("unknown field `{key}`");
        }
        let folder = CardPath::try_from(format!("apps/{name}"))?;
        ensure!(
            folder.to_string().matches('/').count() == 2,
            "app name {name} must be a single folder name"
        );
        let source = match self.content {
            Content::Files(files) => {
                ensure!(
                    self.sha256.is_none() && self.extract.is_none(),
                    "`files` takes no sha256 or extract: give them per file"
                );
                return with_boot_file(&folder, loose(&folder, files)?);
            }
            Content::Path(path) => Source::Path(path),
            Content::Url(url) => Source::Url(url),
            Content::Inline(bytes) => Source::Inline(bytes),
            Content::Github(asset) => Source::Github(asset),
        };
        let files = archive::files(&read(source, self.sha256)?)?;
        let placed = match self.extract {
            Some(extract) => extract.place(files, &folder)?,
            None => app_folder(files, &folder)?,
        };
        with_boot_file(&folder, placed)
    }
}

fn loose(
    folder: &CardPath,
    files: BTreeMap<String, LooseFile>,
) -> Result<Vec<(CardPath, Vec<u8>)>> {
    (files.into_iter())
        .map(|(name, file)| {
            if let Some(key) = file.unknown.keys().next() {
                bail!("{name}: unknown field `{key}`");
            }
            let bytes = read(file.source, file.sha256).with_context(|| name.clone())?;
            Ok((folder.join(&name)?, bytes))
        })
        .collect()
}

fn is_boot_file(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or_default().to_lowercase();
    base == "boot.dol" || base == "boot.elf"
}

/// Moves the archive's one folder holding boot.dol or boot.elf to `folder`.
fn app_folder(
    files: Vec<(String, Vec<u8>)>,
    folder: &CardPath,
) -> Result<Vec<(CardPath, Vec<u8>)>> {
    let mut roots: Vec<String> = (files.iter())
        .filter(|(name, _)| is_boot_file(name))
        .map(|(name, _)| {
            name.rsplit_once('/')
                .map_or(String::new(), |(parent, _)| format!("{parent}/"))
        })
        .collect();
    roots.sort();
    roots.dedup();
    let root = match roots.as_slice() {
        [root] => root.clone(),
        [] => bail!("the archive holds no boot.dol or boot.elf"),
        _ => bail!(
            "the archive holds several app folders ({}): choose with extract",
            roots.join(", ")
        ),
    };
    (files.into_iter())
        .map(|(name, bytes)| match name.strip_prefix(&root) {
            Some(relative) => Ok((folder.join(relative)?, bytes)),
            None => bail!("{name} is outside the app folder {root}: map it with extract, or list the archive under bundles"),
        })
        .collect()
}

fn with_boot_file(
    folder: &CardPath,
    files: Vec<(CardPath, Vec<u8>)>,
) -> Result<Vec<(CardPath, Vec<u8>)>> {
    let prefix = format!("{}/", folder.folded());
    let has_boot = files.iter().any(|(path, _)| {
        let path = path.folded();
        path.strip_prefix(&prefix)
            .is_some_and(|rest| rest == "boot.dol" || rest == "boot.elf")
    });
    ensure!(has_boot, "{folder} gets no boot.dol or boot.elf");
    Ok(files)
}
