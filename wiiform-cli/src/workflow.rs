// SPDX-License-Identifier: GPL-2.0-only
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use tempfile::Builder;

use crate::{
    native::{self, InstalledWad, StockIdentity},
    priiloader_payload,
    recipe::{Menu, Recipe, ResolvedPath, open_regular_file},
    seed::StockSeed,
};

pub use native::BootPolicy;

#[derive(Clone, Debug)]
pub enum SeedInput {
    Stdin,
    File(ResolvedPath),
}

impl TryFrom<PathBuf> for SeedInput {
    type Error = anyhow::Error;

    fn try_from(path: PathBuf) -> Result<Self> {
        if path.as_os_str() == "-" {
            Ok(Self::Stdin)
        } else {
            Ok(Self::File(path.try_into()?))
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub seed: SeedInput,
    pub recipe: ResolvedPath,
    pub output: PathBuf,
    pub cache: PathBuf,
    pub offline: bool,
    pub boot_policy: BootPolicy,
    pub menu: Option<Menu>,
}

/// NUS uses memory-only downloads by default. Set `WIIFORM_NUS_CACHE` to reuse
/// authenticated files across builds, and `WIIFORM_NUS_OFFLINE=1` to require
/// that cache to contain every requested file.
pub fn nus_environment() -> Result<(PathBuf, bool)> {
    let cache = std::env::var_os("WIIFORM_NUS_CACHE")
        .map(PathBuf::from)
        .unwrap_or_default();
    let offline = match std::env::var("WIIFORM_NUS_OFFLINE") {
        Ok(value) if value == "1" || value.eq_ignore_ascii_case("true") => true,
        Ok(value) if value == "0" || value.eq_ignore_ascii_case("false") => false,
        Ok(_) => bail!("WIIFORM_NUS_OFFLINE must be 1, 0, true, or false"),
        Err(std::env::VarError::NotPresent) => false,
        Err(error) => return Err(error.into()),
    };
    ensure!(
        !offline || !cache.as_os_str().is_empty(),
        "WIIFORM_NUS_OFFLINE requires WIIFORM_NUS_CACHE"
    );
    Ok((cache, offline))
}

pub fn build(options: &BuildOptions) -> Result<()> {
    build_with(options, |options, recipe| {
        let seed = match &options.seed {
            SeedInput::Stdin => StockSeed::from_reader(io::stdin().lock()),
            SeedInput::File(path) => StockSeed::from_reader(open_regular_file(path.as_ref())?.0),
        }?;
        prepare_nus_cache(&options.cache, options.offline)?;
        let mut nand = allocate_nand()?;
        let identity = native::create_stock(
            &seed,
            &mut nand,
            &options.cache,
            recipe.menu,
            options.boot_policy,
            options.offline,
        )?;
        Ok((nand, identity))
    })
}

fn build_with(
    options: &BuildOptions,
    create_stock: impl FnOnce(&BuildOptions, &Recipe) -> Result<(Vec<u8>, StockIdentity)>,
) -> Result<()> {
    validate_output(&options.output)?;
    let parent = output_parent(&options.output)?;
    let output = parent
        .canonicalize()?
        .join(options.output.file_name().context("output needs a name")?);
    let cache = if options.cache.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        resolve_cache(&options.cache)?
    };
    let options = BuildOptions {
        cache,
        ..options.clone()
    };
    if !options.cache.as_os_str().is_empty() {
        ensure!(
            !options.cache.starts_with(&output),
            "cache cannot be inside the output bundle"
        );
    }
    let (recipe, removals) = Recipe::load(&options.recipe, options.menu)?;
    let priiloader = recipe
        .priiloader
        .as_ref()
        .map(|config| priiloader_payload::load().map(|assets| (config, assets)))
        .transpose()?;
    let (mut nand, identity) =
        create_stock(&options, &recipe).context("cannot create stock NAND")?;

    let removals: Vec<_> = removals.iter().map(|id| id.as_u64()).collect();
    for &id in &removals {
        native::remove_title(&mut nand, id)
            .with_context(|| format!("cannot remove title {id:016x}"))?;
    }

    if let Some(setup) = &recipe.setup {
        native::apply_system_settings(&mut nand, recipe.menu, setup)
            .context("cannot generate system settings")?;
    }

    for (index, title) in recipe.titles.iter().enumerate() {
        let installed = native::install_wad(&mut nand, title)
            .with_context(|| format!("cannot install titles[{index}] WAD"))?;
        ensure!(
            !removals.contains(&installed.title_id),
            "recipe both removes and installs title {:016x}",
            installed.title_id
        );
        ensure!(
            !recipe
                .cios
                .iter()
                .any(|cios| cios.options.target.title_id() == installed.title_id),
            "recipe lists a title WAD and a cIOS for the same slot ({:016x})",
            installed.title_id
        );
        installed.warn_if_already_present();
        warn_region(&installed, recipe.menu);
    }

    for cios in &recipe.cios {
        native::install_cios(&mut nand, cios)
            .with_context(|| format!("cannot configure cIOS slot {}", cios.options.target.slot))?
            .warn_if_already_present();
    }

    if let Some((config, (app, definitions))) = priiloader {
        native::apply_priiloader(
            &mut nand,
            &app,
            &definitions,
            &config.settings,
            &config.hacks,
        )
        .context("cannot install or configure Priiloader")?;
    }

    let keys = native::prepare_backupmii(&mut nand, &identity)
        .context("cannot prepare BackupMii footer")?;
    publish_bundle(&nand, &keys, &options.output)
}

fn warn_region(wad: &InstalledWad, menu: Menu) {
    if !matches!(wad.title_id >> 32, 0x0001_0001 | 0x0001_0002) {
        return;
    }
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

pub fn prepare_nus_cache(cache: &Path, offline: bool) -> Result<()> {
    if cache.as_os_str().is_empty() {
        return Ok(());
    }
    if offline {
        ensure!(
            cache.is_dir(),
            "offline NUS cache must be an existing directory"
        );
    } else {
        fs::create_dir_all(cache).context("cannot create NUS cache")?;
    }
    Ok(())
}

fn resolve_cache(path: &Path) -> Result<PathBuf> {
    let absolute = std::env::current_dir()?.join(path);
    let mut existing = PathBuf::new();
    for component in absolute.components() {
        match existing.join(component.as_os_str()).canonicalize() {
            Ok(canonical) => existing = canonical,
            Err(error) if error.kind() == io::ErrorKind::NotFound => match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    existing.pop();
                }
                _ => existing.push(component.as_os_str()),
            },
            Err(error) => return Err(error).context("cannot resolve NUS cache path"),
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

fn rename_noreplace(source: &Path, target: &Path) -> std::io::Result<()> {
    renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::read_bounded;
    use sha2::{Digest, Sha256};
    use std::{
        io::{BufRead, BufReader},
        process::Command,
    };

    #[test]
    fn online_creates_the_cache_and_offline_requires_it() -> Result<()> {
        let work = tempfile::tempdir()?;
        let expected = work.path().canonicalize()?.join("cache/contents");
        let path = resolve_cache(&work.path().join("missing/../cache/contents"))?;
        assert_eq!(path, expected);
        assert!(prepare_nus_cache(&path, true).is_err());
        prepare_nus_cache(&path, false)?;
        assert!(path.is_dir());
        prepare_nus_cache(&path, true)
    }

    #[test]
    fn no_replace_preserves_an_existing_directory() -> Result<()> {
        let work = tempfile::tempdir()?;
        let from = work.path().join("staged");
        let to = work.path().join("existing");
        fs::create_dir(&from)?;
        fs::create_dir(&to)?;
        assert!(rename_noreplace(&from, &to).is_err());
        assert!(from.is_dir() && to.is_dir());
        Ok(())
    }

    fn hash(path: &Path) -> Result<Vec<u8>> {
        let mut digest = Sha256::new();
        let mut reader = BufReader::new(File::open(path)?);
        loop {
            let data = reader.fill_buf()?;
            if data.is_empty() {
                break;
            }
            digest.update(data);
            let length = data.len();
            reader.consume(length);
        }
        Ok(digest.finalize().to_vec())
    }

    #[test]
    fn near_full_recipe_leaves_inputs_and_publishes_nothing() -> Result<()> {
        let Some(builder) = std::env::var_os("WIIFORM_TEST_FIXTURE_BUILDER") else {
            eprintln!("run mise run test:rust to exercise the native NAND fixture");
            return Ok(());
        };
        let work = tempfile::tempdir()?;
        let fixture = work.path().join("fixture");
        ensure!(
            Command::new(builder)
                .arg("--make-fixtures")
                .arg(&fixture)
                .status()?
                .success(),
            "could not make synthetic NAND fixture"
        );
        let image = fixture.join("nand.bin");
        let wad = fixture.join("fixture.wad");
        let seed = fixture.join("seed.yaml");
        let recipe = fixture.join("recipe.yaml");
        let output = work.path().join("output");
        fs::write(&seed, "synthetic stock input")?;
        let expected = fs::read_to_string(fixture.join("fixture.wad.sha256"))?;
        fs::write(
            &recipe,
            format!(
                "menu: 4.3E\ntitles:\n  - source: {{ path: fixture.wad }}\n    sha256: \"{}\"\n    signature_policy: allow-unsigned\n",
                expected.trim()
            ),
        )?;
        let image_before = hash(&image)?;
        let wad_before = hash(&wad)?;
        let options = BuildOptions {
            seed: SeedInput::File(seed.clone().try_into()?),
            recipe: recipe.try_into()?,
            output: output.clone(),
            cache: fixture,
            offline: true,
            boot_policy: BootPolicy::Stock,
            menu: None,
        };
        let failure = build_with(&options, |_, _| {
            let nand = read_bounded(&image, native::NAND_BYTES..=native::NAND_BYTES)?;
            Ok((
                nand,
                StockIdentity {
                    boot_area: [0; 32],
                    original_footer: [0; 32],
                },
            ))
        })
        .expect_err("the WAD cannot fit the synthetic NAND");
        assert!(
            format!("{failure:#}").contains("Insufficient NAND space"),
            "{failure:#}"
        );
        assert!(!output.exists());
        assert_eq!(fs::read_dir(work.path())?.count(), 1);
        assert_eq!(fs::read(&seed)?, b"synthetic stock input");
        assert_eq!(hash(&image)?, image_before);
        assert_eq!(hash(&wad)?, wad_before);
        Ok(())
    }
}
