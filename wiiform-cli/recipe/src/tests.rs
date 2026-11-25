// SPDX-License-Identifier: GPL-2.0-only
use std::fs;

use base64::{Engine, engine::general_purpose::STANDARD};

use super::*;
use crate::{
    nand::{
        menu::Menu,
        priiloader::{Autoboot, BackgroundMusic, DiscIos, Hack, ReturnTo, UsbGecko},
        setup::{Display, Language, SensorBar, display::Aspect},
        titles::cios::{Kind as CiosKind, d2x::Module},
    },
    wad::tests::with_fetch,
};

const WAD: [u8; 0x40] = [7; 0x40];

/// `yaml` as the `nand` key of a recipe.
fn nested(yaml: &str) -> String {
    let indented: String = yaml.lines().map(|line| format!("  {line}\n")).collect();
    format!("nand:\n{indented}")
}

/// Reads `yaml` as the `nand` part of a recipe.
fn read(yaml: &str) -> Result<Nand> {
    read_recipe(&nested(yaml)).map(|recipe| recipe.nand)
}

fn read_recipe(yaml: &str) -> Result<Recipe> {
    read_recipe_with(yaml, Collisions::default())
}

fn read_recipe_with(yaml: &str, collisions: Collisions) -> Result<Recipe> {
    Recipe::read(yaml.as_bytes(), Path::new("."), &store(), collisions)
}

/// Where the tests' downloads are kept: one directory per test process.
fn store() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("wiiform-recipe-tests-{}", std::process::id()))
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::of(bytes).0)
}

/// The keys of a WAD entry carrying `WAD` inline.
fn inline() -> String {
    format!("inline: {}, sha256: {}", STANDARD.encode(WAD), sha(&WAD))
}

fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
    format!("{:#}", result.expect_err("the recipe must be rejected"))
}

#[test]
fn hbc_reborn_uses_the_release_digest_or_a_supplied_hash() -> Result<()> {
    let digest = sha(&WAD);
    for (version, name, sha256, asset_digest) in [
        (
            "2.3.4",
            "hbc-reborn-2.3.4-OHBC.wad",
            String::new(),
            format!("\"sha256:{digest}\""),
        ),
        (
            "1.12.1",
            "hbc-1.12.1-OHBC.wad",
            format!(", sha256: {digest}"),
            "null".to_owned(),
        ),
    ] {
        let yaml = format!("menu: 4.3E\nhbc: {{reborn: {{version: {version}{sha256}}}}}");
        let recipe = with_fetch(
            move |url, _| {
                if url.starts_with("https://api.github.com/") {
                    assert!(url.ends_with(&format!("/releases/tags/v{version}")));
                    Ok(format!(
                        r#"{{"assets":[{{"name":"other.wad"}},{{"name":"{name}","digest":{asset_digest}}}]}}"#
                    ).into_bytes())
                } else {
                    assert!(url.ends_with(&format!("/v{version}/{name}")));
                    Ok(WAD.to_vec())
                }
            },
            || read(&yaml),
        )?;
        assert_eq!(&*recipe.hbc.unwrap().0, WAD);
    }
    for metadata in [
        r#"{"assets":[{"name":"hbc-reborn-2.3.4-LULZ.wad"}]}"#,
        r#"{"assets":[{"name":"hbc-reborn-2.3.4-OHBC.wad"}]}"#,
        r#"{"assets":[{"name":"hbc-reborn-2.3.4-OHBC.wad","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}]}"#,
    ] {
        let result = with_fetch(
            move |url, _| {
                if url.starts_with("https://api.github.com/") {
                    Ok(metadata.as_bytes().to_vec())
                } else {
                    Ok(WAD.to_vec())
                }
            },
            || read("menu: 4.3E\nhbc: {reborn: {version: 2.3.4}}"),
        );
        assert!(result.is_err(), "{metadata}");
    }
    Ok(())
}

#[test]
fn hbc_takes_exactly_one_variant() -> Result<()> {
    let wad = inline();
    let recipe = read(&format!("menu: 4.3E\nhbc: {{custom: {{{wad}}}}}"))?;
    assert_eq!(&*recipe.hbc.unwrap().0, WAD);
    for fields in [
        "reborn: {version: '1.2'}".to_owned(),
        "classic: {}".to_owned(),
        "custom: {path: file.wad}".to_owned(),
        format!("custom: {{version: 1.16.0, {wad}}}"),
        format!("reborn: {{version: 1.16.0}}, custom: {{{wad}}}"),
    ] {
        assert!(
            read(&format!("menu: 4.3E\nhbc: {{{fields}}}")).is_err(),
            "{fields}"
        );
    }
    Ok(())
}

#[test]
fn a_wad_is_one_source_whose_bytes_match_its_hash() -> Result<()> {
    let title = |fields: &str| {
        read(&format!(
            "menu: 4.3E\ntitles:\n  channels:\n    - {fields}\n"
        ))
    };
    let encoded = STANDARD.encode(WAD);
    let digest = sha(&WAD);
    let (first, second) = encoded.split_at(40);
    let block = format!("sha256: {digest}\n      inline: |\n        {first}\n        {second}");
    assert_eq!(&*title(&block)?.titles.channels[0].wad, WAD);

    // The recipe holds inline bytes itself, so their SHA-256 is optional.
    assert_eq!(
        &*title(&format!("inline: {encoded}"))?.titles.channels[0].wad,
        WAD
    );
    // A URL needs one, and is refused before anything is downloaded.
    assert!(message(title("url: https://example.com/a.wad")).contains("required for a url"));

    let wrong = sha(b"something else");
    let small = STANDARD.encode([7; 0x3f]);
    assert!(
        message(title(&format!("inline: {encoded}\n      sha256: {wrong}"))).contains("SHA-256")
    );
    assert!(
        message(title(&format!(
            "inline: {small}\n      sha256: {}",
            sha(&[7; 0x3f])
        )))
        .contains("size")
    );
    for bad in [
        format!("inline: {encoded}\n      path: a.wad\n      sha256: {digest}"),
        format!("inline: {encoded}\n      url: https://example.com/a.wad\n      sha256: {digest}"),
        format!("sha256: {digest}"),
        format!("inline: '***'\n      sha256: {digest}"),
        format!("inline: !!binary {encoded}\n      sha256: {digest}"),
        format!("path: null\n      sha256: {digest}"),
        format!("inline: {encoded}\n      sha256: {digest}\n    unexpected: true"),
    ] {
        assert!(title(&bad).is_err(), "{bad}");
    }
    Ok(())
}

#[test]
fn a_url_source_is_fetched_within_the_wad_size_limit() -> Result<()> {
    const URL: &str = "https://example.com/package.wad";
    let yaml = format!(
        "menu: 4.3E\ntitles: {{channels: [{{url: {URL}, sha256: {}}}]}}",
        sha(&WAD)
    );
    let recipe = with_fetch(
        |requested, limit| {
            assert_eq!(requested, URL);
            assert_eq!(limit, 0x2100_0000);
            Ok(WAD.to_vec())
        },
        || read(&yaml),
    )?;
    assert_eq!(&*recipe.titles.channels[0].wad, WAD);
    assert!(with_fetch(|_, _| Ok(vec![]), || read(&yaml)).is_err());
    assert!(with_fetch(|_, _| anyhow::bail!("offline"), || read(&yaml)).is_err());
    Ok(())
}

#[test]
fn titles_and_cios_accept_only_their_own_fields() -> Result<()> {
    let target = "target: {slot: 249, base: 56, revision: 65535}";
    let wad = inline();
    let titles = |fields: &str| read(&format!("menu: 4.3E\ntitles: {{channels: [{{{fields}}}]}}"));
    let cios = |fields: &str| read(&format!("menu: 4.3E\ntitles: {{cios: [{{{fields}}}]}}"));

    assert!(!titles(&wad)?.titles.channels[0].allow_unsigned);
    assert!(
        titles(&format!("{wad}, allow_unsigned: true"))?
            .titles
            .channels[0]
            .allow_unsigned
    );
    let recipe = cios(&format!("{wad}, {target}"))?;
    let cios_entry = &recipe.titles.cios[0];
    assert_eq!(
        Some(cios_entry.target.title_id()),
        TitleId::new(0x0000_0001_0000_00f9)
    );
    assert!(matches!(&cios_entry.kind, CiosKind::Prebuilt(wad) if **wad == WAD));

    for fields in [
        format!("{wad}, unexpected: true"),
        format!("{wad}, {target}"),
        format!("{wad}, allow_unsigned: false, allow_unsigned: true"),
        format!("source: {{{wad}}}"),
    ] {
        assert!(titles(&fields).is_err(), "{fields}");
    }
    for fields in [
        target.to_owned(),
        wad.clone(),
        format!("{wad}, {target}, allow_unsigned: true"),
    ] {
        assert!(cios(&fields).is_err(), "{fields}");
    }
    Ok(())
}

