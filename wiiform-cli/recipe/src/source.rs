// SPDX-License-Identifier: GPL-2.0-only
//! Where a recipe gets a file from, shared by WADs and device files.
use std::{
    cell::RefCell,
    fs::{self, File},
    io::Write,
    ops::RangeInclusive,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

use crate::{
    blob::Hashed,
    device::Collisions,
    files::open_regular_file,
    github, read_bounded,
    wad::{Sha256, download, fetch},
};

const NEEDS_SHA256: &str =
    "sha256 is required for a url, and for a GitHub release asset whose release states none";
const MISMATCH: &str = "file does not match its SHA-256";

/// Largest file read into memory: a NAND's size. Device files are staged on disk
/// instead and have no bound of their own.
pub(crate) const MEMORY_BOUNDS: RangeInclusive<usize> = 1..=0x2100_0000;

#[derive(Default)]
struct Scope {
    /// Directory of the recipe being read; its relative paths start there.
    directory: PathBuf,
    /// Where verified downloads wait, by SHA-256.
    store: Option<PathBuf>,
    /// What happens when device entries collide.
    collisions: Collisions,
}

thread_local! {
    static SCOPE: RefCell<Scope> = RefCell::new(Scope::default());
}

/// Runs `read` with relative paths resolved against `directory`, downloads kept
/// in `store`, and device collisions settled by `collisions`.
pub(crate) fn relative_to<T>(
    directory: PathBuf,
    store: PathBuf,
    collisions: Collisions,
    read: impl FnOnce() -> T,
) -> T {
    let previous = SCOPE.replace(Scope {
        directory,
        store: Some(store),
        collisions,
    });
    let result = read();
    SCOPE.set(previous);
    result
}

/// Where a recipe gets a file from.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    /// Local path, relative to the recipe.
    Path(PathBuf),
    /// HTTPS URL of the raw file.
    Url(String),
    /// The file itself, base64-encoded.
    Inline(
        #[serde(deserialize_with = "base64")]
        #[schemars(with = "String", extend("contentEncoding" = "base64"))]
        Vec<u8>,
    ),
    /// An asset of a tagged GitHub release.
    Github(github::Asset),
}

/// A file from one source, with its SHA-256 where the source needs one.
#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields, transform = crate::schema::sha256_for_url)]
pub(crate) struct Pinned {
    #[serde(flatten)]
    source: Source,
    /// SHA-256 of the exact file, checked when given. Required for a url, and
    /// for a GitHub release asset whose release states none.
    sha256: Option<Sha256>,
    // Serde cannot reject unknown keys next to flattened fields; they land here.
    #[serde(flatten)]
    #[schemars(skip)]
    unknown: std::collections::BTreeMap<String, serde::de::IgnoredAny>,
}

impl Pinned {
    pub(crate) fn read(self) -> Result<Vec<u8>> {
        if let Some(key) = self.unknown.keys().next() {
            bail!("unknown field `{key}`");
        }
        self.source.read_checked(self.sha256, MEMORY_BOUNDS)
    }
}

/// A file's bytes, and the SHA-256 its publisher states for them, if any.
pub(crate) struct Fetched {
    pub bytes: Vec<u8>,
    pub published_sha256: Option<Sha256>,
}

impl Source {
    /// A command-line file: a URL, or the path of an existing file.
    pub(crate) fn from_command_line(input: std::path::PathBuf) -> Result<Self> {
        Ok(match input.to_str().filter(|value| value.contains("://")) {
            Some(url) => Self::Url(url.to_owned()),
            None => Self::Path(crate::ResolvedPath::try_from(input)?.as_ref().to_owned()),
        })
    }

    /// Whether the recipe holds the file or names it on this machine. Its
    /// bytes are then whatever the recipe's author put there, so a SHA-256 is
    /// only checked when given.
    fn is_local(&self) -> bool {
        matches!(self, Self::Path(_) | Self::Inline(_))
    }

    /// Reads the file into memory, refusing one whose size is outside
    /// `bounds`, and checks it against the SHA-256 it must have.
    pub(crate) fn read_checked(
        self,
        sha256: Option<Sha256>,
        bounds: RangeInclusive<usize>,
    ) -> Result<Vec<u8>> {
        let local = self.is_local();
        // Nothing but the recipe could vouch for a URL: refuse before downloading.
        ensure!(
            sha256.is_some() || !matches!(self, Self::Url(_)),
            NEEDS_SHA256
        );
        let fetched = self.read(bounds)?;
        if let Some(expected) = expected_sha256(sha256, fetched.published_sha256, local)? {
            ensure!(Sha256::of(&fetched.bytes) == expected, MISMATCH);
        }
        Ok(fetched.bytes)
    }

