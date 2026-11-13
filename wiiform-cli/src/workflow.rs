// SPDX-License-Identifier: GPL-2.0-only
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use tempfile::Builder;

use crate::{
    native::{self, CacheMode, InstalledWad, StockIdentity, WadPolicy},
    priiloader_payload,
    recipe::{self, Menu, SignaturePolicy},
    seed::StockSeed,
};

pub use native::BootPolicy;

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub seed: PathBuf,
    pub recipe: PathBuf,
    pub output: PathBuf,
    pub cache: PathBuf,
    pub offline: bool,
    pub boot_policy: BootPolicy,
    pub menu: Option<Menu>,
}

pub fn build(options: &BuildOptions) -> Result<()> {
    build_with(options, |options, recipe, nand| {
        let seed = StockSeed::read(&options.seed)?;
        let mode = if options.offline {
            CacheMode::Offline
        } else {
            CacheMode::DownloadMissing
        };
        native::create_stock(
            &seed,
            nand,
            &options.cache,
            recipe.menu(),
            options.boot_policy,
            mode,
        )
    })
}

fn build_with(
    options: &BuildOptions,
    create_stock: impl FnOnce(&BuildOptions, &recipe::Recipe, &mut [u8]) -> Result<StockIdentity>,
) -> Result<()> {
    validate_output(&options.output)?;
    let parent = output_parent(&options.output)?;
    let output = parent
        .canonicalize()?
        .join(options.output.file_name().context("output needs a name")?);
    let cache = resolve_cache(&options.cache)?;
    let options = BuildOptions {
        cache,
        ..options.clone()
    };
    ensure!(
        !options.cache.starts_with(&output),
        "cache cannot be inside the output bundle"
    );
    let recipe =
        recipe::load_recipe(&options.recipe, options.menu).context("cannot parse recipe")?;
    if !options.offline {
        fs::create_dir_all(&options.cache).context("cannot create title cache")?;
    }
    ensure!(
        options.cache.is_dir(),
        "cache must be an existing directory"
    );

    let mut nand = allocate_nand()?;
    let identity =
        create_stock(&options, &recipe, &mut nand).context("cannot create stock NAND")?;

    let removals: Vec<_> = recipe
        .remove()
        .iter()
        .map(recipe::TitleId::as_u64)
        .collect();
    for &id in &removals {
        native::remove_title(&mut nand, id)
            .with_context(|| format!("cannot remove title {id:016x}"))?;
    }

    if recipe.setup().is_some() || recipe.display().is_some() {
        native::apply_system_settings(&mut nand, recipe.menu(), recipe.setup(), recipe.display())
            .context("cannot generate system settings")?;
    }

    for title in recipe.titles() {
        let wad = read_bounded(&title.path, 0x40, native::WAD_MAX_BYTES)?;
        let policy = match title.signature_policy {
            SignaturePolicy::Nintendo => WadPolicy::Standard,
            SignaturePolicy::AllowUnsigned => WadPolicy::LegacyChannel,
        };
        let installed = native::install_wad(&mut nand, &wad, &title.sha256, policy)
            .with_context(|| format!("cannot install WAD {}", title.path.display()))?;
        ensure!(
            !removals.contains(&installed.title_id),
            "recipe both removes and installs title {:016x}",
            installed.title_id
        );
        warn_region(&installed, recipe.menu());
    }

    if let Some(priiloader) = recipe.priiloader() {
        let assets = priiloader_payload::prepare(
            &options.cache,
            options.offline,
            priiloader.app.as_deref(),
            priiloader.hacks.is_some(),
        )
        .context("cannot prepare pinned Priiloader assets")?;
        native::apply_priiloader(
            &mut nand,
            &assets.app,
            assets.definitions.as_deref().unwrap_or_default(),
            &priiloader.settings,
            priiloader.hacks.as_ref(),
        )
        .context("cannot install or configure Priiloader")?;
    }

    let keys = native::prepare_backupmii(&mut nand, &identity)
        .context("cannot prepare BackupMii footer")?;
    ensure!(
        keys.len() == 0x400 && nand.ends_with(&keys),
        "BackupMii sidecar does not match NAND"
    );
    publish_bundle(&nand, &keys, &options.output)
}

