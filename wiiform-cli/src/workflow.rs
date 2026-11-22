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
    priiloader_payload,
    recipe::{
        Blob, Collisions, Device, Menu, Priiloader, Recipe, Region, Rvloader, Sd, TitleId, Usb,
        nand::titles::cios,
    },
    seed::{self, StockSeed},
};

pub use crate::native::{BootPolicy, NusCache};

/// The BootMii as IOS to install for `bootmii`, or None when the console only
/// runs BootMii as boot2 (still reachable). A console with neither is an error.
fn bootmii_ios(bootmii: &seed::Bootmii) -> Result<Option<&seed::BootmiiIos>> {
    match (&bootmii.ios, bootmii.boot2) {
        (Some(ios), _) => Ok(Some(ios)),
        (None, seed::Boot2::Detected) => Ok(None),
        (None, seed::Boot2::Stock | seed::Boot2::Unknown) => bail!(
            "the seed's console has no BootMii, neither as IOS nor as boot2: run the HackMii Installer, back the console up again and regenerate the seed"
        ),
    }
}

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
    /// What happens when two device entries write the same path.
    pub collisions: Collisions,
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
        if recipe.nand.bootmii.is_some() {
            bootmii_ios(seed.bootmii())?;
        }
        options.cache.prepare()?;
        let (nand, identity) = create_stock(
            &mut seed,
            recipe.nand.menu,
            options.boot_policy,
            &options.cache,
        )
        .context("cannot create stock NAND")?;
        Ok((nand, identity, seed.bootmii().clone()))
    })
}

/// Builds the image a factory-fresh console of this menu's region would have.
fn create_stock(
    seed: &mut StockSeed,
    menu: Menu,
    boot_policy: BootPolicy,
    cache: &NusCache,
) -> Result<(Nand, StockIdentity)> {
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
    create_stock: impl FnOnce(&Recipe) -> Result<(Nand, StockIdentity, seed::Bootmii)>,
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

    // Downloaded device files wait in the NUS cache when there is one, so the next
    // build reuses them; otherwise in a directory removed when this build ends.
    let (store, _scratch) = match &options.cache {
        NusCache::Directory { path, .. } => (path.join("files"), None),
        NusCache::Memory => {
            let directory = Builder::new()
                .prefix(".wiiform-downloads-")
                .tempdir_in(output_parent(&options.output)?)
                .context("cannot create a download directory next to the output")?;
            (directory.path().to_owned(), Some(directory))
        }
    };

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
        Recipe::read(
            open(&options.recipe)?,
            &directory,
            &store,
            options.collisions,
        )?
    };
    if let Some(menu) = options.menu {
        recipe.nand.menu = menu;
        recipe.validate()?;
    }
    for defaulted in &recipe.defaulted {
        eprintln!("info: {} defaults to {}", defaulted.key, defaulted.value);
    }
    for notice in recipe.collision_notices() {
        eprintln!("warning: {notice}");
    }
    let (mut nand, identity, bootmii) = create_stock(&recipe)?;

    let removals: Vec<TitleId> = (recipe.nand.remove.iter())
        .flat_map(|channel| channel.title_ids(recipe.nand.menu))
        .collect();
    for &id in &removals {
        nand.remove_title(id)
            .with_context(|| format!("cannot remove title {id}"))?;
    }

    if let Some(setup) = &recipe.nand.setup {
        nand.apply_system_settings(setup)
            .context("cannot generate system settings")?;
    }

    let titles = &recipe.nand.titles;
    for (index, channel) in titles.channels.iter().enumerate() {
        let installed = if channel.allow_unsigned {
            nand.install_unsigned_channel(&channel.wad)
        } else {
            nand.install_channel(&channel.wad)
        }
        .with_context(|| format!("cannot install titles.channels[{index}]"))?;
        ensure!(
            !removals.contains(&installed.title_id),
            "recipe both removes and installs title {}",
            installed.title_id
        );
        report_installed(&installed, recipe.nand.menu);
    }
    for ios in &titles.ios {
        let installed = nand
            .install_nus_title(&options.cache, ios.title_id(), ios.version)
            .with_context(|| format!("cannot install IOS{} v{}", ios.slot, ios.version))?;
        warn_if_already_present(&installed);
    }
    for cios in &titles.cios {
        let installed = match &cios.kind {
            cios::Kind::Prebuilt(wad) => nand.install_cios(wad, &cios.target),
            cios::Kind::Built(build) => {
                nand.install_built_cios(&options.cache, build, &cios.target)
            }
        }
        .with_context(|| format!("cannot install cIOS slot {}", cios.target.slot))?;
        warn_if_already_present(&installed);
    }

    if recipe.nand.bootmii.is_some() {
        match bootmii_ios(&bootmii)? {
            Some(ios) => {
                if bootmii.boot2 == seed::Boot2::Detected {
                    eprintln!("info: this console also runs BootMii as boot2");
                }
                let installed = nand
                    .install_bootmii_ios(ios)
                    .context("cannot install BootMii as IOS")?;
                warn_if_already_present(&installed);
            }
            None => eprintln!(
                "warning: the seed's console has no BootMii as IOS; BootMii as boot2 still starts at power-on"
            ),
        }
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
    publish_bundle(
        &nand,
        recipe.sd.as_ref(),
        recipe.usb.as_ref(),
        &options.output,
    )
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
    nand.set_priiloader_hacks(priiloader_payload::DEFINITIONS, &config)
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

/// Publishes sd/, the SD card's root (the NAND and its keys for BootMii, and
/// the recipe's `sd` files), and usb/ when the recipe has `usb`.
fn publish_bundle(
    nand: &Nand,
    sd: Option<&Device<Sd>>,
    usb: Option<&Device<Usb>>,
    output: &Path,
) -> Result<()> {
    let staging = Builder::new()
        .prefix(".wiiform-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(output_parent(output)?)
        .context("cannot create a staging directory next to the output")?;
    let bundle = staging.path().join("bundle");
    fs::create_dir(&bundle).context("cannot create the staging bundle")?;
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o700))?;
    let card = bundle.join("sd");
    fs::create_dir(&card).context("cannot create sd/")?;
    write_private(&card.join("nand.bin"), nand.as_bytes()).context("cannot write NAND")?;
    write_private(&card.join("keys.bin"), nand.keys()).context("cannot write keys sidecar")?;
    if let Some(sd) = sd {
        write_tree(&card, sd).context("cannot write the SD files")?;
    }
    if let Some(usb) = usb {
        let drive = bundle.join("usb");
        fs::create_dir(&drive).context("cannot create usb/")?;
        write_tree(&drive, usb).context("cannot write the USB files")?;
    }
    File::open(&bundle)?.sync_all()?;
    publish(&bundle, output).context("cannot publish NAND restore bundle")
}

