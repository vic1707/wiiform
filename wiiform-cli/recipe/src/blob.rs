// SPDX-License-Identifier: GPL-2.0-only
//! A device file's contents, kept on disk until they are copied to the card.
use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use zip::ZipArchive;

use crate::wad::Sha256;

/// Where a device file's bytes are. Nothing is held in memory: a file can be
/// larger than the memory of the machine building the card.
#[derive(Clone, Debug)]
pub enum Blob {
    /// A whole file: a verified download, a local file, or an inline copy.
    File { path: PathBuf, size: u64 },
    /// One member of a zip archive on disk, already verified as a whole.
    Member {
        archive: PathBuf,
        index: usize,
        size: u64,
    },
}

impl Blob {
    /// A whole file on disk.
    pub(crate) fn file(path: PathBuf) -> Result<Self> {
        let size = fs::metadata(&path)
            .with_context(|| format!("cannot read {}", path.display()))?
            .len();
        Ok(Self::File { path, size })
    }

    pub fn size(&self) -> u64 {
        match *self {
            Self::File { size, .. } | Self::Member { size, .. } => size,
        }
    }

    /// Copies the contents to `out`.
    pub fn copy_to(&self, out: &mut impl Write) -> Result<()> {
        match self {
            Self::File { path, .. } => {
                let mut file =
                    File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
                io::copy(&mut file, out)?;
            }
            Self::Member { archive, index, .. } => {
                let mut zip = ZipArchive::new(File::open(archive)?)?;
                io::copy(&mut zip.by_index(*index)?, out)?;
            }
        }
        Ok(())
    }

    /// SHA-256 of the contents, read from disk.
    pub fn sha256(&self) -> Result<Sha256> {
        let mut hashed = Hashed::new(io::sink());
        self.copy_to(&mut hashed)?;
        Ok(hashed.finish().1)
    }

    /// The contents, for tests and other small uses.
    #[cfg(test)]
    pub fn bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.copy_to(&mut bytes)?;
        Ok(bytes)
    }
}

/// Passes bytes through to `inner` while hashing them.
pub(crate) struct Hashed<W> {
    inner: W,
    digest: sha2::Sha256,
}

impl<W: Write> Hashed<W> {
    pub(crate) fn new(inner: W) -> Self {
        Self {
            inner,
            digest: sha2::Digest::new(),
        }
    }

    /// The inner writer and the SHA-256 of everything written.
    pub(crate) fn finish(self) -> (W, Sha256) {
        use sha2::Digest;
        (self.inner, Sha256(self.digest.finalize().into()))
    }
}

impl<W: Write> Write for Hashed<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        use sha2::Digest;
        let written = self.inner.write(buffer)?;
        self.digest.update(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