fn warn_region(wad: &InstalledWad, menu: Menu) {
    let expected = match menu {
        Menu::J43 => 0,
        Menu::U43 => 1,
        Menu::E43 => 2,
    };
    if wad.region != expected && wad.region != 3 {
        let region = match wad.region {
            0 => "Japan",
            1 => "USA",
            2 => "PAL",
            4 => "Korea",
            _ => "unknown region",
        };
        eprintln!(
            "warning: WAD {:016x} declares {region} ({}). Launching on {} may require a region-free System Menu.",
            wad.title_id,
            wad.region,
            menu.as_str(),
        );
    }
}

fn allocate_nand() -> Result<Vec<u8>> {
    let mut nand = Vec::new();
    nand.try_reserve_exact(native::NAND_BYTES)
        .context("cannot allocate NAND buffer")?;
    nand.resize(native::NAND_BYTES, 0);
    Ok(nand)
}

pub fn read_bounded(path: &Path, min: usize, max: usize) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    let metadata = file.metadata()?;
    let size = usize::try_from(metadata.len())?;
    ensure!(
        metadata.is_file() && (min..=max).contains(&size),
        "{} has an unsupported file size or type",
        path.display(),
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

fn resolve_cache(path: &Path) -> Result<PathBuf> {
    let absolute = std::env::current_dir()?.join(path);
    let mut existing = PathBuf::new();
    for part in absolute.components() {
        existing.push(part);
        if let Ok(canonical) = existing.canonicalize() {
            existing = canonical;
        } else if part == std::path::Component::ParentDir {
            existing.pop();
            existing.pop();
        }
    }
    Ok(existing)
}

fn output_parent(path: &Path) -> Result<&Path> {
    ensure!(
        path.file_name().is_some(),
        "output needs a new file or directory name"
    );
    Ok(path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new(".")))
}

pub fn validate_output(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!("output already exists; refusing to replace it"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("cannot check whether output exists"),
    }
    ensure!(
        output_parent(path)?.is_dir(),
        "output parent must be an existing directory"
    );
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub fn publish_edit(nand: &[u8], output: &Path) -> Result<()> {
    validate_output(output)?;
    let staging = Builder::new()
        .prefix(".wii-nand-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(output_parent(output)?)?;
    let candidate = staging.path().join("nand.bin");
    write_private(&candidate, nand).context("cannot write edited NAND")?;
    rename_noreplace(&candidate, output).context("cannot publish edited NAND")
}

fn publish_bundle(nand: &[u8], keys: &[u8], output: &Path) -> Result<()> {
    let staging = Builder::new()
        .prefix(".wiiform-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(output_parent(output)?)?;
    let bundle = staging.path().join("bundle");
    fs::create_dir(&bundle)?;
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o700))?;
    write_private(&bundle.join("nand.bin"), nand).context("cannot write NAND")?;
    write_private(&bundle.join("keys.bin"), keys).context("cannot write keys sidecar")?;
    File::open(&bundle)?.sync_all()?;
    rename_noreplace(&bundle, output).context("cannot publish NAND restore bundle")
}

#[cfg(target_os = "linux")]
fn rename_noreplace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::{os::unix::ffi::OsStrExt, os::unix::io::RawFd};
    const RENAME_NOREPLACE: libc::c_uint = 1;
    let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "source path contains NUL")
    })?;
    let target = CString::new(target.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "target path contains NUL")
    })?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD as RawFd,
            source.as_ptr(),
            libc::AT_FDCWD as RawFd,
            target.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn rename_noreplace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "source path contains NUL")
    })?;
    let target = CString::new(target.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "target path contains NUL")
    })?;
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
