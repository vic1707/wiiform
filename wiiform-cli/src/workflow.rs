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
    native::{self, InstalledWad, Nand, StockIdentity},
    priiloader_payload,
    recipe::{Menu, Recipe, Region, ResolvedPath, TitleId, open_regular_file},
    seed::StockSeed,
};

pub use crate::native::{BootPolicy, NusCache};

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
    pub cache: NusCache,
    pub boot_policy: BootPolicy,
    pub menu: Option<Menu>,
}

impl NusCache {
    /// NUS uses memory-only downloads by default. Set `WIIFORM_NUS_CACHE` to reuse
    /// authenticated files across builds, and `WIIFORM_NUS_OFFLINE=1` to require
    /// that cache to contain every requested file.
    pub fn from_environment() -> Result<Self> {
        let offline = match std::env::var("WIIFORM_NUS_OFFLINE") {
            Ok(value) if value == "1" || value.eq_ignore_ascii_case("true") => true,
            Ok(value) if value == "0" || value.eq_ignore_ascii_case("false") => false,
            Ok(_) => bail!("WIIFORM_NUS_OFFLINE must be 1, 0, true, or false"),
            Err(std::env::VarError::NotPresent) => false,
            Err(error) => return Err(error.into()),
        };
        match std::env::var_os("WIIFORM_NUS_CACHE").filter(|path| !path.is_empty()) {
            Some(path) => Ok(Self::Directory {
                path: resolve_cache(Path::new(&path))?,
                offline,
            }),
            None => {
                ensure!(!offline, "WIIFORM_NUS_OFFLINE requires WIIFORM_NUS_CACHE");
                Ok(Self::Memory)
            }
        }
    }

    /// Creates the cache directory, or checks that an offline one exists.
    pub fn prepare(&self) -> Result<()> {
        match self {
            Self::Memory => Ok(()),
            Self::Directory {
                path,
                offline: true,
            } => {
                ensure!(
                    path.is_dir(),
                    "offline NUS cache must be an existing directory"
                );
                Ok(())
            }
            Self::Directory { path, .. } => {
                fs::create_dir_all(path).context("cannot create NUS cache")
            }
        }
    }
}

pub fn build(options: &BuildOptions) -> Result<()> {
    build_with(options, |recipe| {
        let mut seed = match &options.seed {
            SeedInput::Stdin => StockSeed::from_reader(io::stdin().lock()),
            SeedInput::File(path) => StockSeed::from_reader(open_regular_file(path.as_ref())?.0),
        }?;
        options.cache.prepare()?;
        create_stock(&mut seed, recipe.menu, options.boot_policy, &options.cache)
    })
}

/// Builds the image a factory-fresh console of this menu's region would have.
fn create_stock(
    seed: &mut StockSeed,
    menu: Menu,
    boot_policy: BootPolicy,
    cache: &NusCache,
) -> Result<(Nand, StockIdentity)> {
    if seed.write_backupmii_header()? {
        eprintln!("warning: replaced the seed's key footer header with a BackupMii header");
    }
    let mut nand = Nand::zeroed()?;
    let identity = nand.initialize_image(seed, menu, boot_policy)?;
    for (title_id, version) in native::stock_titles(menu)? {
        nand.install_nus_title(cache, title_id, version)
            .with_context(|| format!("cannot install stock title {title_id} v{version}"))?;
    }
    nand.verify_stock(seed, menu)?;
    Ok((nand, identity))
}