#[test]
fn a_path_is_relative_to_the_recipe_and_names_a_regular_file() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("wiiform-path-test-{}", std::process::id()));
    fs::create_dir(&dir)?;
    fs::write(dir.join("payload.wad"), WAD)?;
    std::os::unix::fs::symlink("payload.wad", dir.join("linked.wad"))?;
    let digest = sha(&WAD);
    let title = |path: &str| {
        format!("menu: 4.3E\ntitles: {{channels: [{{path: {path}, sha256: {digest}}}]}}")
    };

    let recipe = Recipe::read(
        nested(&title("linked.wad")).as_bytes(),
        &dir,
        &store(),
        Collisions::default(),
    )?
    .nand;
    assert_eq!(&*recipe.titles.channels[0].wad, WAD);
    for bad in [".", "missing.wad"] {
        assert!(
            Recipe::read(
                nested(&title(bad)).as_bytes(),
                &dir,
                &store(),
                Collisions::default(),
            )
            .is_err(),
            "{bad}"
        );
    }
    assert!(read(&title("linked.wad")).is_err(), "another directory");
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_local_path_needs_no_sha256_but_is_checked_against_one() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("wiiform-local-test-{}", std::process::id()));
    fs::create_dir(&dir)?;
    fs::write(dir.join("payload.wad"), WAD)?;
    fs::write(dir.join("gc.cfg"), b"config")?;
    let read = |nand: &str, sd: &str| {
        Recipe::read(
            format!("nand: {{menu: 4.3E{nand}}}\nsd: {sd}").as_bytes(),
            &dir,
            &store(),
            Collisions::default(),
        )
    };
    let channel = |hash: &str| format!(", titles: {{channels: [{{path: payload.wad{hash}}}]}}");
    let bundle = |hash: &str| {
        format!("{{bundles: [{{path: ./gc.cfg{hash}, to: /rvloader/defaults/gc.cfg}}]}}")
    };

    let recipe = read(&channel(""), &bundle(""))?;
    assert_eq!(&*recipe.nand.titles.channels[0].wad, WAD);
    let files: Vec<(String, Vec<u8>)> = (recipe.sd.iter().flat_map(|sd| sd.files()))
        .map(|(path, blob)| Ok((path.to_string(), blob.bytes()?)))
        .collect::<Result<_>>()?;
    assert_eq!(
        files,
        [("/rvloader/defaults/gc.cfg".to_owned(), b"config".to_vec())]
    );

    let wrong = format!(", sha256: {}", sha(b"other"));
    assert!(message(read(&channel(&wrong), "{}")).contains("does not match its SHA-256"));
    assert!(message(read("", &bundle(&wrong))).contains("does not match its SHA-256"));
    read(
        &channel(&format!(", sha256: {}", sha(&WAD))),
        &bundle(&format!(", sha256: {}", sha(b"config"))),
    )?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn removals_and_setup_follow_the_menu_after_an_override() -> Result<()> {
    let mut recipe = read("menu: 4.3E\nremove: [news-channel, forecast-channel, news-channel]")?;
    let ids = |recipe: &Nand| -> Vec<u64> {
        (recipe.remove.iter())
            .flat_map(|channel| channel.title_ids(recipe.menu))
            .map(TitleId::as_u64)
            .collect()
    };
    assert_eq!(
        ids(&recipe),
        [
            0x0001_0002_4841_4650,
            0x0001_0002_4841_4641,
            0x0001_0002_4841_4750,
            0x0001_0002_4841_4741
        ]
    );
    recipe.menu = Menu::J43;
    recipe.validate()?;
    assert_eq!(ids(&recipe)[0], 0x0001_0002_4841_464a);

    let mut recipe = read("menu: 4.3E\nsetup: {language: de, country: FR}")?;
    recipe.menu = Menu::U43;
    assert!(recipe.validate().is_err());
    for yaml in [
        "menu: 4.3E\nsetup: {language: en, country: '110'}",
        "menu: 4.3E\nsetup: {language: ja, country: GB}",
        "menu: 4.3J\nsetup: {language: ja, country: GB}",
        "menu: 4.3E\nremove: [disc-channel]",
        "menu: 4.3E\nremove: ['0001000248414650']",
    ] {
        assert!(read(yaml).is_err(), "{yaml}");
    }
    Ok(())
}

#[test]
fn display_is_part_of_first_run_setup() -> Result<()> {
    for yaml in ["menu: 4.3E", "menu: 4.3E\nsetup: null"] {
        assert!(read(yaml)?.setup.is_none());
    }
    // The locale has no default: wiiform would have to guess it from the menu.
    for yaml in ["{}", "{language: fr}", "{country: FR}"] {
        let result = read(&format!("menu: 4.3E\nsetup: {yaml}"));
        assert!(message(result).contains("missing field"), "{yaml}");
    }
    let setup = read("menu: 4.3E\nsetup: {language: fr, country: FR}")?.setup;
    let setup = setup.context("expected setup settings")?;
    assert_eq!(
        (setup.language, setup.country.as_alpha2()),
        (Language::Fr, "FR")
    );
    assert_eq!(setup.nickname.as_ref(), "Wii");
    assert_eq!(setup.sensor_bar, SensorBar::Above);
    assert_eq!(setup.display, Display::default());

    let recipe = read(
        "menu: 4.3E\nsetup: {language: en, country: GB, nickname: 'Éééééééééé', display: {aspect: '16:9', progressive: true}}",
    )?;
    let setup = recipe.setup.context("expected setup settings")?;
    assert_eq!(setup.nickname.as_ref(), "Éééééééééé");
    assert_eq!(setup.display.aspect, Aspect::SixteenNine);
    assert!(setup.display.progressive);
    assert!(!setup.display.pal60);
    assert!(read("menu: 4.3E\ndisplay: {}").is_err());
    Ok(())
}

#[test]
fn priiloader_takes_settings_and_a_list_of_hacks() -> Result<()> {
    let recipe = read(
        "menu: 4.3E\npriiloader: {stop_disc: true, autoboot: homebrew_channel, hacks: [wiimmfi, fix_480p]}",
    )?;
    let config = recipe.priiloader.context("expected Priiloader settings")?;
    assert!(config.stop_disc && !config.white_background);
    assert_eq!(config.autoboot, Some(Autoboot::HomebrewChannel));
    assert_eq!(config.hacks, [Hack::Wiimmfi, Hack::Fix480p]);

    let recipe = read("menu: 4.3E\npriiloader: {}")?;
    assert_eq!(recipe.priiloader, Some(Priiloader::default()));
    assert_eq!(Autoboot::default(), Autoboot::SystemMenu);
    assert!(read("menu: 4.3E\npriiloader: null")?.priiloader.is_none());
    for priiloader in [
        "{app: override.app}",
        "{hacks: [unknown]}",
        "{hacks: {fix_480p: true}}",
    ] {
        assert!(
            read(&format!("menu: 4.3E\npriiloader: {priiloader}")).is_err(),
            "{priiloader}"
        );
    }
    Ok(())
}

/// An RVLoader release whose installer carries `bootloaders`, and whose Wi-Fi
/// driver is `wifi_module`, under the release's versioned top folder.
fn rvloader_release(bootloaders: &[&[u8]], wifi_module: &[u8]) -> Vec<u8> {
    let mut installer = crate::nand::rvloader::sample_dol(&[0; 64]);
    for bootloader in bootloaders {
        installer.extend_from_slice(bootloader);
    }
    // Priiloader's app, which the installer carries too.
    installer.extend(crate::nand::rvloader::sample_dol(b"Priiloader"));
    installer.extend_from_slice(wifi_module);
    zip_of(
        &[
            ("RVLoader_v2.1/apps/RVLoader/installer.dol", &installer),
            ("RVLoader_v2.1/rvloader/Hiidra/modules/wd.bin", wifi_module),
            ("RVLoader_v2.1/apps/RVLoader/boot.dol", b"RVLoader itself"),
        ],
        &[],
    )
}

