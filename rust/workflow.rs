// SPDX-License-Identifier: GPL-2.0-only
//! End-to-end build orchestration for one verified NAND restore bundle.

use crate::{native, prepared_verify, priiloader_payload, recipe, stock_verify, worker::Worker};
use anyhow::{Context, Result, bail, ensure};
use recipe::{
    Aspect, Autoboot, Display, HackSettings, Language, Menu, MountPoint, PriiloaderSettings,
    Recipe, ReturnTo, ShutdownTo, SignaturePolicy,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256 as Sha256Hasher};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::Builder;

const IMAGE_BYTES: u64 = 0x21000400;
const KEYS_BYTES: u64 = 0x400;
const PRIILOADER_APP_SHA256: &str = priiloader_payload::APP_SHA256;
const TITLE_REGION_NAMES: &[(u64, &str)] = &[(0, "Japan"), (1, "USA"), (2, "PAL"), (4, "Korea")];

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub seed: PathBuf,
    pub recipe: PathBuf,
    pub output: PathBuf,
    pub cache: PathBuf,
    pub reference: Option<PathBuf>,
    pub offline: bool,
    pub boot_policy: BootPolicy,
    pub menu: Option<Menu>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootPolicy {
    Stock,
    PreserveSource,
}

impl BootPolicy {
    fn native(self) -> native::BootPolicy {
        match self {
            Self::Stock => native::BootPolicy::Stock,
            Self::PreserveSource => native::BootPolicy::PreserveSource,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Stock => "stock",
            Self::PreserveSource => "preserve-source",
        }
    }
}

pub struct BuildOutcome {
    pub menu: &'static str,
    pub nand: PathBuf,
    pub keys: PathBuf,
    pub nand_sha256: String,
    pub keys_sha256: String,
    pub evidence_dir: PathBuf,
}

struct BuildContext<'a> {
    options: &'a BuildOptions,
    recipe: Recipe,
    worker: Worker,
    work: PathBuf,
    input_hashes: BTreeMap<String, String>,
    tool_hashes: BTreeMap<String, String>,
    package_reports: Vec<Value>,
    warnings: Vec<String>,
    stages: Vec<Value>,
    operations: Vec<Value>,
    current: Option<PathBuf>,
    stock_image: Option<PathBuf>,
    before: Option<Value>,
    stock_manifest: Option<Value>,
    display_report: Option<Value>,
    priiloader_report: Option<Value>,
}

/// Build, verify, and atomically publish `nand.bin` plus the matching
/// `keys.bin`. The private evidence directory is retained on both success and
/// failure so the caller can inspect the decisions and authenticated reports.
pub fn build(options: &BuildOptions) -> Result<BuildOutcome> {
    let options = absolute_options(options)?;
    validate_inputs(&options)?;
    validate_output(&options.output)?;
    ensure!(
        !options.cache.starts_with(&options.output),
        "cache cannot be inside the new output bundle"
    );

    let recipe_hash = sha256_file(&options.recipe)?;
    let recipe = recipe::load_recipe(&options.recipe, options.menu)
        .context("cannot parse the Wiiform recipe")?;
    ensure!(
        sha256_file(&options.recipe)? == recipe_hash,
        "recipe changed while parsing"
    );
    if (recipe.display.is_some() || recipe.setup.is_some()) && options.reference.is_none() {
        bail!("a same-console --reference is required for recipe display or setup settings");
    }
    let worker = Worker::bundled().context("cannot locate bundled read-only worker")?;
    prepare_cache(&options.cache, options.offline)?;
    let parent = options
        .output
        .parent()
        .context("output path has no parent")?;
    let work_dir = Builder::new()
        .prefix(&format!(".{}-wiiform-", output_name(&options.output)))
        .rand_bytes(8)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)
        .context("cannot create private build evidence directory")?;
    match build_in(&options, recipe, recipe_hash, worker, work_dir.path()) {
        Ok((outcome, _manifest)) => {
            let evidence_dir = work_dir.keep();
            Ok(BuildOutcome {
                evidence_dir,
                ..outcome
            })
        }
        Err(error) => {
            let evidence_dir = work_dir.keep();
            Err(error.context(format!(
                "private evidence retained at {}",
                evidence_dir.display()
            )))
        }
    }
}

