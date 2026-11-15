// SPDX-License-Identifier: GPL-2.0-only
use std::{
    fs::{File, Metadata, OpenOptions},
    io::Read,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

/// A canonical path checked to point to a regular file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPath(PathBuf);

impl TryFrom<PathBuf> for ResolvedPath {
    type Error = anyhow::Error;

    fn try_from(path: PathBuf) -> Result<Self> {
        let resolved = path
            .canonicalize()
            .with_context(|| format!("cannot resolve {}", path.display()))?;
        ensure!(
            resolved.is_file(),
            "{} is not a regular file",
            path.display()
        );
        Ok(Self(resolved))
    }
}

impl AsRef<Path> for ResolvedPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[cfg(feature = "cli")]
impl clap::builder::ValueParserFactory for ResolvedPath {
    type Parser = clap::builder::ValueParser;

    fn value_parser() -> Self::Parser {
        use clap::builder::{PathBufValueParser, TypedValueParser};
        PathBufValueParser::new().try_map(Self::try_from).into()
    }
}

pub fn open_regular_file(path: &Path) -> Result<(File, Metadata)> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "{} is not a regular file",
        path.display()
    );
    Ok((file, metadata))
}

pub fn read_bounded(path: &Path, bounds: RangeInclusive<usize>) -> Result<Vec<u8>> {
    let (mut file, metadata) = open_regular_file(path)?;
    let size = usize::try_from(metadata.len())?;
    ensure!(
        bounds.contains(&size),
        "{} has an unsupported file size",
        path.display()
    );
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .context("cannot allocate file buffer")?;
    bytes.resize(size, 0);
    file.read_exact(&mut bytes)
        .with_context(|| format!("cannot read {}", path.display()))?;
    ensure!(
        file.read(&mut [0])? == 0,
        "{} grew while reading",
        path.display()
    );
    Ok(bytes)
}