#[test]
fn rvloader_requires_priiloader_and_owns_its_boot_settings() -> Result<()> {
    let dol = crate::nand::rvloader::sample_dol(b"RVLoader bootloader 00");
    const WD: [u8; 4] = [4, 5, 6, 7];
    let release = rvloader_release(&[&dol], &WD);
    let rvloader = |sha256: &str, options: &str| {
        format!(
            "rvloader: {{release: {{inline: {}, sha256: {sha256}}}, {options}}}",
            STANDARD.encode(&release)
        )
    };
    let read = |yaml: String| read(&format!("menu: 4.3E\n{yaml}"));
    let options = "patch_out_wifi: true, vga: false";
    let good = rvloader(&sha(&release), options);

    let recipe = read(format!("{good}\npriiloader: {{stop_disc: true}}"))?;
    let loaded = recipe.rvloader.as_ref().context("expected RVLoader")?;
    assert_eq!(
        (&*loaded.bootloader, &*loaded.wifi_module),
        (&dol[..], &WD[..])
    );
    let ios = loaded.ios();
    assert_eq!(ios.map(|ios| ios.slot), [236, 240, 241, 36, 58, 80]);
    assert!(ios.iter().all(|ios| ios.without_wifi));
    assert_eq!(
        ios.map(|ios| ios.es_patches),
        [true, false, false, false, false, false]
    );
    let config = recipe
        .priiloader
        .context("expected Priiloader")?
        .booting_file();
    assert_eq!(config.autoboot, Some(Autoboot::File));
    assert_eq!(config.return_to, Some(ReturnTo::Autoboot));
    assert!(config.stop_disc);

    let keep_wifi = read(format!(
        "{}\npriiloader: {{}}",
        rvloader(&sha(&release), "patch_out_wifi: false, vga: false")
    ))?;
    let ios = keep_wifi.rvloader.context("expected RVLoader")?.ios();
    assert_eq!(
        ios.map(|ios| ios.without_wifi),
        [true, true, true, false, false, false]
    );

    assert!(message(read(good.clone())).contains("requires a priiloader section"));
    for setting in [
        "autoboot: file",
        "autoboot: system_menu",
        "return_to: autoboot",
    ] {
        let result = read(format!("{good}\npriiloader: {{{setting}}}"));
        assert!(message(result).contains("remove them"), "{setting}");
    }
    let vga = rvloader(&sha(&release), "patch_out_wifi: true, vga: true");
    for (setup, error) in [
        ("", "requires setup.display.progressive"),
        (
            "setup: {language: en, country: GB, display: {pal60: true}}",
            "requires setup.display.progressive",
        ),
        (
            "setup: {language: en, country: GB, display: {progressive: true}}",
            "requires setup.display.pal60",
        ),
    ] {
        let result = read(format!("{vga}\npriiloader: {{}}\n{setup}"));
        assert!(message(result).contains(error), "{setup}");
    }
    read(format!(
        "{vga}\npriiloader: {{}}\nsetup: {{language: en, country: GB, display: {{progressive: true, pal60: true}}}}"
    ))?;

    let wrong = rvloader(&sha(&WAD), options);
    assert!(message(read(wrong)).contains("does not match its SHA-256"));
    for missing in ["patch_out_wifi: true", "vga: false"] {
        assert!(
            read(rvloader(&sha(&release), missing)).is_err(),
            "{missing}"
        );
    }
    Ok(())
}

#[test]
fn rvloader_takes_its_files_from_the_release() -> Result<()> {
    let bootloader = crate::nand::rvloader::sample_dol(b"RVLoader bootloader 00");
    let other = crate::nand::rvloader::sample_dol(b"RVLoader bootloader 01");
    let from = |release: &[u8]| Rvloader::from_release(release, false, false);
    let installer = |parts: &[&[u8]]| {
        let mut installer = crate::nand::rvloader::sample_dol(&[0; 64]);
        installer.extend(parts.concat());
        installer
    };
    let loaded = from(&rvloader_release(&[&bootloader], b"driver"))?;
    assert_eq!(&*loaded.bootloader, &bootloader[..]);
    assert_eq!(&*loaded.wifi_module, b"driver");
    // A release laid out without a top folder works too.
    let flat = zip_of(
        &[
            (
                "apps/RVLoader/installer.dol",
                &installer(&[&bootloader, b"driver"]),
            ),
            ("rvloader/Hiidra/modules/wd.bin", b"driver"),
        ],
        &[],
    );
    assert_eq!(&*from(&flat)?.bootloader, &bootloader[..]);

    for (release, error) in [
        (rvloader_release(&[], b"driver"), "carries 0 DOLs"),
        (
            rvloader_release(&[&bootloader, &other], b"driver"),
            "carries 2 DOLs",
        ),
        (
            zip_of(&[("apps/RVLoader/boot.dol", b"x")], &[]),
            "holds no apps/RVLoader/installer.dol",
        ),
        (
            zip_of(
                &[("apps/RVLoader/installer.dol", &installer(&[&bootloader]))],
                &[],
            ),
            "holds no rvloader/Hiidra/modules/wd.bin",
        ),
        (
            zip_of(
                &[
                    (
                        "apps/RVLoader/installer.dol",
                        &installer(&[&bootloader, b"driver"]),
                    ),
                    (
                        "rvloader/Hiidra/modules/wd.bin",
                        b"a driver the installer lacks",
                    ),
                ],
                &[],
            ),
            "is not the Wi-Fi driver",
        ),
    ] {
        assert!(message(from(&release)).contains(error), "{error}");
    }
    Ok(())
}

/// The keys `yaml`, as the `nand` part of a recipe, leaves to Wiiform.
fn defaulted(yaml: &str) -> Result<Vec<String>> {
    let recipe = read_recipe(&nested(yaml))?;
    Ok((recipe.defaulted.iter())
        .map(|defaulted| format!("{}={}", defaulted.key, defaulted.value))
        .collect())
}