fn build_in(
    options: &BuildOptions,
    recipe: Recipe,
    recipe_hash: String,
    worker: Worker,
    work: &Path,
) -> Result<(BuildOutcome, Value)> {
    let mut input_hashes = BTreeMap::new();
    input_hashes.insert("recipe".to_owned(), recipe_hash.clone());
    input_hashes.insert("seed".to_owned(), sha256_file(&options.seed)?);
    if let Some(reference) = &options.reference {
        input_hashes.insert("reference".to_owned(), sha256_file(reference)?);
    }
    for (index, title) in recipe.titles.iter().enumerate() {
        let key = format!("wad-{index}");
        let actual = sha256_file(&title.path)?;
        ensure!(
            actual == title.sha256.as_ref().as_str(),
            "WAD {index} SHA-256 differs from the recipe"
        );
        input_hashes.insert(key, actual);
    }
    if let Some(priiloader) = &recipe.priiloader
        && let Some(app) = &priiloader.app
    {
        input_hashes.insert("priiloader-app".into(), sha256_file(app)?);
    }
    let mut tool_hashes = BTreeMap::new();
    tool_hashes.insert("wiiform".into(), sha256_file(&std::env::current_exe()?)?);
    tool_hashes.insert("read_worker".into(), sha256_file(worker.executable())?);

    let mut context = BuildContext {
        options,
        recipe,
        worker,
        work: work.to_path_buf(),
        input_hashes,
        tool_hashes,
        package_reports: Vec::new(),
        warnings: Vec::new(),
        stages: Vec::new(),
        operations: Vec::new(),
        current: None,
        stock_image: None,
        before: None,
        stock_manifest: None,
        display_report: None,
        priiloader_report: None,
    };

    context.inspect_packages()?;
    let stock = context.build_stock()?;
    context.report_package_regions();
    let mut inventory_refs = Map::new();
    inventory_refs.insert(
        "stock".into(),
        context.stages.last().unwrap()["inventory"].clone(),
    );

    if !context.recipe.remove.is_empty() {
        let image = context.copy_current("01-channels-removed.bin")?;
        let before = context.before()?.clone();
        let removals: Vec<String> = context
            .recipe
            .remove
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        for title in &context.recipe.remove {
            let id = u64::from_str_radix(title.as_str(), 16).with_context(|| {
                format!("invalid normalized removal title ID {}", title.as_str())
            })?;
            native::remove_title(&image, id)
                .with_context(|| format!("cannot remove title {}", title.as_str()))?;
            context
                .operations
                .push(json!({"operation":"remove-title","title_id":title.as_str()}));
        }
        let (after, inventory_path) = context.inventory(&image, "01-channels-removed")?;
        prepared_verify::verify_inventory_delta(
            &before,
            &after,
            &prepared_verify::PreparedExpectations {
                removals: &removals,
                ..Default::default()
            },
        )?;
        context.record(image, "01-channels-removed.bin", after, inventory_path)?;
    }

    if context.recipe.display.is_some() || context.recipe.setup.is_some() {
        let name = if context.recipe.setup.is_some() {
            "02-system-settings.bin"
        } else {
            "02-display.bin"
        };
        let image = context.copy_current(name)?;
        let before = context.before()?.clone();
        let source_path = context.options.reference.as_deref().unwrap();
        let source = context
            .worker
            .display_source(source_path)
            .context("cannot authenticate display source against this console")?;
        let mut settings = Vec::new();
        let display_value = context.recipe.display.as_ref().map(display_json);
        let setup_value = context.recipe.setup.as_ref().map(setup_json);
        if let Some(setup) = &context.recipe.setup {
            settings.push(setting("require-setup", "true"));
            if let Some(nickname) = &setup.nickname {
                settings.push(setting("nickname", nickname));
            }
            if let Some(language) = setup.language {
                settings.push(setting("language", language_code(language)));
            }
        }
        if let Some(display) = &context.recipe.display {
            append_display_settings(&mut settings, display);
        }
        let report = if context.recipe.setup.is_some() {
            native::set_sysconf(&image, &source, &settings)
        } else {
            native::set_display(&image, &source, &settings)
        }
        .context("cannot write requested SYSCONF settings")?;
        let (after, inventory_path) = context.inventory(
            &image,
            Path::new(name).file_stem().unwrap().to_str().unwrap(),
        )?;
        prepared_verify::verify_inventory_delta(
            &before,
            &after,
            &prepared_verify::PreparedExpectations {
                display: display_value.as_ref(),
                display_report: Some(&report),
                setup: setup_value.as_ref(),
                ..Default::default()
            },
        )?;
        write_json(&context.work.join("system-settings.json"), &report)?;
        context
            .operations
            .push(json!({"operation":"set-sysconf","report":"system-settings.json"}));
        context.display_report = Some(report);
        context.record(image, name, after, inventory_path)?;
    }

    if !context.package_reports.is_empty() {
        let image = context.copy_current("03-wads.bin")?;
        let before = context.before()?.clone();
        for (title, report) in context.recipe.titles.iter().zip(&context.package_reports) {
            let digest = title.sha256.as_ref().as_str();
            let allow_unsigned = title.signature_policy == SignaturePolicy::AllowUnsigned;
            native::install_wad(&image, &title.path, digest, allow_unsigned)
                .with_context(|| format!("cannot install WAD {}", title.path.display()))?;
            context.operations.push(
                json!({"operation":"install-wad","title_id":report["title_id"],"sha256":digest}),
            );
        }
        let (after, inventory_path) = context.inventory(&image, "03-wads")?;
        prepared_verify::verify_inventory_delta(
            &before,
            &after,
            &prepared_verify::PreparedExpectations {
                packages: &context.package_reports,
                ..Default::default()
            },
        )?;
        context.record(image, "03-wads.bin", after, inventory_path)?;
    }
    inventory_refs.insert(
        "prepared".into(),
        context.stages.last().unwrap()["inventory"].clone(),
    );

    if let Some(priiloader_recipe) = context.recipe.priiloader.clone() {
        let wants_hacks = has_hacks(priiloader_recipe.hacks.as_ref());
        let assets = priiloader_payload::prepare(
            &context.options.cache,
            &context.work,
            context.options.offline,
            priiloader_recipe.app.as_deref(),
            wants_hacks,
        )
        .context("cannot prepare the pinned Priiloader payload")?;
        let image = context.work.join("04-priiloader.bin");
        let before = context.before()?.clone();
        let report =
            native::install_priiloader(context.current()?.as_path(), &assets.app, &image, &[])
                .context("cannot install Priiloader")?;
        let (after, inventory_path) = context.inventory(&image, "04-priiloader")?;
        prepared_verify::verify_priiloader_install(&before, &after, &report)?;
        ensure!(
            report["app_sha256"].as_str() == Some(PRIILOADER_APP_SHA256),
            "Priiloader installer reported an unexpected app digest"
        );
        ensure!(
            sha256_file(&assets.app)? == PRIILOADER_APP_SHA256,
            "Priiloader app changed during installation"
        );
        let mut install_report = report.clone();
        install_report["app_source"] = json!(if priiloader_recipe.app.is_some() {
            "recipe"
        } else {
            "verified-cache-or-release"
        });
        install_report["app_path"] = json!(assets.app);
        if let Some(definitions) = &assets.definitions {
            install_report["hacks_source"] = json!("verified-release-cache-or-download");
            install_report["hacks_sha256"] = json!(sha256_file(definitions)?);
        }
        write_json(
            &context.work.join("priiloader-install.json"),
            &install_report,
        )?;
        context
            .operations
            .push(json!({"operation":"install-priiloader","report":"priiloader-install.json"}));
        context.priiloader_report = Some(install_report);
        context.record(image, "04-priiloader.bin", after, inventory_path)?;

        if !settings_empty(&priiloader_recipe.settings) || wants_hacks {
            let image = context.work.join("05-priiloader-settings.bin");
            let before = context.before()?.clone();
            let settings = priiloader_settings(&priiloader_recipe.settings);
            let hacks = priiloader_hacks(priiloader_recipe.hacks.as_ref(), context.recipe.menu);
            let definitions = assets.definitions.as_deref();
            let report = native::set_priiloader_settings(
                context.current()?.as_path(),
                &image,
                &settings,
                definitions,
                &hacks,
            )
            .context("cannot apply Priiloader settings")?;
            let (after, inventory_path) = context.inventory(&image, "05-priiloader-settings")?;
            let inspection = context
                .worker
                .inspect_priiloader(&image)
                .context("cannot inspect prepared Priiloader settings")?;
            let settings_json = settings_json(&settings);
            let hacks_json = hacks_json(priiloader_recipe.hacks.as_ref());
            prepared_verify::verify_priiloader_config(
                &before,
                &after,
                &inspection,
                &report,
                &settings_json,
                &hacks_json,
                menu_name(context.recipe.menu),
            )?;
            write_json(&context.work.join("priiloader-settings.json"), &report)?;
            context.operations.push(
                json!({"operation":"set-priiloader-settings","report":"priiloader-settings.json"}),
            );
            if let Some(install) = context.priiloader_report.as_mut() {
                install["settings"] = report;
            }
            context.record(image, "05-priiloader-settings.bin", after, inventory_path)?;
        }
        inventory_refs.insert(
            "priiloader".into(),
            context.stages.last().unwrap()["inventory"].clone(),
        );
    }

    context.check_inputs_unchanged(&recipe_hash)?;
    ensure!(
        context.current()?.is_file(),
        "candidate is missing before bundle publication"
    );

    let final_inventory = context.before()?.clone();
    let final_identity = final_inventory["identity"].clone();
    let expected_keys_sha = final_identity["keys_sha256"]
        .as_str()
        .context("final inventory omitted authenticated keys SHA-256")?
        .to_owned();
    let keys_path = context.work.join("keys.bin");
    let keys_sha = native::prepare_backupmii_keys(
        context.current()?.as_path(),
        &keys_path,
        &expected_keys_sha,
    )
    .context("cannot prepare the matching BackupMii keys sidecar")?;
    ensure!(
        keys_sha == expected_keys_sha,
        "keys sidecar hash differs from final NAND identity"
    );
    ensure!(
        fs::metadata(&keys_path)?.len() == KEYS_BYTES,
        "keys sidecar has an invalid size"
    );

    let nand_path = context.current()?.clone();
    sync_file(&nand_path)?;
    sync_file(&keys_path)?;
    let nand_sha = sha256_file(&nand_path)?;
    let nand_size = fs::metadata(&nand_path)?.len();
    ensure!(
        nand_size == IMAGE_BYTES,
        "prepared NAND has an invalid image size"
    );
    let output_nand = context.options.output.join("nand.bin");
    let output_keys = context.options.output.join("keys.bin");
    let mut stages = context.stages.clone();
    if let Some(last) = stages.last_mut() {
        last.as_object_mut()
            .context("internal stage record must be an object")?
            .remove("path");
        last["path"] = json!(output_nand);
    }
    let manifest = json!({
        "schema_version":1,
        "kind":"prepared-nand-build",
        "created_utc":utc_timestamp(),
        "menu":menu_name(context.recipe.menu),
        "boot_policy":context.options.boot_policy.name(),
        "recipe":{"path":context.options.recipe,"sha256":recipe_hash},
        "recipe_sha256":recipe_hash,
        "seed_sha256":context.input_hashes["seed"],
        "reference_sha256":context.input_hashes.get("reference"),
        "removals":context.recipe.remove.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "packages":context.package_reports.iter().enumerate().map(|(index, report)| json!({
            "path":context.recipe.titles[index].path,
            "sha256":context.recipe.titles[index].sha256.as_ref().as_str(),
            "signature_policy":if context.recipe.titles[index].signature_policy == SignaturePolicy::AllowUnsigned {"allow-unsigned"} else {"nintendo"},
            "inspection":report,
        })).collect::<Vec<_>>(),
        "display":if context.recipe.display.is_some() { context.display_report.clone() } else { None },
        "setup":context.recipe.setup.as_ref().and(context.display_report.as_ref().map(|report| {
            json!({"setup_complete":report["setup_complete"],"nickname":report["nickname"],"language":report["language"]})
        })),
        "stock":stock,
        "stock_preflight_report":context.stock_manifest.as_ref().map(|value| value["preflight"]["path"].clone()),
        "inventory":inventory_refs,
        "tools":context.tool_hashes,
        "stages":stages,
        "operations":context.operations,
        "evidence_dir":context.work,
        "warnings":context.warnings,
        "priiloader":context.priiloader_report,
        "keys":{"path":output_keys,"sha256":keys_sha,"size":KEYS_BYTES},
        "output":{"path":output_nand,"sha256":nand_sha,"size":nand_size},
    });
    write_json(&context.work.join("bundle-manifest.json"), &manifest)?;
    publish_bundle(&nand_path, &keys_path, &context.options.output)?;
    if let Some(stock_image) = &context.stock_image
        && stock_image != &nand_path
    {
        let _ = fs::remove_file(stock_image);
    }
    let _ = fs::remove_file(&nand_path);
    let _ = fs::remove_file(&keys_path);

    Ok((
        BuildOutcome {
            menu: menu_name(context.recipe.menu),
            nand: output_nand,
            keys: output_keys,
            nand_sha256: nand_sha,
            keys_sha256: keys_sha,
            evidence_dir: context.work.clone(),
        },
        manifest,
    ))
}

