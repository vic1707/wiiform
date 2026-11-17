// SPDX-License-Identifier: GPL-2.0-only
use std::{
    borrow::Cow,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
pub use clap_stdin::FileOrStdin;
use rustix::fs::{CWD, RenameFlags, renameat_with};
use tempfile::Builder;

use crate::{
    native::{self, InstalledWad, Nand, StockIdentity},
    priiloader_payload, recipe,
    recipe::{Menu, Priiloader, Recipe, Region, Rvloader, TitleId},
    seed::StockSeed,
};

pub use crate::native::{BootPolicy, NusCache};

#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// A path, which may be a pipe such as `<(age -d …)`, or `-` for stdin.
    pub seed: FileOrStdin,
    /// The same; WAD paths are relative to the recipe file, or to the working
    /// directory when it comes from stdin.
    pub recipe: FileOrStdin,
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

fn open(input: &FileOrStdin) -> Result<impl io::Read> {
    (input.clone().into_reader()).with_context(|| format!("cannot read {}", input.filename()))
}

pub fn build(options: &BuildOptions) -> Result<()> {
    build_with(options, |recipe| {
        let mut seed = StockSeed::from_reader(open(&options.seed)?)?;
        options.cache.prepare()?;
        create_stock(
            &mut seed,
            recipe.nand.menu,
            options.boot_policy,
            &options.cache,
        )
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
    ensure!(
        !(options.seed.is_stdin() && options.recipe.is_stdin()),
        "only one of the seed and the recipe can come from stdin"
    );
    // The seed is read after the recipe's downloads: catch a mistyped path first.
    if options.seed.is_file() {
        let seed = Path::new(options.seed.filename());
        ensure!(seed.exists(), "cannot find seed {}", seed.display());
    }
    let mut recipe = {
        let directory = match options.recipe.is_stdin() {
            true => Path::new(""),
            false => Path::new(options.recipe.filename())
                .parent()
                .unwrap_or(Path::new("")),
        };
        let directory = match directory.as_os_str().is_empty() {
            true => std::env::current_dir()?,
            false => directory.canonicalize()?,
        };
        Recipe::read(open(&options.recipe)?, &directory)?
    };
    if let Some(menu) = options.menu {
        recipe.nand.menu = menu;
        recipe.validate()?;
    }
    let (mut nand, identity) = create_stock(&recipe).context("cannot create stock NAND")?;

    let removals: Vec<TitleId> = (recipe.nand.remove.iter())
        .flat_map(|channel| channel.title_ids(recipe.nand.menu))
        .collect();
    for &id in &removals {
        nand.remove_title(id)
            .with_context(|| format!("cannot remove title {id}"))?;
    }

    if let Some(setup) = &recipe.nand.setup {
        nand.apply_system_settings(recipe.nand.menu, setup)
            .context("cannot generate system settings")?;
    }

    for (index, title) in recipe.nand.titles.iter().enumerate() {
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
                .nand
                .cios
                .iter()
                .any(|cios| cios.target.title_id() == installed.title_id),
            "recipe lists a title WAD and a cIOS for the same slot ({})",
            installed.title_id
        );
        report_installed(&installed, recipe.nand.menu);
    }

    for cios in &recipe.nand.cios {
        let installed = nand
            .install_cios(&cios.wad, &cios.target)
            .with_context(|| format!("cannot configure cIOS slot {}", cios.target.slot))?;
        warn_if_already_present(&installed);
    }

    if let Some(hbc) = &recipe.nand.hbc {
        let installed = nand
            .install_homebrew_channel(&hbc.0)
            .context("cannot install Homebrew Channel")?;
        report_installed(&installed, recipe.nand.menu);
    }

    if let Some(config) = &recipe.nand.priiloader {
        install_priiloader(
            &mut nand,
            config,
            recipe.nand.rvloader.as_ref(),
            &options.cache,
        )?;
    }

    nand.verify_generated_nand(&identity)
        .context("cannot verify generated NAND")?;
    publish_bundle(&nand, &recipe.sd, &options.output)
}

/// Installs and configures Priiloader, and with `rvloader` does what RVLoader's
/// installer does on top of it.
pub fn install_priiloader(
    nand: &mut Nand,
    config: &Priiloader,
    rvloader: Option<&Rvloader>,
    cache: &NusCache,
) -> Result<()> {
    nand.install_priiloader(priiloader_payload::APP)
        .context("cannot install Priiloader")?;
    let config = match rvloader {
        None => Cow::Borrowed(config),
        Some(rvloader) => {
            // RVLoader's bootloader needs hardware access to bring up USB and video.
            nand.install_priiloader_boot_file(&rvloader.bootloader, true)
                .context("cannot install the RVLoader bootloader")?;
            cache.prepare()?;
            for ios in rvloader.ios() {
                let installed = nand
                    .install_rvloader_ios(cache, &ios, &rvloader.wifi_module)
                    .with_context(|| format!("cannot install RVLoader IOS{}", ios.slot))?;
                warn_if_already_present(&installed);
            }
            nand.set_rvloader_vga(rvloader.vga)
                .context("cannot set RVLoader's VGA option")?;
            Cow::Owned(config.booting_file())
        }
    };
    nand.configure_priiloader(&config)
        .context("cannot configure Priiloader")?;
    nand.set_priiloader_hacks(priiloader_payload::DEFINITIONS, &config.hacks)
        .context("cannot set Priiloader hacks")
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

/// Publishes the SD card root: the NAND and its keys for BootMii, and the recipe's `sd` files.
fn publish_bundle(nand: &Nand, card: &[recipe::sd::File], output: &Path) -> Result<()> {
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
    for (path, bytes) in card.iter().flat_map(|entry| entry.files()) {
        let target = path.under(&bundle);
        let parent = target.parent().context("SD file has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
        write_private(&target, bytes).with_context(|| format!("cannot write {path}"))?;
    }
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

    #[test]
    fn the_bundle_is_the_card_root() -> Result<()> {
        let work = tempfile::tempdir()?;
        let digest: String = (Sha256::digest(b"boot").iter())
            .map(|byte| format!("{byte:02x}"))
            .collect();
        // Ym9vdA== is "boot" in base64.
        let yaml = format!(
            "nand: {{menu: 4.3E}}\nsd: [{{inline: Ym9vdA==, sha256: {digest}, to: apps/x/boot.dol}}]"
        );
        let recipe = Recipe::read(yaml.as_bytes(), work.path())?;
        let output = work.path().join("card");
        publish_bundle(&Nand::zeroed()?, &recipe.sd, &output)?;
        let mut names: Vec<_> = fs::read_dir(&output)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<_, _>>()?;
        names.sort();
        assert_eq!(names, ["apps", "keys.bin", "nand.bin"]);
        assert_eq!(fs::read(output.join("apps/x/boot.dol"))?, b"boot");
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
                "nand:\n  menu: 4.3E\n  titles:\n    - path: fixture.wad\n      sha256: \"{}\"\n      allow_unsigned: true\n",
                expected.trim()
            ),
        )?;
        let image_before = hash(&image)?;
        let wad_before = hash(&wad)?;
        let options = BuildOptions {
            seed: seed.to_str().context("UTF-8 path")?.parse()?,
            recipe: recipe.to_str().context("UTF-8 path")?.parse()?,
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