#[test]
fn the_build_is_told_each_value_the_recipe_leaves_out() -> Result<()> {
    let all = [
        "nand.priiloader.autoboot=system_menu",
        "nand.priiloader.dump_gecko_text=false",
        "nand.priiloader.ignore_shutdown_mode=false",
        "nand.priiloader.lid_slot_on_error=false",
        "nand.priiloader.preferred_mount_point=auto",
        "nand.priiloader.return_to=system_menu",
        "nand.priiloader.show_rc_updates=false",
        "nand.priiloader.shutdown_to=none",
        "nand.priiloader.stop_disc=false",
        "nand.priiloader.white_background=false",
        "nand.setup.display.aspect=4:3",
        "nand.setup.display.pal60=false",
        "nand.setup.display.progressive=false",
        "nand.setup.nickname=Wii",
        "nand.setup.sensor_bar=above",
    ];
    let bare = "menu: 4.3E\nsetup: {language: en, country: GB}\npriiloader: {}";
    assert_eq!(defaulted(bare)?, all);

    // Sections left out, empty lists and safety switches are not defaults.
    assert!(defaulted("menu: 4.3E")?.is_empty());
    let channel = format!("menu: 4.3E\ntitles: {{channels: [{{{}}}]}}", inline());
    assert!(defaulted(&channel)?.is_empty());

    let partial = "menu: 4.3E\nsetup: {language: en, country: GB, nickname: Speed, display: {aspect: '16:9'}}";
    assert_eq!(
        defaulted(partial)?,
        [
            "nand.setup.display.pal60=false",
            "nand.setup.display.progressive=false",
            "nand.setup.sensor_bar=above",
        ]
    );
    let given = "menu: 4.3E
setup: {language: en, country: GB, nickname: Wii, sensor_bar: above,
  display: {aspect: '4:3', progressive: false, pal60: false}}
priiloader: {autoboot: system_menu, return_to: system_menu, shutdown_to: none,
  preferred_mount_point: auto, stop_disc: false, lid_slot_on_error: false,
  ignore_shutdown_mode: false, white_background: false, dump_gecko_text: false,
  show_rc_updates: false}";
    assert!(defaulted(given)?.is_empty(), "stated, even as the default");

    // Every key of these mappings that may be left out is reported.
    let schema: serde_json::Value = serde_json::from_str(&crate::json_schema()?)?;
    for (name, path) in [
        ("Setup", "nand.setup"),
        ("Display", "nand.setup.display"),
        ("Priiloader", "nand.priiloader"),
    ] {
        let definition = &schema["$defs"][name];
        let properties = definition["properties"]
            .as_object()
            .context("expected keys")?;
        let required = definition["required"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for key in properties.keys() {
            let reported = all.iter().any(|entry| {
                entry.starts_with(&format!("{path}.{key}="))
                    || entry.starts_with(&format!("{path}.{key}."))
            });
            let optional = !required.contains(&key.as_str().into());
            assert!(
                reported
                    || !optional
                    || matches!(
                        key.as_str(),
                        "hacks" | "background_music" | "usb_gecko" | "force_disc_ios"
                    ),
                "{path}.{key} may be left out but is not reported"
            );
        }
    }
    Ok(())
}

#[test]
fn rvloader_sets_priiloader_autoboot_and_return_to_rather_than_defaults() -> Result<()> {
    let release = rvloader_release(
        &[&crate::nand::rvloader::sample_dol(
            b"RVLoader bootloader 00",
        )],
        b"driver",
    );
    let yaml = format!(
        "menu: 4.3E\npriiloader: {{}}\nrvloader: {{release: {{inline: {}}}, patch_out_wifi: false, vga: false}}",
        STANDARD.encode(&release)
    );
    let defaulted = defaulted(&yaml)?;
    assert!(!defaulted.is_empty());
    assert!(
        (defaulted.iter())
            .all(|entry| !entry.contains(".autoboot=") && !entry.contains(".return_to=")),
        "{defaulted:?}"
    );
    Ok(())
}

#[test]
fn the_schema_requires_sha256_beside_a_url_only() -> Result<()> {
    let schema: serde_json::Value = serde_json::from_str(&crate::json_schema()?)?;
    let validator = jsonschema::validator_for(&schema)?;
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let github = "github: {repo: owner/app, tag: v1, asset: app.zip}";
    let target = "target: {slot: 249, base: 56, revision: 65535}";
    let recipe = |yaml: String| -> Result<serde_json::Value> {
        Ok(serde_saphyr::from_str(&format!(
            "nand: {{menu: 4.3E}}\n{yaml}"
        ))?)
    };
    let entries = |source: &str| {
        [
            format!("sd: {{bundles: [{{{source}, to: /x}}]}}"),
            format!("sd: {{apps: {{x: {{{source}}}}}}}"),
            format!("sd: {{apps: {{x: {{files: {{boot.dol: {{{source}}}}}}}}}}}"),
        ]
    };
    let nand = |source: &str| {
        [
            format!("{{channels: [{{{source}}}]}}"),
            format!("{{cios: [{{{source}, {target}}}]}}"),
            format!("{{cios: [{{d2x: {{{source}}}, {target}}}]}}"),
        ]
        .map(|titles| format!("nand: {{menu: 4.3E, titles: {titles}}}"))
        .into_iter()
        .chain([format!(
            "nand: {{menu: 4.3E, priiloader: {{}}, rvloader: {{release: {{{source}}}, patch_out_wifi: false, vga: false}}}}"
        )])
    };
    let all = |source: &str| -> Result<Vec<serde_json::Value>> {
        let nand = nand(source).map(|yaml| serde_saphyr::from_str(&yaml));
        let mut values: Vec<serde_json::Value> = nand.into_iter().collect::<Result<_, _>>()?;
        for yaml in entries(source) {
            values.push(recipe(yaml)?);
        }
        Ok(values)
    };
    for source in [
        "path: x.zip".to_owned(),
        "inline: AAAA".to_owned(),
        github.to_owned(),
        format!("url: 'https://example.com/x.zip', sha256: {hash}"),
        format!("path: x.zip, sha256: {hash}"),
    ] {
        for value in all(&source)? {
            let errors: Vec<String> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{source} in {value}: {errors:?}");
        }
    }
    for value in all("url: 'https://example.com/x.zip'")? {
        assert!(
            !validator.is_valid(&value),
            "a url without sha256 in {value}"
        );
    }
    Ok(())
}

#[test]
fn the_documented_example_is_a_valid_recipe() -> Result<()> {
    // The example names a WAD that does not exist; give it one.
    let example = include_str!("../../../docs/recipe.example.yaml");
    let placeholder = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let example = example
        .replacen(
            "path: wads/example-pal-channel.wad",
            &format!("inline: {}", STANDARD.encode(WAD)),
            1,
        )
        .replacen(placeholder, &sha(&WAD), 1);
    let app = zip_of(&[("apps/Example/boot.dol", b"dol")], &[]);
    let example = example.replacen(
        &format!("path: sd/example-app.zip\n      sha256: \"{placeholder}\""),
        &format!(
            "inline: {}\n      sha256: {}",
            STANDARD.encode(&app),
            sha(&app)
        ),
        1,
    );
    let recipe = read_recipe(&example)?;
    assert_eq!(recipe.sd.as_ref().context("expected sd")?.files().len(), 1);
    let recipe = recipe.nand;
    assert_eq!(recipe.remove.len(), 2);
    assert_eq!(&*recipe.titles.channels[0].wad, WAD);
    assert_eq!(recipe.priiloader.unwrap().hacks, [Hack::Fix480p]);
    Ok(())
}

#[test]
fn the_documented_example_lists_every_hack_and_removable_channel() -> Result<()> {
    let example = include_str!("../../../docs/recipe.example.yaml");
    let schema: serde_json::Value = serde_json::from_str(&crate::json_schema()?)?;
    for (list, enumeration) in [("hacks", "Hack"), ("remove", "RemovableChannel")] {
        let values = (schema["$defs"][enumeration]["oneOf"].as_array())
            .context("expected described values")?;
        for value in values {
            let name = value["const"].as_str().context("expected a value name")?;
            // An entry, commented out or not, may carry a trailing comment.
            let entry = format!("- {name}");
            let listed = |line: &str| {
                let line = line.trim_start_matches([' ', '#']);
                line.split(" #").next() == Some(entry.as_str())
            };
            assert!(
                example.lines().any(listed),
                "docs/recipe.example.yaml does not list {name} under {list}"
            );
        }
    }
    Ok(())
}

#[test]
fn cios_slots_and_bases_are_checked_when_the_recipe_is_read() {
    let cios = |slot: u8, base: u8| {
        format!(
            "{{{}, target: {{slot: {slot}, base: {base}, revision: 1}}}}",
            inline()
        )
    };
    let recipe = |entries: &[String]| {
        read(&format!(
            "menu: 4.3E\ntitles: {{cios: [{}]}}",
            entries.join(", ")
        ))
    };
    assert!(recipe(&[cios(249, 56), cios(250, 57)]).is_ok());
    assert!(recipe(&[cios(247, 56)]).is_err());
    assert!(recipe(&[cios(249, 55)]).is_err());
    assert!(message(recipe(&[cios(249, 56), cios(249, 57)])).contains("twice"));
}

/// The committed schema is what editors use, so it must match the types.
/// Run with `UPDATE_SCHEMA=1` to rewrite it.
#[test]
fn committed_schema_matches_the_recipe_types() -> Result<()> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/wiiform.recipe.schema.json");
    let schema = crate::json_schema()? + "\n";
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        fs::write(&path, &schema)?;
    }
    assert!(
        fs::read_to_string(&path)? == schema,
        "docs/wiiform.recipe.schema.json is stale: run `mise run schema`"
    );
    Ok(())
}