impl BuildContext<'_> {
    fn inspect_packages(&mut self) -> Result<()> {
        let mut title_ids = BTreeSet::new();
        for (index, title) in self.recipe.titles.iter().enumerate() {
            let expected = title.sha256.as_ref().as_str();
            let observed = sha256_file(&title.path)?;
            ensure!(
                observed == expected,
                "WAD {} SHA-256 differs from the recipe",
                index
            );
            let report = self
                .worker
                .inspect_wad(
                    &title.path,
                    Some(expected),
                    title.signature_policy == SignaturePolicy::AllowUnsigned,
                )
                .with_context(|| format!("cannot inspect WAD {}", index))?;
            ensure!(
                report["sha256"].as_str() == Some(expected),
                "WAD inspection did not bind its expected SHA-256"
            );
            let id = report["title_id"]
                .as_str()
                .context("WAD inspection omitted title ID")?;
            ensure!(
                id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()),
                "WAD inspection returned an invalid title ID"
            );
            ensure!(
                title_ids.insert(id.to_ascii_lowercase()),
                "recipe selects the same WAD title twice"
            );
            ensure!(
                title.signature_policy == SignaturePolicy::AllowUnsigned
                    || report["nintendo_signed"] == true,
                "WAD lacks valid Nintendo signatures; set signature_policy: allow-unsigned to opt in"
            );
            self.package_reports.push(report);
        }
        let removals: BTreeSet<String> = self
            .recipe
            .remove
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        ensure!(
            title_ids.is_disjoint(&removals),
            "recipe both adds and removes a title"
        );
        write_json(
            &self.work.join("wad-inspections.json"),
            &json!(self.package_reports),
        )?;
        Ok(())
    }

    fn report_package_regions(&mut self) {
        let expected_region = match self.recipe.menu {
            Menu::J43 => 0,
            Menu::U43 => 1,
            Menu::E43 => 2,
        };
        for package in &self.package_reports {
            let Some(region) = package.get("region_code").and_then(Value::as_u64) else {
                continue;
            };
            if region == expected_region || region == 3 {
                continue;
            }
            let id = package["title_id"].as_str().unwrap_or("unknown");
            let name = TITLE_REGION_NAMES
                .iter()
                .find(|(code, _)| *code == region)
                .map(|(_, name)| *name);
            let warning = match name {
                Some(name) => format!(
                    "WAD {id} declares {name} and will be installed on {}. Launching may require a region-free System Menu.",
                    menu_name(self.recipe.menu)
                ),
                None => format!(
                    "WAD {id} declares unknown region code {region}. Launch compatibility with {} is unknown.",
                    menu_name(self.recipe.menu)
                ),
            };
            eprintln!("wiiform: warning: {warning}");
            self.warnings.push(warning);
        }
    }

    fn build_stock(&mut self) -> Result<Value> {
        let area = match self.recipe.menu {
            Menu::E43 => "EUR",
            Menu::U43 => "USA",
            Menu::J43 => "JPN",
        };
        let setting_values = menu_overrides(self.recipe.menu);
        let raw_titles = self
            .worker
            .stock_titles(area)
            .context("cannot obtain native stock title table")?;
        let titles = stock_verify::verify_stock_title_table(&raw_titles)?;
        let title_table_hash = sha256_bytes(&serde_json::to_vec(&titles)?);
        let image = self.work.join("00-stock.bin");
        native::create(
            &self.options.seed,
            &image,
            self.options.boot_policy.native(),
            &setting_values,
        )
        .context("cannot create the stock 4.3 NAND")?;
        ensure!(
            fs::metadata(&image)?.len() == IMAGE_BYTES,
            "created NAND has an invalid image size"
        );
        let mut installed = Vec::with_capacity(titles.len());
        for (id, version) in &titles {
            let id = u64::from_str_radix(id, 16)?;
            native::install_nus_title(
                &image,
                &self.options.cache,
                id,
                *version,
                self.options.offline,
            )
            .with_context(|| format!("cannot install stock title {id:016x} version {version}"))?;
            installed.push(json!({"title_id":format!("{id:016x}"),"version":version}));
        }
        self.operations.push(json!({"operation":"create-stock","menu":menu_name(self.recipe.menu),"titles":installed.len()}));
        let overrides_json = settings_value(&setting_values);
        let reference_hash = self.input_hashes.get("reference").map(String::as_str);
        let candidate_hash = sha256_file(&image)?;
        let report = self
            .worker
            .preflight(
                &self.options.seed,
                self.options.reference.as_deref(),
                &image,
                self.options.boot_policy.name(),
                &setting_pairs(&setting_values),
            )
            .context("native stock preflight failed")?;
        write_json(&self.work.join("preflight.json"), &report)?;
        stock_verify::verify_preflight(
            &report,
            &stock_verify::StockExpectation {
                boot_policy: self.options.boot_policy.name(),
                region: area,
                setting_overrides: &overrides_json,
                reference_requested: self.options.reference.is_some(),
                seed_sha256: &self.input_hashes["seed"],
                reference_sha256: reference_hash,
                candidate_sha256: &candidate_hash,
            },
        )?;

        let pre_normal_inventory = self
            .worker
            .inventory(&image)
            .context("cannot inventory stock NAND")?;
        stock_verify::verify_inventory_image_binding(
            &pre_normal_inventory,
            &candidate_hash,
            IMAGE_BYTES,
        )?;
        write_json(
            &self.work.join("00-stock-before-restore.json"),
            &pre_normal_inventory,
        )?;
        let original_identity = pre_normal_inventory["identity"].clone();
        let original_keys_sha = original_identity["keys_sha256"]
            .as_str()
            .context("stock inventory omitted footer SHA-256")?
            .to_owned();
        let normalized_sha = native::normalize_backupmii_footer(&image, &original_keys_sha)
            .context("cannot normalize the BackupMii footer")?;
        let (inventory, evidence_path) = if normalized_sha.is_empty() {
            (
                pre_normal_inventory,
                self.work.join("00-stock-inventory.json"),
            )
        } else {
            let inventory = self
                .worker
                .inventory(&image)
                .context("cannot inventory normalized stock NAND")?;
            let observed = sha256_file(&image)?;
            stock_verify::verify_inventory_image_binding(&inventory, &observed, IMAGE_BYTES)?;
            let identity = inventory["identity"]
                .as_object()
                .context("normalized identity is malformed")?;
            let mut expected_identity = original_identity
                .as_object()
                .cloned()
                .context("original identity is malformed")?;
            expected_identity.insert("keys_sha256".into(), json!(normalized_sha));
            ensure!(
                identity == &expected_identity,
                "footer normalization changed identity fields other than the footer digest"
            );
            (inventory, self.work.join("00-stock-inventory.json"))
        };
        write_json(&evidence_path, &inventory)?;
        let final_stock_hash = sha256_file(&image)?;
        let final_inventory_size = fs::metadata(&image)?.len();
        let authenticity = report["boot_authenticity"].clone();
        let stock = json!({
            "menu":menu_name(self.recipe.menu),
            "boot_policy":self.options.boot_policy.name(),
            "boot_authenticity":authenticity,
            "settings":{"AREA":area,"GAME":menu_game(self.recipe.menu),"VIDEO":menu_video(self.recipe.menu),"sha256":report["settings_sha256"]},
            "seed":{"path":self.options.seed,"sha256":self.input_hashes["seed"]},
            "reference":self.options.reference.as_ref().map(|path| json!({"path":path,"sha256":self.input_hashes["reference"]})),
            "cache_path":self.options.cache,
            "offline":self.options.offline,
            "stock_titles":{"count":titles.len(),"sha256":title_table_hash},
            "candidate":{"sha256":candidate_hash,"size":IMAGE_BYTES},
            "output":{"sha256":final_stock_hash,"size":final_inventory_size},
            "preflight":{"path":self.work.join("preflight.json"),"status":"pass","input_sha256":report["input_sha256"],"settings_sha256":report["settings_sha256"],"boot_area_sha256":report["boot_area_sha256"]},
            "restore_footer":if normalized_sha.is_empty() { Value::Null } else { json!({"source_keys_sha256":original_keys_sha,"keys_sha256":normalized_sha}) },
        });
        stock_verify::verify_stock_inventory_binding(&stock["output"], &inventory)?;
        self.stock_image = Some(image.clone());
        self.current = Some(image.clone());
        self.before = Some(inventory.clone());
        self.stock_manifest = Some(stock.clone());
        self.stages.push(stage_record(
            "00-stock.bin",
            &image,
            &inventory,
            &evidence_path,
        )?);
        Ok(stock)
    }

    fn current(&self) -> Result<&PathBuf> {
        self.current
            .as_ref()
            .context("stock NAND has not been created")
    }
    fn before(&self) -> Result<&Value> {
        self.before
            .as_ref()
            .context("previous inventory is unavailable")
    }

    fn copy_current(&self, name: &str) -> Result<PathBuf> {
        let path = self.work.join(name);
        ensure!(
            !path.exists() && !path.is_symlink(),
            "temporary stage path already exists"
        );
        let mut input = File::open(self.current()?).context("cannot open previous NAND stage")?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .context("cannot create next NAND stage")?;
        std::io::copy(&mut input, &mut output).context("cannot copy previous NAND stage")?;
        output.sync_all().context("cannot sync copied NAND stage")?;
        Ok(path)
    }

    fn inventory(&self, image: &Path, label: &str) -> Result<(Value, PathBuf)> {
        let report = self
            .worker
            .inventory(image)
            .with_context(|| format!("cannot authenticate {label} NAND"))?;
        let digest = sha256_file(image)?;
        let size = fs::metadata(image)?.len();
        stock_verify::verify_inventory_image_binding(&report, &digest, size)?;
        let evidence = self.work.join(format!("{label}-inventory.json"));
        write_json(&evidence, &report)?;
        Ok((report, evidence))
    }

    fn record(
        &mut self,
        image: PathBuf,
        name: &str,
        inventory: Value,
        evidence: PathBuf,
    ) -> Result<()> {
        let digest = sha256_file(&image)?;
        let size = fs::metadata(&image)?.len();
        self.stages.push(
            json!({"name":name,"path":image,"sha256":digest,"size":size,"inventory":evidence}),
        );
        let previous = self.current.replace(image.clone());
        self.before = Some(inventory);
        if let Some(previous) = previous
            && Some(&previous) != self.stock_image.as_ref()
            && previous != image
        {
            fs::remove_file(previous).context("cannot remove superseded private stage")?;
        }
        Ok(())
    }

    fn check_inputs_unchanged(&self, recipe_hash: &str) -> Result<()> {
        stock_verify::verify_unchanged_digest(
            "recipe",
            recipe_hash,
            &sha256_file(&self.options.recipe)?,
        )?;
        for (name, path) in std::iter::once(("seed", self.options.seed.as_path()))
            .chain(self.options.reference.as_deref().map(|p| ("reference", p)))
        {
            stock_verify::verify_unchanged_digest(
                name,
                &self.input_hashes[name],
                &sha256_file(path)?,
            )?;
        }
        for (index, title) in self.recipe.titles.iter().enumerate() {
            stock_verify::verify_unchanged_digest(
                &format!("WAD {index}"),
                &self.input_hashes[&format!("wad-{index}")],
                &sha256_file(&title.path)?,
            )?;
        }
        if let Some(priiloader) = &self.recipe.priiloader
            && let Some(app) = &priiloader.app
        {
            stock_verify::verify_unchanged_digest(
                "Priiloader app",
                &self.input_hashes["priiloader-app"],
                &sha256_file(app)?,
            )?;
        }
        let current_exe = std::env::current_exe()?;
        ensure!(
            sha256_file(&current_exe)? == self.tool_hashes["wiiform"],
            "Wiiform executable changed during the build"
        );
        ensure!(
            sha256_file(self.worker.executable())? == self.tool_hashes["read_worker"],
            "read-only worker changed during the build"
        );
        Ok(())
    }
}