fn build_with(
    options: &BuildOptions,
    create_stock: impl FnOnce(&Recipe) -> Result<(Nand, StockIdentity)>,
) -> Result<()> {
    validate_output(&options.output)?;
    if let NusCache::Directory { path, .. } = &options.cache {
        let output = output_parent(&options.output)?
            .canonicalize()?
            .join(options.output.file_name().context("output needs a name")?);
        ensure!(
            !path.starts_with(&output),
            "cache cannot be inside the output bundle"
        );
    }

    // Reading the recipe loads and checks every WAD it names, before the long stock build.
    let mut recipe = Recipe::try_from(&options.recipe)?;
    if let Some(menu) = options.menu {
        recipe.menu = menu;
        recipe.validate()?;
    }
    let (mut nand, identity) = create_stock(&recipe).context("cannot create stock NAND")?;

    let removals: Vec<TitleId> = (recipe.remove.iter())
        .flat_map(|channel| channel.title_ids(recipe.menu))
        .collect();
    for &id in &removals {
        nand.remove_title(id)
            .with_context(|| format!("cannot remove title {id}"))?;
    }

    if let Some(setup) = &recipe.setup {
        nand.apply_system_settings(recipe.menu, setup)
            .context("cannot generate system settings")?;
    }

    for (index, title) in recipe.titles.iter().enumerate() {
        let installed = if title.allow_unsigned {
            nand.install_unsigned_channel(&title.wad)
        } else {
            nand.install_wad(&title.wad)
        }
        .with_context(|| format!("cannot install titles[{index}] WAD"))?;
        ensure!(
            !removals.contains(&installed.title_id),
            "recipe both removes and installs title {}",
            installed.title_id
        );
        ensure!(
            !recipe
                .cios
                .iter()
                .any(|cios| cios.target.title_id() == installed.title_id),
            "recipe lists a title WAD and a cIOS for the same slot ({})",
            installed.title_id
        );
        report_installed(&installed, recipe.menu);
    }

    for cios in &recipe.cios {
        let installed = nand
            .install_cios(&cios.wad, &cios.target)
            .with_context(|| format!("cannot configure cIOS slot {}", cios.target.slot))?;
        warn_if_already_present(&installed);
    }

    if let Some(hbc) = &recipe.hbc {
        let installed = nand
            .install_homebrew_channel(&hbc.0)
            .context("cannot install Homebrew Channel")?;
        report_installed(&installed, recipe.menu);
    }

    if let Some(config) = &recipe.priiloader {
        nand.install_priiloader(priiloader_payload::APP)
            .context("cannot install Priiloader")?;
        let config = match &recipe.rvloader {
            Some(bootloader) => {
                // RVLoader's bootloader needs hardware access to bring up USB and video.
                nand.install_priiloader_boot_file(bootloader, true)
                    .context("cannot install the RVLoader bootloader")?;
                &config.booting_file()
            }
            None => config,
        };
        nand.configure_priiloader(config)
            .context("cannot configure Priiloader")?;
        nand.set_priiloader_hacks(priiloader_payload::DEFINITIONS, &config.hacks)
            .context("cannot set Priiloader hacks")?;
    }

    nand.verify_generated_nand(&identity)
        .context("cannot verify generated NAND")?;
    publish_bundle(&nand, &options.output)
}

pub fn warn_if_already_present(wad: &InstalledWad) {
    if wad.already_present {
        eprintln!(
            "warning: title {} v{} is already installed and verified",
            wad.title_id, wad.version
        );
    }
}

fn report_installed(wad: &InstalledWad, menu: Menu) {
    warn_if_already_present(wad);
    // Only channels are region-locked by the System Menu.
    if !matches!(wad.title_id.kind(), 0x0001_0001 | 0x0001_0002) {
        return;
    }
    if wad.region != Some(menu.region()) && wad.region != Some(Region::Free) {
        let region = wad.region.map_or("an unknown region".to_owned(), |region| {
            format!("{region:?}")
        });
        eprintln!(
            "warning: WAD {} declares {region}. Launching on {menu} may require a region-free System Menu.",
            wad.title_id,
        );
    }
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

pub fn publish_edit(nand: &Nand, output: &Path) -> Result<()> {
    let staging = Builder::new()
        .prefix(".wii-nand-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(output_parent(output)?)
        .context("cannot create a staging directory next to the output")?;
    let candidate = staging.path().join("nand.bin");
    write_private(&candidate, nand.as_bytes()).context("cannot write edited NAND")?;
    publish(&candidate, output).context("cannot publish edited NAND")
}

fn publish_bundle(nand: &Nand, output: &Path) -> Result<()> {
    let staging = Builder::new()
        .prefix(".wiiform-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(output_parent(output)?)
        .context("cannot create a staging directory next to the output")?;
    let bundle = staging.path().join("bundle");
    fs::create_dir(&bundle).context("cannot create the staging bundle")?;
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o700))?;
    write_private(&bundle.join("nand.bin"), nand.as_bytes()).context("cannot write NAND")?;
    write_private(&bundle.join("keys.bin"), nand.keys()).context("cannot write keys sidecar")?;
    File::open(&bundle)?.sync_all()?;
    publish(&bundle, output).context("cannot publish NAND restore bundle")
}

/// Moves a staged file or directory to `target` without replacing anything, then makes the move durable.
fn publish(source: &Path, target: &Path) -> Result<()> {
    renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE)?;
    File::open(output_parent(target)?)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let cache = |offline| NusCache::Directory {
            path: path.clone(),
            offline,
        };
        assert!(cache(true).prepare().is_err());
        cache(false).prepare()?;
        assert!(path.is_dir());
        cache(true).prepare()
    }

    #[test]
    fn no_replace_preserves_an_existing_directory() -> Result<()> {
        let work = tempfile::tempdir()?;
        let from = work.path().join("staged");
        let to = work.path().join("existing");
        fs::create_dir(&from)?;
        fs::create_dir(&to)?;
        assert!(publish(&from, &to).is_err());
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
                "menu: 4.3E\ntitles:\n  - path: fixture.wad\n    sha256: \"{}\"\n    allow_unsigned: true\n",
                expected.trim()
            ),
        )?;
        let image_before = hash(&image)?;
        let wad_before = hash(&wad)?;
        let options = BuildOptions {
            seed: SeedInput::File(seed.clone().try_into()?),
            recipe: recipe.try_into()?,
            output: output.clone(),
            cache: NusCache::Memory,
            boot_policy: BootPolicy::Stock,
            menu: None,
        };
        let failure = build_with(&options, |_| {
            Ok((Nand::read(&image)?, StockIdentity::unverifiable()))
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