fn write_tree<K>(root: &Path, device: &Device<K>) -> Result<()> {
    for (path, blob) in device.files() {
        let target = path.under(root);
        let parent = target.parent().context("file has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
        write_blob(&target, blob).with_context(|| format!("cannot write {path}"))?;
    }
    Ok(())
}

/// Copies a device file to a new file only its owner may read, streaming it
/// from where the recipe staged it.
fn write_blob(path: &Path, blob: &Blob) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    blob.copy_to(&mut file)?;
    file.sync_all()?;
    Ok(())
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
    fn bootmii_needs_the_ios_or_at_least_boot2() {
        let ios = seed::BootmiiIos {
            tmd: vec![1],
            ticket: vec![2],
            content: vec![3],
        };
        let state = |ios: Option<seed::BootmiiIos>, boot2| seed::Bootmii {
            boot2,
            boot2_version: None,
            ios,
        };
        assert!(matches!(
            bootmii_ios(&state(Some(ios.clone()), seed::Boot2::Stock)),
            Ok(Some(_))
        ));
        assert!(
            matches!(bootmii_ios(&state(None, seed::Boot2::Detected)), Ok(None)),
            "boot2 still reaches BootMii"
        );
        for boot2 in [seed::Boot2::Stock, seed::Boot2::Unknown] {
            let error = format!("{:#}", bootmii_ios(&state(None, boot2)).unwrap_err());
            assert!(error.contains("neither as IOS nor as boot2"));
        }
    }

    #[test]
    fn the_output_holds_the_card_and_the_drive() -> Result<()> {
        let work = tempfile::tempdir()?;
        let digest: String = (Sha256::digest(b"boot").iter())
            .map(|byte| format!("{byte:02x}"))
            .collect();
        // Ym9vdA== is "boot" in base64.
        let file = format!("{{inline: Ym9vdA==, sha256: {digest}, to: apps/x/boot.dol}}");
        let names = |path: &Path| -> Result<Vec<String>> {
            let mut names: Vec<String> = (fs::read_dir(path)?)
                .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
                .collect::<Result<_, _>>()?;
            names.sort();
            Ok(names)
        };

        let yaml =
            format!("nand: {{menu: 4.3E}}\nsd: {{bundles: [{file}]}}\nusb: {{bundles: [{file}]}}");
        let recipe = Recipe::read(
            yaml.as_bytes(),
            work.path(),
            &work.path().join("downloads"),
            Collisions::default(),
        )?;
        let output = work.path().join("both");
        publish_bundle(
            &Nand::zeroed()?,
            recipe.sd.as_ref(),
            recipe.usb.as_ref(),
            &output,
        )?;
        assert_eq!(names(&output)?, ["sd", "usb"]);
        assert_eq!(names(&output.join("sd"))?, ["apps", "keys.bin", "nand.bin"]);
        assert_eq!(fs::read(output.join("usb/apps/x/boot.dol"))?, b"boot");

        let output = work.path().join("nand-only");
        publish_bundle(&Nand::zeroed()?, None, None, &output)?;
        assert_eq!(names(&output)?, ["sd"]);
        assert_eq!(names(&output.join("sd"))?, ["keys.bin", "nand.bin"]);
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
                "nand:\n  menu: 4.3E\n  titles:\n    channels:\n      - path: fixture.wad\n        sha256: \"{}\"\n        allow_unsigned: true\n",
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
            collisions: Collisions::default(),
        };
        let failure = build_with(&options, |_| {
            let no_bootmii = seed::Bootmii {
                boot2: seed::Boot2::Unknown,
                boot2_version: None,
                ios: None,
            };
            Ok((
                Nand::read(&image)?,
                StockIdentity::unverifiable(),
                no_bootmii,
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