fn menu_name(menu: Menu) -> &'static str {
    match menu {
        Menu::E43 => "4.3E",
        Menu::U43 => "4.3U",
        Menu::J43 => "4.3J",
    }
}
fn menu_game(menu: Menu) -> &'static str {
    match menu {
        Menu::E43 => "EU",
        Menu::U43 => "US",
        Menu::J43 => "JP",
    }
}
fn menu_video(menu: Menu) -> &'static str {
    match menu {
        Menu::E43 => "PAL",
        Menu::U43 | Menu::J43 => "NTSC",
    }
}
fn menu_overrides(menu: Menu) -> Vec<native::SettingOverride> {
    vec![
        setting(
            "AREA",
            match menu {
                Menu::E43 => "EUR",
                Menu::U43 => "USA",
                Menu::J43 => "JPN",
            },
        ),
        setting("GAME", menu_game(menu)),
        setting("VIDEO", menu_video(menu)),
    ]
}
fn setting(name: &str, value: &str) -> native::SettingOverride {
    native::SettingOverride {
        name: name.to_owned(),
        value: value.to_owned(),
    }
}
fn setting_pairs(settings: &[native::SettingOverride]) -> Vec<(String, String)> {
    settings
        .iter()
        .map(|value| (value.name.clone(), value.value.clone()))
        .collect()
}
fn settings_value(settings: &[native::SettingOverride]) -> Value {
    Value::Object(
        settings
            .iter()
            .map(|s| (s.name.clone(), json!(s.value)))
            .collect(),
    )
}
fn display_json(display: &Display) -> Value {
    let mut value = Map::new();
    if let Some(aspect) = display.aspect {
        value.insert(
            "aspect".into(),
            json!(match aspect {
                Aspect::FourThree => "4:3",
                Aspect::SixteenNine => "16:9",
            }),
        );
    }
    if let Some(progressive) = display.progressive {
        value.insert("progressive".into(), json!(progressive));
    }
    if let Some(pal60) = display.pal60 {
        value.insert("pal60".into(), json!(pal60));
    }
    Value::Object(value)
}
fn setup_json(setup: &recipe::Setup) -> Value {
    let mut value = Map::new();
    if let Some(name) = &setup.nickname {
        value.insert("nickname".into(), json!(name));
    }
    if let Some(language) = setup.language {
        value.insert("language".into(), json!(language_code(language)));
    }
    Value::Object(value)
}
fn language_code(language: Language) -> &'static str {
    match language {
        Language::Ja => "ja",
        Language::En => "en",
        Language::De => "de",
        Language::Fr => "fr",
        Language::Es => "es",
        Language::It => "it",
        Language::Nl => "nl",
    }
}
fn append_display_settings(values: &mut Vec<native::SettingOverride>, display: &Display) {
    if let Some(aspect) = display.aspect {
        values.push(setting(
            "aspect",
            match aspect {
                Aspect::FourThree => "4:3",
                Aspect::SixteenNine => "16:9",
            },
        ));
    }
    if let Some(progressive) = display.progressive {
        values.push(setting(
            "progressive",
            if progressive { "on" } else { "off" },
        ));
    }
    if let Some(pal60) = display.pal60 {
        values.push(setting("pal60", if pal60 { "on" } else { "off" }));
    }
}
fn settings_empty(settings: &PriiloaderSettings) -> bool {
    settings.autoboot.is_none()
        && settings.return_to.is_none()
        && settings.shutdown_to.is_none()
        && settings.preferred_mount_point.is_none()
        && settings.stop_disc.is_none()
        && settings.lid_slot_on_error.is_none()
        && settings.ignore_shutdown_mode.is_none()
        && settings.black_background.is_none()
        && settings.dump_gecko_text.is_none()
        && settings.show_rc_updates.is_none()
}
fn priiloader_settings(value: &PriiloaderSettings) -> Vec<native::SettingOverride> {
    let mut result = Vec::new();
    if let Some(v) = value.autoboot {
        result.push(setting(
            "autoboot",
            match v {
                Autoboot::Disabled => "disabled",
                Autoboot::HomebrewChannel => "homebrew_channel",
                Autoboot::BootmiiIos => "bootmii_ios",
                Autoboot::SystemMenu => "system_menu",
                Autoboot::File => "file",
            },
        ));
    }
    if let Some(v) = value.return_to {
        result.push(setting(
            "return_to",
            match v {
                ReturnTo::SystemMenu => "system_menu",
                ReturnTo::Priiloader => "priiloader",
                ReturnTo::Autoboot => "autoboot",
                ReturnTo::File => "file",
            },
        ));
    }
    if let Some(v) = value.shutdown_to {
        result.push(setting(
            "shutdown_to",
            match v {
                ShutdownTo::None => "none",
                ShutdownTo::Priiloader => "priiloader",
                ShutdownTo::Autoboot => "autoboot",
            },
        ));
    }
    if let Some(v) = value.preferred_mount_point {
        result.push(setting(
            "preferred_mount_point",
            match v {
                MountPoint::Auto => "auto",
                MountPoint::Sd => "sd",
                MountPoint::Usb => "usb",
            },
        ));
    }
    for (key, value) in [
        ("stop_disc", value.stop_disc),
        ("lid_slot_on_error", value.lid_slot_on_error),
        ("ignore_shutdown_mode", value.ignore_shutdown_mode),
        ("black_background", value.black_background),
        ("dump_gecko_text", value.dump_gecko_text),
        ("show_rc_updates", value.show_rc_updates),
    ] {
        if let Some(value) = value {
            result.push(setting(key, if value { "on" } else { "off" }));
        }
    }
    result
}
fn settings_json(settings: &[native::SettingOverride]) -> Value {
    let mut map = Map::new();
    for item in settings {
        let value = match item.value.as_str() {
            "on" => json!(true),
            "off" => json!(false),
            _ => json!(item.value),
        };
        map.insert(item.name.clone(), value);
    }
    Value::Object(map)
}
fn has_hacks(hacks: Option<&HackSettings>) -> bool {
    hacks
        .map(|h| {
            [
                h.fix_480p,
                h.block_online_updates,
                h.block_disc_updates,
                h.block_disc_autoboot,
                h.auto_press_a,
                h.remove_deflicker,
                h.move_disc_channel,
                h.remove_nocopy_save_protection,
                h.replace_health_screen,
                h.no_background_music,
                h.wiimmfi,
                h.region_free_everything,
            ]
            .into_iter()
            .any(|v| v.is_some())
        })
        .unwrap_or(false)
}
fn priiloader_hacks(hacks: Option<&HackSettings>, menu: Menu) -> Vec<native::SettingOverride> {
    let Some(hacks) = hacks else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut add = |alias: &str, title: &str, value: Option<bool>| {
        if let Some(enabled) = value {
            result.push(setting(
                if alias == "region_free_everything" && menu == Menu::J43 {
                    "Region Free EVERYTHING JP"
                } else {
                    title
                },
                if enabled { "on" } else { "off" },
            ));
        }
    };
    add(
        "fix_480p",
        "480p graphics fix in system menu",
        hacks.fix_480p,
    );
    add(
        "block_online_updates",
        "Block Online Updates",
        hacks.block_online_updates,
    );
    add(
        "block_disc_updates",
        "Block Disc Updates",
        hacks.block_disc_updates,
    );
    add(
        "block_disc_autoboot",
        "Block Disc Autoboot",
        hacks.block_disc_autoboot,
    );
    add(
        "auto_press_a",
        "Auto-Press A at Health Screen",
        hacks.auto_press_a,
    );
    add(
        "remove_deflicker",
        "Remove Deflicker",
        hacks.remove_deflicker,
    );
    add(
        "move_disc_channel",
        "Move Disc Channel",
        hacks.move_disc_channel,
    );
    add(
        "remove_nocopy_save_protection",
        "Remove NoCopy Save File Protection",
        hacks.remove_nocopy_save_protection,
    );
    add(
        "replace_health_screen",
        "Replace Health Screen with Backmenu",
        hacks.replace_health_screen,
    );
    add(
        "no_background_music",
        "No System Menu Background Music",
        hacks.no_background_music,
    );
    add("wiimmfi", "Wiimmfi Patch v4", hacks.wiimmfi);
    add(
        "region_free_everything",
        "Region Free EVERYTHING",
        hacks.region_free_everything,
    );
    result
}
fn hacks_json(hacks: Option<&HackSettings>) -> Value {
    let Some(hacks) = hacks else {
        return Value::Object(Map::new());
    };
    let values = [
        ("fix_480p", hacks.fix_480p),
        ("block_online_updates", hacks.block_online_updates),
        ("block_disc_updates", hacks.block_disc_updates),
        ("block_disc_autoboot", hacks.block_disc_autoboot),
        ("auto_press_a", hacks.auto_press_a),
        ("remove_deflicker", hacks.remove_deflicker),
        ("move_disc_channel", hacks.move_disc_channel),
        (
            "remove_nocopy_save_protection",
            hacks.remove_nocopy_save_protection,
        ),
        ("replace_health_screen", hacks.replace_health_screen),
        ("no_background_music", hacks.no_background_music),
        ("wiimmfi", hacks.wiimmfi),
        ("region_free_everything", hacks.region_free_everything),
    ];
    Value::Object(
        values
            .into_iter()
            .filter_map(|(key, value)| value.map(|v| (key.into(), json!(v))))
            .collect(),
    )
}