    /// Reads the file into memory, refusing one whose size is outside `bounds`.
    pub(crate) fn read(self, bounds: RangeInclusive<usize>) -> Result<Fetched> {
        let (bytes, published_sha256) = match self {
            Self::Path(path) => {
                let path = SCOPE.with_borrow(|context| context.directory.join(path));
                (read_bounded(&path, bounds.clone())?, None)
            }
            Self::Url(url) => {
                let bytes =
                    fetch(&url, *bounds.end()).with_context(|| format!("cannot download {url}"))?;
                (bytes, None)
            }
            Self::Inline(bytes) => (bytes, None),
            Self::Github(asset) => {
                let release = asset.find()?;
                let bytes = fetch(&release.url, *bounds.end())
                    .with_context(|| format!("cannot download {}", release.url))?;
                (bytes, release.sha256)
            }
        };
        ensure!(
            bounds.contains(&bytes.len()),
            "file has an unsupported size"
        );
        Ok(Fetched {
            bytes,
            published_sha256,
        })
    }

    /// Puts the file on disk, verified, and returns its path. A local file stays
    /// where it is; anything else is stored under its SHA-256, so a file already
    /// there is not downloaded again.
    pub(crate) fn stage(self, sha256: Option<Sha256>) -> Result<PathBuf> {
        match self {
            Self::Path(path) => {
                let path = SCOPE.with_borrow(|context| context.directory.join(path));
                let (mut file, _) = open_regular_file(&path)?;
                if let Some(expected) = sha256 {
                    let mut hashed = Hashed::new(std::io::sink());
                    std::io::copy(&mut file, &mut hashed)
                        .with_context(|| format!("cannot read {}", path.display()))?;
                    ensure!(hashed.finish().1 == expected, MISMATCH);
                }
                Ok(path)
            }
            Self::Url(url) => {
                let expected = sha256.context(NEEDS_SHA256)?;
                store(expected, |out| download(&url, out))
                    .with_context(|| format!("cannot download {url}"))
            }
            Self::Inline(bytes) => match sha256 {
                Some(expected) => store(expected, |out| Ok(out.write_all(&bytes)?)),
                None => store_bytes(&bytes),
            },
            Self::Github(asset) => {
                let release = asset.find()?;
                let expected =
                    expected_sha256(sha256, release.sha256, false)?.context(NEEDS_SHA256)?;
                store(expected, |out| download(&release.url, out))
                    .with_context(|| format!("cannot download {}", release.url))
            }
        }
    }
}

/// The SHA-256 a file must have: the recipe's, which must agree with the one its
/// release states, if the release states one. Only a `local` file may have
/// none. A cached copy is only checked against this, so a disagreement is
/// refused before any copy is looked up.
fn expected_sha256(
    given: Option<Sha256>,
    published: Option<Sha256>,
    local: bool,
) -> Result<Option<Sha256>> {
    match (given, published) {
        (Some(given), Some(published)) => {
            ensure!(
                given == published,
                "the recipe's SHA-256 differs from the one the release states"
            );
            Ok(Some(given))
        }
        (None, None) => {
            ensure!(local, NEEDS_SHA256);
            Ok(None)
        }
        (given, published) => Ok(given.or(published)),
    }
}

/// What happens when device entries collide, as the recipe was read.
pub(crate) fn collisions() -> Collisions {
    SCOPE.with_borrow(|scope| scope.collisions)
}

/// Puts generated bytes in the store under their SHA-256 and returns the path.
pub(crate) fn store_bytes(bytes: &[u8]) -> Result<PathBuf> {
    store(Sha256::of(bytes), |out| Ok(out.write_all(bytes)?))
}

/// The recipe's store directory, created if needed.
fn store_directory() -> Result<PathBuf> {
    let store = SCOPE
        .with_borrow(|context| context.store.clone())
        .context("no directory is set for downloads")?;
    fs::create_dir_all(&store).with_context(|| format!("cannot create {}", store.display()))?;
    Ok(store)
}

/// Writes a file under its SHA-256 in the store, unless it is already there.
/// The bytes go to a partial file first, so an interrupted download leaves
/// nothing under the name a later build would trust.
fn store(expected: Sha256, write: impl FnOnce(&mut Hashed<File>) -> Result<()>) -> Result<PathBuf> {
    let name: String = expected
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let directory = store_directory()?;
    let target = directory.join(&name);
    if target.is_file() {
        return Ok(target);
    }
    // Each write has its own partial file, so concurrent writes of one digest do not share it.
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let writes = WRITES.fetch_add(1, Ordering::Relaxed);
    let partial = directory.join(format!(".partial-{name}-{}-{writes}", std::process::id()));
    let mut hashed = Hashed::new(File::create(&partial)?);
    let result = write(&mut hashed);
    let (file, actual) = hashed.finish();
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    if actual != expected {
        let _ = fs::remove_file(&partial);
        bail!(MISMATCH);
    }
    file.sync_all()?;
    fs::rename(&partial, &target).with_context(|| format!("cannot store {}", target.display()))?;
    Ok(target)
}

pub(crate) fn base64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = String::deserialize(deserializer)?;
    let compact: String = text.split_ascii_whitespace().collect();
    STANDARD.decode(compact).map_err(serde::de::Error::custom)
}