/// A zip of `files`, with `symlinks` as (name, target) pairs.
fn zip_of(files: &[(&str, &[u8])], symlinks: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer.add_directory("apps/", options).unwrap();
    for (name, bytes) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    for (name, target) in symlinks {
        writer.add_symlink(*name, *target, options).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// The keys of a source carrying `bytes` inline, with their SHA-256.
fn inline_file(bytes: &[u8]) -> String {
    format!("inline: {}, sha256: {}", STANDARD.encode(bytes), sha(bytes))
}

/// The files a device mapping such as `{apps: …}` puts on the card.
#[test]
fn wifi_takes_its_passphrase_from_a_relative_file_or_text() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("wiiform-wifi-test-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("passphrase.txt"), "correct horse\n")?;
    fs::write(dir.join("short.txt"), "short\n")?;
    let wifi = |passphrase: &str| {
        format!(
            "nand:\n  menu: 4.3E\n  wifi: {{set: {{ssid: HomeNet, security: wpa2_aes, passphrase: {passphrase}}}}}"
        )
    };
    let profile_file = |yaml: String| -> Result<Vec<u8>> {
        let recipe = Recipe::read(yaml.as_bytes(), &dir, &store(), Collisions::default())?;
        recipe.nand.validate()?;
        match recipe.nand.wifi.context("expected a wifi section")? {
            crate::nand::wifi::Wifi::Set(profile) => profile.config(),
            crate::nand::wifi::Wifi::Keep => anyhow::bail!("expected set, not keep"),
        }
    };
    let from_file = profile_file(wifi("{file: passphrase.txt}"))?;
    assert_eq!(&from_file[2040..2053], b"correct horse");
    assert_eq!(profile_file(wifi("{text: correct horse}"))?, from_file);
    assert!(
        profile_file(wifi("{file: short.txt}")).is_err(),
        "a 5-character passphrase"
    );
    assert!(profile_file(wifi("{file: missing.txt}")).is_err());
    Ok(())
}

#[test]
fn wiimotes_need_setup_because_sysconf_carries_them() -> Result<()> {
    // Reading a recipe already checks it, so the refusal comes from `read` itself.
    let refusal = message(read("menu: 4.3E\nwiimotes: keep"));
    assert!(refusal.contains("wiimotes requires setup"), "{refusal}");
    read("menu: 4.3E\nsetup: {language: en, country: GB}\nwiimotes: keep")?;
    Ok(())
}

fn card(device: &str) -> Result<Vec<(String, Vec<u8>)>> {
    card_with(Collisions::default(), device)
}

fn card_with(collisions: Collisions, device: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let recipe = read_recipe_with(&format!("nand: {{menu: 4.3E}}\nsd: {device}"), collisions)?;
    (recipe.sd.iter())
        .flat_map(|sd| sd.files())
        .map(|(path, blob)| Ok((path.to_string(), blob.bytes()?)))
        .collect()
}

fn paths(files: &[(String, Vec<u8>)]) -> Vec<&str> {
    files.iter().map(|(path, _)| path.as_str()).collect()
}

#[test]
fn apps_find_their_folder_in_the_archive() -> Result<()> {
    let app =
        |name: &str, zip: &[u8]| card(&format!("{{apps: {{{name}: {{{}}}}}}}", inline_file(zip)));
    for (layout, files) in [
        (
            "apps/App/",
            zip_of(
                &[("apps/App/boot.dol", b"d"), ("apps/App/meta.xml", b"m")],
                &[],
            ),
        ),
        (
            "App/",
            zip_of(&[("App/boot.elf", b"d"), ("App/meta.xml", b"m")], &[]),
        ),
        (
            "root",
            zip_of(&[("BOOT.DOL", b"d"), ("meta.xml", b"m")], &[]),
        ),
        (
            "nested",
            zip_of(
                &[
                    ("x/y/App/boot.dol", b"d"),
                    ("x/y/App/meta.xml", b"m"),
                    ("__MACOSX/._x", b""),
                ],
                &[],
            ),
        ),
    ] {
        let placed = app("app", &files)?;
        assert_eq!(placed.len(), 2, "{layout}");
        assert!(
            placed.iter().any(|(path, _)| path == "/apps/app/meta.xml"),
            "{layout}"
        );
    }
    let one_folder_many_files = zip_of(
        &[
            ("a/boot.dol", b"d"),
            ("a/meta.xml", b"m"),
            ("a/boot.elf", b"e"),
        ],
        &[],
    );
    assert_eq!(app("app", &one_folder_many_files)?.len(), 3);

    for (zip, error) in [
        (
            zip_of(&[("readme.txt", b"r")], &[]),
            "no boot.dol or boot.elf",
        ),
        (
            zip_of(&[("a/boot.dol", b"d"), ("b/boot.dol", b"d")], &[]),
            "several app folders",
        ),
        (
            zip_of(&[("apps/a/boot.dol", b"d"), ("data/x", b"x")], &[]),
            "outside the app folder",
        ),
        (
            zip_of(&[], &[("apps/a/boot.dol", "/etc/passwd")]),
            "not a regular file",
        ),
        (b"not a zip".to_vec(), "zip archive"),
    ] {
        assert!(message(app("app", &zip)).contains(error), "{error}");
    }
    assert!(message(app("a/b", &one_folder_many_files)).contains("single folder name"));
    assert!(message(app("a:b", &one_folder_many_files)).contains("cannot store"));
    Ok(())
}

#[test]
fn apps_take_loose_files_or_a_chosen_folder() -> Result<()> {
    let loose = card(&format!(
        "{{apps: {{Nintendont: {{files: {{boot.dol: {{{}}}, meta.xml: {{{}}}}}}}}}}}",
        inline_file(b"dol"),
        inline_file(b"xml")
    ))?;
    assert_eq!(
        paths(&loose),
        ["/apps/Nintendont/boot.dol", "/apps/Nintendont/meta.xml"]
    );
    let no_boot = format!(
        "{{apps: {{x: {{files: {{meta.xml: {{{}}}}}}}}}}}",
        inline_file(b"xml")
    );
    assert!(message(card(&no_boot)).contains("gets no boot.dol or boot.elf"));
    let hashed = format!(
        "{{apps: {{x: {{files: {{boot.dol: {{{}}}}}, sha256: {}}}}}}}",
        inline_file(b"dol"),
        sha(b"dol")
    );
    assert!(message(card(&hashed)).contains("per file"));

    // Several app folders, as in GCMM's release: folder picks one, and the
    // rest of the archive is left out.
    let release = zip_of(
        &[
            ("gcmm_1.5.2/gc/gcmm.dol", b"gc"),
            ("gcmm_1.5.2/wii/gcmm/boot.dol", b"light"),
            ("gcmm_1.5.2/wii/gcmm-dark/boot.dol", b"dark"),
            ("gcmm_1.5.2/wii/gcmm-dark/lang/fr.lang", b"fr"),
            ("gcmm_1.5.2/readme.txt", b"read me"),
        ],
        &[],
    );
    let folder = |glob: &str| {
        card(&format!(
            "{{apps: {{gcmm: {{{}, folder: '{glob}'}}}}}}",
            inline_file(&release)
        ))
    };
    let dark = ["/apps/gcmm/boot.dol", "/apps/gcmm/lang/fr.lang"];
    assert_eq!(paths(&folder("*/wii/gcmm-dark")?), dark);
    assert_eq!(paths(&folder("*/wii/gcmm-dark/")?), dark, "a trailing /");
    assert_eq!(paths(&folder("**/gcmm-dark")?), dark);
    assert!(
        message(card(&format!(
            "{{apps: {{gcmm: {{{}}}}}}}",
            inline_file(&release)
        )))
        .contains("choose one with folder")
    );
    for (glob, error) in [
        (
            "*/wii/*",
            "matches several folders (gcmm_1.5.2/wii/gcmm, gcmm_1.5.2/wii/gcmm-dark)",
        ),
        ("wii/gcmm-dark", "matches no folder"),
        ("*/gc", "gets no boot.dol"),
        ("*/wii/gcmm-dark/lang", "gets no boot.dol"),
        ("/gcmm_1.5.2/wii/gcmm", "without a leading /"),
        ("[", ""),
    ] {
        assert!(message(folder(glob)).contains(error), "{glob}");
    }
    let loose_with_folder = format!(
        "{{apps: {{x: {{files: {{boot.dol: {{{}}}}}, folder: x}}}}}}",
        inline_file(b"dol")
    );
    assert!(message(card(&loose_with_folder)).contains("takes no sha256 or folder"));
    let extract = format!(
        "{{apps: {{x: {{{}, extract: {{'**': /}}}}}}}}",
        inline_file(&release)
    );
    assert!(
        message(card(&extract)).contains("unknown field `extract`"),
        "apps no longer take extract"
    );
    Ok(())
}

#[test]
fn bundles_unzip_extract_or_copy_from_the_root() -> Result<()> {
    let release = zip_of(
        &[
            ("RVLoader_v2.1/apps/RVLoader/boot.dol", b"rvl"),
            ("RVLoader_v2.1/apps/BBLoader/boot.dol", b"rvl"),
            ("RVLoader_v2.1/rvloader/themes/main/theme.xml", b"t"),
            ("RVLoader_v2.1/LCDDriver.bit", b"lcd"),
        ],
        &[],
    );
    let bundle = |placement: &str| {
        card(&format!(
            "{{bundles: [{{{}, {placement}}}]}}",
            inline_file(&release)
        ))
    };
    assert_eq!(
        paths(&bundle(
            "extract: {'RVLoader_v2.1/apps/RVLoader/**': /apps/RVLoader/, 'RVLoader_v2.1/rvloader/**': /rvloader/, 'RVLoader_v2.1/LCDDriver.bit': /}"
        )?),
        [
            "/LCDDriver.bit",
            "/apps/RVLoader/boot.dol",
            "/rvloader/themes/main/theme.xml"
        ]
    );
    assert_eq!(bundle("unzip: /extra")?.len(), 4);
    // A versioned top folder can be a wildcard: everything before ** is dropped.
    assert_eq!(
        paths(&bundle(
            "extract: {'*/apps/RVLoader/**': /apps/RVLoader/, '*/*.bit': /}"
        )?),
        ["/LCDDriver.bit", "/apps/RVLoader/boot.dol"]
    );
    assert_eq!(
        paths(&bundle("extract: {'**/*.xml': /x/}")?),
        ["/x/RVLoader_v2.1/rvloader/themes/main/theme.xml"]
    );
    for (placement, error) in [
        ("extract: {'nothing/**': /}", "matches no file"),
        ("extract: {'**': /a/, '**/*.bit': /b/}", "matches both"),
        ("extract: {}", "at least one glob"),
        ("extract: {'[': /}", ""),
        ("unzip: /, to: x", ""),
        ("to: /", "not the root"),
        ("unzip: /, extra: 1", "unknown field"),
    ] {
        assert!(message(bundle(placement)).contains(error), "{placement}");
    }
    let copy = card(&format!(
        "{{bundles: [{{{}, to: a/b}}]}}",
        inline_file(b"x")
    ))?;
    assert_eq!(paths(&copy), ["/a/b"]);
    Ok(())
}

#[test]
fn a_failing_bundle_is_named_by_its_source() {
    let error = |bundle: &str| message(card(&format!("{{bundles: [{{{bundle}}}]}}")));
    assert!(error("path: ./missing.cfg, to: a").contains("bundle path ./missing.cfg: "));
    assert!(
        error("url: 'https://example.invalid/x.zip', unzip: /")
            .contains("bundle url https://example.invalid/x.zip: sha256 is required for a url"),
    );
    // A long inline file shows the start of its base64, as written in the recipe.
    assert!(
        error(&format!("{}, to: /", inline_file(b"a long inline file")))
            .contains("bundle inline YSBsb25nIGlu…: "),
    );
}

#[test]
fn device_files_must_not_collide() -> Result<()> {
    let to = |bytes: &[u8], path: &str| format!("{{{}, to: '{path}'}}", inline_file(bytes));
    let bundles = |entries: &[String]| {
        card_with(
            Collisions::Error,
            &format!("{{bundles: [{}]}}", entries.join(", ")),
        )
    };
    assert!(message(bundles(&[to(b"one", "a/b"), to(b"two", "A/B")])).contains("different files"));
    bundles(&[to(b"same", "a/b"), to(b"same", "a/b")])?;
    assert!(
        message(bundles(&[to(b"one", "a"), to(b"two", "a/b")]))
            .contains("both a file and a directory")
    );
    let app_and_bundle = format!(
        "{{apps: {{x: {{files: {{boot.dol: {{{}}}}}}}}}, bundles: [{}]}}",
        inline_file(b"dol"),
        to(b"other", "apps/X/BOOT.DOL")
    );
    assert!(message(card_with(Collisions::Error, &app_and_bundle)).contains("different files"));
    for reserved in ["nand.bin", "/KEYS.BIN"] {
        assert!(message(bundles(&[to(b"x", reserved)])).contains("NAND and its keys"));
    }
    bundles(&[to(b"x", "backup/nand.bin")])?;
    // usb has no NAND at its root.
    let usb = read_recipe(&format!(
        "nand: {{menu: 4.3E}}\nusb: {{bundles: [{}]}}",
        to(b"x", "nand.bin")
    ))?;
    assert_eq!(usb.usb.context("expected usb")?.files().len(), 1);
    Ok(())
}

#[test]
fn github_sources_use_the_release_digest_or_a_given_hash() -> Result<()> {
    const ASSET: &[u8] = b"release asset";
    let entry = |sha256: &str| {
        format!(
            "{{bundles: [{{github: {{repo: owner/app, tag: v1.0, asset: app.dol}}, {sha256} to: apps/app/boot.dol}}]}}"
        )
    };
    let read = |entry: String, digest: Option<String>| {
        with_fetch(
            move |url, _| match url {
                "https://api.github.com/repos/owner/app/releases/tags/v1.0" => {
                    Ok(serde_json::to_vec(
                        &serde_json::json!({"assets": [{"name": "app.dol", "digest": digest}]}),
                    )?)
                }
                "https://github.com/owner/app/releases/download/v1.0/app.dol" => Ok(ASSET.to_vec()),
                _ => panic!("unexpected download of {url}"),
            },
            || card(&entry),
        )
    };
    let published = Some(format!("sha256:{}", sha(ASSET)));
    assert_eq!(read(entry(""), published.clone())?[0].1, ASSET);
    assert_eq!(
        read(entry(&format!("sha256: {},", sha(ASSET))), None)?[0].1,
        ASSET
    );
    assert!(message(read(entry(""), None)).contains("sha256 is required"));
    let wrong = format!("sha256: {},", sha(b"other"));
    assert!(message(read(entry(&wrong), published)).contains("SHA-256"));
    let bad_repo =
        "{bundles: [{github: {repo: owner, tag: v1.0, asset: app.dol}, to: a}]}".to_owned();
    assert!(message(read(bad_repo, None)).contains("owner/name"));
    Ok(())
}

#[test]
fn exclusive_hacks_each_have_one_key_taking_one_value() -> Result<()> {
    // The hacks that exclude each other are not list entries: each group is a key.
    for hacks in [
        "[no_background_music]",
        "[usb_gecko_report]",
        "[force_disc_ios249]",
    ] {
        let result = read(&format!("menu: 4.3E\npriiloader: {{hacks: {hacks}}}"));
        assert!(message(result).contains("unknown variant"), "{hacks}");
    }
    for setting in [
        "background_music: [no_background_music, no_sfx_and_background_music]",
        "usb_gecko: [usb_gecko_report, usb_gecko_report_geckoos]",
        "force_disc_ios: [force_disc_ios249, force_disc_ios250]",
    ] {
        let result = read(&format!("menu: 4.3E\npriiloader: {{{setting}}}"));
        assert!(result.is_err(), "{setting}");
    }
    let config = read(
        "menu: 4.3E\npriiloader: {hacks: [remove_ios16_disc_error, fix_480p], background_music: no_sfx_and_background_music, usb_gecko: usb_gecko_report_geckoos, force_disc_ios: force_disc_ios249}",
    )?
    .priiloader
    .context("expected Priiloader settings")?;
    assert_eq!(config.hacks, [Hack::RemoveIos16DiscError, Hack::Fix480p]);
    assert_eq!(
        config.background_music,
        Some(BackgroundMusic::NoSfxAndBackgroundMusic)
    );
    assert_eq!(config.usb_gecko, Some(UsbGecko::UsbGeckoReportGeckoOs));
    assert_eq!(config.force_disc_ios, Some(DiscIos::ForceDiscIos249));
    Ok(())
}

#[test]
fn a_flat_recipe_is_refused() {
    assert!(message(read_recipe("menu: 4.3E")).contains("unknown field `menu`"));
}

#[test]
fn ios_come_from_nus_by_slot_and_version() -> Result<()> {
    let titles = read("menu: 4.3E\ntitles: {ios: [{slot: 9, version: 1034}]}")?.titles;
    assert_eq!(titles.ios[0].title_id().as_u64(), 0x0000_0001_0000_0009);
    assert_eq!(titles.ios[0].version, 1034);
    for ios in [
        "{slot: 2, version: 1}",
        "{slot: 254, version: 1}",
        "{slot: 9}",
        "{slot: 9, version: 1, sha256: x}",
    ] {
        assert!(
            read(&format!("menu: 4.3E\ntitles: {{ios: [{ios}]}}")).is_err(),
            "{ios}"
        );
    }
    let cios = format!(
        "{{{}, target: {{slot: 249, base: 56, revision: 1}}}}",
        inline()
    );
    let both = read(&format!(
        "menu: 4.3E\ntitles: {{ios: [{{slot: 249, version: 1}}], cios: [{cios}]}}"
    ));
    assert!(message(both).contains("IOS slot 249 twice"));
    assert!(
        read("menu: 4.3E\ntitles: [{path: a.wad}]").is_err(),
        "the old list is refused"
    );
    assert!(
        read("menu: 4.3E\ncios: []").is_err(),
        "cios moved under titles"
    );
    Ok(())
}

/// A d2x release holding `map` and the modules named in `modules`.
fn d2x_release(map: &str, modules: &[&str]) -> Vec<u8> {
    let files: Vec<(String, Vec<u8>)> =
        std::iter::once(("apps/d2x/ciosmaps.xml".to_owned(), map.as_bytes().to_vec()))
            .chain(modules.iter().map(|name| {
                (
                    format!("apps/d2x/d2x-test/{name}.app"),
                    name.as_bytes().to_vec(),
                )
            }))
            .collect();
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    zip_of(&refs, &[])
}

const MAP: &str = r#"<ciosmaps ciosgroupscount="1"><ciosgroup name="d2x-test" version="1" basescount="2">
  <base ios="56" version="5661"><content id="0x1f"><patch offset="0x4" size="2" originalbytes="0x01,0x02" newbytes="0xaa,0xbb"/></content>
    <content id="0x10"/><content id="0x25" module="EHCI" tmdmoduleid="3"/><content id="0x26" module="ES" tmdmoduleid="-1"/></base>
  <base ios="57" version="5918"/></ciosgroup></ciosmaps>"#;

#[test]
fn d2x_maps_become_builds_for_the_chosen_base() -> Result<()> {
    use crate::nand::titles::cios::d2x;
    let build = d2x::build(&d2x_release(MAP, &["EHCI", "ES"]), 56)?;
    assert_eq!(build.base_version, 5661);
    assert_eq!(build.patches.len(), 1);
    assert_eq!((build.patches[0].cid, build.patches[0].offset), (0x1f, 4));
    assert_eq!(
        (&*build.patches[0].original, &*build.patches[0].replacement),
        (&[1, 2][..], &[0xaa, 0xbb][..])
    );
    assert_eq!(
        build.modules,
        [
            Module {
                cid: 0x25,
                position: Some(3),
                bytes: b"EHCI".to_vec()
            },
            Module {
                cid: 0x26,
                position: None,
                bytes: b"ES".to_vec()
            },
        ]
    );
    assert_eq!(
        d2x::build(&d2x_release(MAP, &["EHCI", "ES"]), 57)?.modules,
        []
    );
    for (release, base, error) in [
        (d2x_release(MAP, &["EHCI", "ES"]), 38, "supports 56, 57"),
        (
            d2x_release(MAP, &["EHCI"]),
            56,
            "lacks apps/d2x/d2x-test/ES.app",
        ),
        (
            d2x_release(
                &MAP.replace("</ciosgroup>", "</ciosgroup><ciosgroup name=\"b\"/>"),
                &[],
            ),
            56,
            "2 cIOS groups",
        ),
        (
            d2x_release(&MAP.replace("size=\"2\"", "size=\"3\""), &["EHCI", "ES"]),
            56,
            "does not match its size",
        ),
        (zip_of(&[("readme.txt", b"x")], &[]), 56, "one ciosmaps.xml"),
    ] {
        assert!(
            message(d2x::build(&release, base)).contains(error),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn cios_entries_are_prebuilt_or_built_from_d2x() -> Result<()> {
    let release = d2x_release(MAP, &["EHCI", "ES"]);
    let d2x = format!("d2x: {{{}}}", inline_file(&release));
    let target = "target: {slot: 249, base: 56, revision: 65535}";
    let cios = |fields: &str| read(&format!("menu: 4.3E\ntitles: {{cios: [{{{fields}}}]}}"));
    let built = cios(&format!("{d2x}, {target}"))?;
    assert!(
        matches!(&built.titles.cios[0].kind, CiosKind::Built(build) if build.base_version == 5661)
    );
    for (fields, error) in [
        (
            format!("{d2x}, sha256: {}, {target}", sha(&release)),
            "inside `d2x`",
        ),
        (
            format!("url: 'https://example.com/cios.wad', {target}"),
            "sha256 is required for a url",
        ),
        (format!("{d2x}, {}, {target}", inline()), ""),
        (format!("{d2x}, {target}, extra: 1"), "unknown field"),
    ] {
        assert!(message(cios(&fields)).contains(error), "{fields}");
    }
    Ok(())
}

#[test]
fn bootmii_is_an_empty_mapping_that_priiloader_autoboot_requires() -> Result<()> {
    assert!(read("menu: 4.3E\nbootmii: {}")?.bootmii.is_some());
    assert!(read("menu: 4.3E")?.bootmii.is_none());
    assert!(read("menu: 4.3E\nbootmii: {extra: 1}").is_err());
    let autoboot = "menu: 4.3E\npriiloader: {autoboot: bootmii_ios}";
    assert!(message(read(autoboot)).contains("requires the bootmii key"));
    read(&format!("{autoboot}\nbootmii: {{}}"))?;
    Ok(())
}

#[test]
fn a_device_download_is_stored_by_digest_and_fetched_once() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static FETCHES: AtomicUsize = AtomicUsize::new(0);
    let payload = vec![0x5a; 0x3001];
    let device = format!(
        "{{bundles: [{{url: https://example.com/pkg.bin, sha256: {}, to: a/pkg.bin}}]}}",
        sha(&payload)
    );
    let fetch = {
        let payload = payload.clone();
        move |url: &str, _: usize| {
            assert_eq!(url, "https://example.com/pkg.bin");
            FETCHES.fetch_add(1, Ordering::SeqCst);
            Ok(payload.clone())
        }
    };
    let first = with_fetch(fetch.clone(), || card(&device))?;
    assert_eq!(first, [("/a/pkg.bin".to_owned(), payload.clone())]);
    let second = with_fetch(fetch, || card(&device))?;
    assert_eq!(second, first);
    assert_eq!(
        FETCHES.load(Ordering::SeqCst),
        1,
        "the stored copy is reused"
    );
    Ok(())
}

#[test]
fn a_download_that_does_not_match_is_not_stored() -> Result<()> {
    let expected = vec![0x6b; 0x3002];
    let device = format!(
        "{{bundles: [{{url: https://example.com/pkg.bin, sha256: {}, to: a/pkg.bin}}]}}",
        sha(&expected)
    );
    let error = with_fetch(|_, _| Ok(vec![0x00; 0x3002]), || card(&device))
        .expect_err("a different download must be refused");
    assert!(format!("{error:#}").contains("does not match its SHA-256"));
    assert!(
        !store().join(sha(&expected)).exists(),
        "nothing unverified is kept"
    );
    Ok(())
}

#[test]
fn device_files_larger_than_fat32_allows_are_refused() -> Result<()> {
    use crate::blob::Blob;
    let path = |name: &str| crate::device::CardPath::try_from(name.to_owned());
    let large = Blob::File {
        path: std::path::PathBuf::from("unused"),
        size: 0x1_0000_0000,
    };
    let small = Blob::File {
        path: std::path::PathBuf::from("unused"),
        size: 0xffff_ffff,
    };
    let fits = [(path("a/small")?, small.clone())].into_iter().collect();
    device::check_fat32(&fits)?;
    let too_big = [(path("a/large")?, large)].into_iter().collect();
    let error = device::check_fat32(&too_big).expect_err("4 GiB and more is refused");
    assert!(format!("{error:#}").contains("a/large"));
    Ok(())
}

#[test]
fn collisions_are_settled_in_recipe_order_by_the_flag() -> Result<()> {
    let to = |bytes: &[u8], path: &str| format!("{{{}, to: '{path}'}}", inline_file(bytes));
    let pair = |first: &[u8], second: &[u8]| {
        format!("{{bundles: [{}, {}]}}", to(first, "a/b"), to(second, "A/B"))
    };
    // Overwrite is the default: the later entry wins, under the later spelling.
    assert_eq!(
        card(&pair(b"one", b"two"))?,
        [("/A/B".to_owned(), b"two".to_vec())]
    );
    assert_eq!(
        card_with(Collisions::Overwrite, &pair(b"one", b"two"))?,
        [("/A/B".to_owned(), b"two".to_vec())]
    );
    // Keep-first: the earlier entry stays, whichever order the files come in.
    assert_eq!(
        card_with(Collisions::KeepFirst, &pair(b"one", b"two"))?,
        [("/a/b".to_owned(), b"one".to_vec())]
    );
    assert_eq!(
        card_with(Collisions::KeepFirst, &pair(b"two", b"one"))?,
        [("/a/b".to_owned(), b"two".to_vec())]
    );
    // Error: identical copies are fine, different ones stop the build.
    assert_eq!(
        card_with(Collisions::Error, &pair(b"same", b"same"))?,
        [("/a/b".to_owned(), b"same".to_vec())]
    );
    assert!(
        message(card_with(Collisions::Error, &pair(b"one", b"two"))).contains("different files")
    );
    Ok(())
}

#[test]
fn entries_are_downloaded_in_recipe_order() -> Result<()> {
    use std::sync::{Arc, Mutex};
    // A download's bytes are its name, so each digest is known up front. Names
    // carry the prefix because the store is shared by the tests of a process.
    let name_of = |url: &str| {
        url.trim_start_matches("https://example.com/")
            .trim_end_matches(".bin")
            .to_owned()
    };
    let device = |prefix: &str, apps_first: bool| {
        let url = |name: &str| format!("https://example.com/{prefix}{name}.bin");
        let bundle = |name: &str| {
            format!(
                "{{url: {}, sha256: {}, to: '{name}.bin'}}",
                url(name),
                sha(format!("{prefix}{name}").as_bytes())
            )
        };
        let app = format!(
            "{{files: {{boot.dol: {{url: {}, sha256: {}}}}}}}",
            url("dol"),
            sha(format!("{prefix}dol").as_bytes())
        );
        let bundles = format!("bundles: [{}, {}]", bundle("first"), bundle("second"));
        let apps = format!("apps: {{alpha: {app}}}");
        match apps_first {
            true => format!("{{{apps}, {bundles}}}"),
            false => format!("{{{bundles}, {apps}}}"),
        }
    };
    let downloads = |device: &str| -> Result<Vec<String>> {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        with_fetch(
            move |requested: &str, _| {
                seen.lock().unwrap().push(name_of(requested));
                Ok(name_of(requested).into_bytes())
            },
            || card(device),
        )?;
        let order = requests.lock().unwrap().clone();
        Ok(order)
    };
    assert_eq!(
        downloads(&device("bundles-", false))?,
        ["bundles-first", "bundles-second", "bundles-dol"]
    );
    assert_eq!(
        downloads(&device("apps-", true))?,
        ["apps-dol", "apps-first", "apps-second"]
    );
    Ok(())
}

#[test]
fn each_colliding_file_gets_one_notice_naming_its_sources() -> Result<()> {
    let to = |bytes: &[u8], path: &str| format!("{{{}, to: '{path}'}}", inline_file(bytes));
    let notices = |collisions: Collisions, entries: &[String]| -> Result<Vec<String>> {
        let yaml = format!(
            "nand: {{menu: 4.3E}}\nsd: {{bundles: [{}]}}",
            entries.join(", ")
        );
        Ok(read_recipe_with(&yaml, collisions)?.collision_notices())
    };
    let entries = [
        to(b"one", "a/b"),
        to(b"two", "A/B"),
        to(b"three", "a/B"),
        to(b"four", "a/c"),
    ];
    // One notice for the file, however many entries wrote it: the last one wins.
    assert_eq!(
        notices(Collisions::Overwrite, &entries)?,
        ["sd: /a/B: from bundle inline dGhyZWU= replaces bundle inline b25l, bundle inline dHdv"]
    );
    assert_eq!(
        notices(Collisions::KeepFirst, &entries)?,
        [
            "sd: /a/b: kept from bundle inline b25l; bundle inline dHdv, bundle inline dGhyZWU= ignored"
        ]
    );
    // Error only reports what it refuses; identical copies say nothing.
    assert!(notices(Collisions::Error, &entries).is_err());
    assert!(notices(Collisions::Error, &[to(b"same", "x"), to(b"same", "x")])?.is_empty());
    assert!(notices(Collisions::Overwrite, &[to(b"x", "p")])?.is_empty());
    Ok(())
}

/// The files a `usb` mapping such as `{rvloader: …}` puts on the drive.
fn usb(device: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let recipe = read_recipe(&format!("nand: {{menu: 4.3E}}\nusb: {device}"))?;
    (recipe.usb.iter())
        .flat_map(|usb| usb.files())
        .map(|(path, blob)| Ok((path.to_string(), blob.bytes()?)))
        .collect()
}

/// The contents of the file at `path`, which must be among `files`.
fn contents<'a>(files: &'a [(String, Vec<u8>)], path: &str) -> &'a [u8] {
    files
        .iter()
        .find(|(name, _)| name == path)
        .map(|(_, bytes)| bytes.as_slice())
        .unwrap_or_else(|| panic!("{path} is not on the device"))
}

#[test]
fn bootmii_ini_holds_the_keys_it_is_given() -> Result<()> {
    let files = card("{bootmii: {video: ntsc, autoboot: hbc, boot_delay: 5}}")?;
    assert_eq!(
        contents(&files, "/bootmii/bootmii.ini"),
        b"VIDEO=NTSC\nAUTOBOOT=HBC\nBOOTDELAY=5\n"
    );
    let files = card("{bootmii: {video: progressive}}")?;
    assert_eq!(
        contents(&files, "/bootmii/bootmii.ini"),
        b"VIDEO=PROGRESSIVE\n"
    );
    Ok(())
}

#[test]
fn bootmii_needs_a_key_and_the_sd_card() {
    let refused = |device: &str| message(read_recipe(&format!("nand: {{menu: 4.3E}}\n{device}")));
    assert!(refused("sd: {bootmii: {}}").contains("sets no key"));
    // The usb mapping has no bootmii key at all.
    assert!(refused("usb: {bootmii: {video: ntsc}}").contains("unknown field `bootmii`"));
}

#[test]
fn rvloader_writes_only_the_settings_given_per_game() -> Result<()> {
    let files =
        usb("{rvloader: {games: {RHAP01: {bluetooth: true, usb_saves: false, wifi: auto}}}}")?;
    assert_eq!(
        contents(&files, "/rvloader/configs/RHAP01.cfg"),
        b"Enable_WiFi = 2\nEnable_Bluetooth = 1\nEnable_USB_saves = 0\n"
    );
    Ok(())
}

#[test]
fn rvloader_values_are_the_numbers_the_loader_reads() -> Result<()> {
    let files = usb(
        "{rvloader: {games: {RHAP01: {wifi: off, padread_mode: redirect, deflicker_mode: on_high, cheats_hooktype: axnextframe}}}}",
    )?;
    assert_eq!(
        contents(&files, "/rvloader/configs/RHAP01.cfg"),
        b"Enable_WiFi = 0\nPADRead_Mode = 2\nDeflicker_Mode = 5\nCheats_Hooktype = 7\n"
    );
    let files = usb(
        "{rvloader: {games: {GALE01: {video_mode: ntsc, language: auto, max_pads: 2, video_width: 640}}}}",
    )?;
    assert_eq!(
        contents(&files, "/rvloader/configs/GALE01.cfg"),
        b"Video_mode = 65540\nLanguage = -1\nMax_Pads = 2\nVideo_Width = 640\n"
    );
    Ok(())
}

#[test]
fn rvloader_refuses_bad_ids_settings_and_the_sd_card() {
    let refused = |device: &str| message(read_recipe(&format!("nand: {{menu: 4.3E}}\n{device}")));
    assert!(
        refused("usb: {rvloader: {games: {rhap01: {bluetooth: true}}}}")
            .contains("is not a game ID")
    );
    assert!(
        refused("usb: {rvloader: {games: {ABC: {bluetooth: true}}}}").contains("is not a game ID")
    );
    assert!(refused("usb: {rvloader: {games: {}}}").contains("lists no games"));
    assert!(refused("usb: {rvloader: {games: {RHAP01: {}}}}").contains("sets no key"));
    assert!(
        refused("usb: {rvloader: {games: {RHAP01: {bluetoth: true}}}}").contains("unknown field")
    );
    assert!(
        refused("usb: {rvloader: {games: {GALE01: {max_pads: 5}}}}").contains("max_pads is 1 to 4")
    );
    assert!(
        refused("usb: {rvloader: {games: {GALE01: {video_width: 800}}}}")
            .contains("video_width is 0")
    );
    // The sd mapping has no rvloader key at all.
    assert!(
        refused("sd: {rvloader: {games: {RHAP01: {bluetooth: true}}}}")
            .contains("unknown field `rvloader`")
    );
    // The YAML reader refuses a repeated key before the recipe is read.
    assert!(
        refused(
            "usb: {rvloader: {games: {RHAP01: {bluetooth: true}, RHAP01: {bluetooth: false}}}}"
        )
        .contains("duplicate mapping key")
    );
}

#[test]
fn generated_settings_collide_like_any_device_file() -> Result<()> {
    let recipe = read_recipe(&format!(
        "nand: {{menu: 4.3E}}\nusb:\n  rvloader: {{games: {{RHAP01: {{bluetooth: true}}}}}}\n  bundles:\n    - {{{}, to: /rvloader/configs/RHAP01.cfg}}\n",
        inline_file(b"own settings")
    ))?;
    let usb = recipe.usb.expect("usb is set");
    assert_eq!(usb.notices().len(), 1, "{:?}", usb.notices());
    assert!(usb.notices()[0].contains("/rvloader/configs/RHAP01.cfg"));
    assert!(usb.notices()[0].contains("bundle inline b3duIHNldHRp…"));
    assert_eq!(usb.files().len(), 1);
    Ok(())
}

#[test]
fn rvloader_config_writes_only_the_keys_given() -> Result<()> {
    let theme = inline_file(b"theme");
    let files = usb(&format!(
        "{{rvloader: {{config: {{theme: main, wii_load_screen: spinning_disc}}}}, bundles: [{{{theme}, to: /rvloader/themes/main/theme.txt}}]}}"
    ))?;
    assert_eq!(
        contents(&files, "/rvloader/config.cfg"),
        b"theme = \"main\"\nWiiLoadScreen = 1\n"
    );
    let files = usb(
        "{rvloader: {config: {wii_load_screen: verbose}, games: {RHAP01: {bluetooth: true}}}}",
    )?;
    assert_eq!(
        contents(&files, "/rvloader/config.cfg"),
        b"WiiLoadScreen = 0\n"
    );
    assert_eq!(
        contents(&files, "/rvloader/configs/RHAP01.cfg"),
        b"Enable_Bluetooth = 1\n"
    );
    Ok(())
}

#[test]
fn rvloader_config_needs_its_theme_on_the_drive_and_a_valid_name() {
    let refused = |device: &str| message(read_recipe(&format!("nand: {{menu: 4.3E}}\n{device}")));
    assert!(refused("usb: {rvloader: {config: {theme: main}}}").contains("holds no files"));
    assert!(
        refused("usb: {rvloader: {config: {theme: \"../x\"}}}").contains("not a theme folder name")
    );
    assert!(refused("usb: {rvloader: {config: {}}}").contains("sets no key"));
    assert!(refused("usb: {rvloader: {}}").contains("neither config nor games"));
    assert!(refused("usb: {rvloader: {games: {}}}").contains("lists no games"));
}