fn stage_record(
    name: &str,
    image: &Path,
    inventory: &Value,
    inventory_path: &Path,
) -> Result<Value> {
    let size = fs::metadata(image)?.len();
    Ok(
        json!({"name":name,"path":image,"sha256":inventory["image"]["sha256"],"size":size,"inventory":inventory_path}),
    )
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("cannot create evidence file {}", path.display()))?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256Hasher::digest(bytes))
}
fn sha256_file(path: &Path) -> Result<String> {
    let metadata =
        fs::metadata(path).with_context(|| format!("cannot stat input {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "expected regular file at {}",
        path.display()
    );
    let mut file = File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut digest = Sha256Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn sync_file(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("cannot open {} to sync", path.display()))?
        .sync_all()
        .with_context(|| format!("cannot sync {}", path.display()))
}
fn validate_inputs(options: &BuildOptions) -> Result<()> {
    require_regular(&options.seed, "decrypted seed")?;
    require_regular(&options.recipe, "recipe")?;
    if let Some(reference) = &options.reference {
        require_regular(reference, "reference NAND")?;
    }
    for path in [
        &options.seed,
        &options.recipe,
        &options.output,
        &options.cache,
    ] {
        ensure!(
            path.to_str().is_some(),
            "native workflow requires UTF-8 paths"
        );
    }
    ensure!(
        options
            .reference
            .as_ref()
            .is_none_or(|p| p.to_str().is_some()),
        "reference path must be UTF-8"
    );
    Ok(())
}
fn require_regular(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("{label} is unavailable at {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "{label} must be a regular file at {}",
        path.display()
    );
    Ok(())
}
fn validate_output(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!("output already exists; refusing to replace it"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("cannot check whether output exists"),
    }
    let parent = path.parent().context("output path has no parent")?;
    let metadata = fs::symlink_metadata(parent)
        .with_context(|| format!("output parent is unavailable: {}", parent.display()))?;
    ensure!(
        metadata.file_type().is_dir(),
        "output parent must be a real directory"
    );
    Ok(())
}
fn prepare_cache(path: &Path, offline: bool) -> Result<()> {
    if !offline {
        fs::create_dir_all(path)
            .with_context(|| format!("cannot create cache at {}", path.display()))?;
    }
    let metadata = fs::metadata(path)
        .with_context(|| format!("cache is unavailable at {}", path.display()))?;
    ensure!(metadata.is_dir(), "cache must be a directory");
    Ok(())
}
fn absolute_options(options: &BuildOptions) -> Result<BuildOptions> {
    let current = std::env::current_dir().context("cannot resolve current directory")?;
    let absolute = |path: &Path| -> PathBuf {
        let expanded = if path == Path::new("~") {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| path.to_owned())
        } else if let Ok(text) = path.strip_prefix("~") {
            let text = text.to_string_lossy();
            if text.starts_with(std::path::MAIN_SEPARATOR) {
                std::env::var_os("HOME")
                    .map(|home| {
                        PathBuf::from(home).join(text.trim_start_matches(std::path::MAIN_SEPARATOR))
                    })
                    .unwrap_or_else(|| path.to_owned())
            } else {
                path.to_owned()
            }
        } else {
            path.to_owned()
        };
        if expanded.is_absolute() {
            expanded
        } else {
            current.join(expanded)
        }
    };
    Ok(BuildOptions {
        seed: absolute(&options.seed),
        recipe: absolute(&options.recipe),
        output: absolute(&options.output),
        cache: absolute(&options.cache),
        reference: options.reference.as_deref().map(absolute),
        offline: options.offline,
        boot_policy: options.boot_policy,
        menu: options.menu,
    })
}
fn output_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("output")
}
fn utc_timestamp() -> String {
    use std::ffi::CStr;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut broken = unsafe { std::mem::zeroed::<libc::tm>() };
    if unsafe { libc::gmtime_r(&now, &mut broken) }.is_null() {
        return "unknown".into();
    }
    let mut buffer = [0_i8; 32];
    let format = b"%Y-%m-%dT%H:%M:%SZ\0";
    let count = unsafe {
        libc::strftime(
            buffer.as_mut_ptr(),
            buffer.len(),
            format.as_ptr().cast(),
            &broken,
        )
    };
    if count == 0 {
        return "unknown".into();
    }
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn publish_bundle(nand: &Path, keys: &Path, target: &Path) -> Result<()> {
    let parent = target.parent().context("bundle output has no parent")?;
    let staging = Builder::new()
        .prefix(".wiiform-publish-")
        .rand_bytes(8)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)
        .context("cannot create private bundle staging directory")?;
    let pending = staging.path().join("bundle");
    fs::create_dir(&pending)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
    }
    fs::hard_link(keys, pending.join("keys.bin")).context("cannot stage keys sidecar")?;
    fs::hard_link(nand, pending.join("nand.bin")).context("cannot stage NAND image")?;
    rename_noreplace(&pending, target)
        .context("cannot publish bundle without replacing an existing path")?;
    Ok(())
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

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_noreplace(_source: &Path, _target: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace directory rename is unsupported on this platform",
    ))
}
