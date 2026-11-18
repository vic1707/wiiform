// SPDX-License-Identifier: GPL-2.0-only
use std::fs;

use base64::{Engine, engine::general_purpose::STANDARD};

use super::*;
use crate::{
    nand::{
        menu::Menu,
        priiloader::{Autoboot, Hack, ReturnTo},
        setup::display::Aspect,
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
    Recipe::read(yaml.as_bytes(), Path::new("."))
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
        format!("inline: {encoded}"),
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

    let recipe = Recipe::read(nested(&title("linked.wad")).as_bytes(), &dir)?.nand;
    assert_eq!(&*recipe.titles.channels[0].wad, WAD);
    for bad in [".", "missing.wad"] {
        assert!(
            Recipe::read(nested(&title(bad)).as_bytes(), &dir).is_err(),
            "{bad}"
        );
    }
    assert!(read(&title("linked.wad")).is_err(), "another directory");
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
        "menu: 4.3E\nsetup: {country: '110'}",
        "menu: 4.3E\nsetup: {language: ja}",
        "menu: 4.3J\nsetup: {country: GB}",
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
    assert_eq!(read("menu: 4.3E\nsetup: {}")?.setup, Some(Setup::default()));

    let recipe = read(
        "menu: 4.3E\nsetup: {nickname: 'Éééééééééé', display: {aspect: '16:9', progressive: true}}",
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

#[test]
fn rvloader_requires_priiloader_and_owns_its_boot_settings() -> Result<()> {
    const DOL: [u8; 3] = [1, 2, 3];
    const WD: [u8; 2] = [4, 5];
    let rvloader = |version: &str, sha256: &str, options: &str| {
        format!(
            "rvloader: {{version: '{version}', sha256: {sha256}, wd_sha256: {}, {options}}}",
            sha(&WD)
        )
    };
    let read = |yaml: String| {
        with_fetch(
            |url, _| {
                let base =
                    "https://raw.githubusercontent.com/Aurelio92/RVLoader/v2.1/installer/data/";
                Ok(match url.strip_prefix(base) {
                    Some("bootloader.dol") => DOL.to_vec(),
                    Some("wd.bin") => WD.to_vec(),
                    _ => panic!("unexpected download of {url}"),
                })
            },
            || read(&format!("menu: 4.3E\n{yaml}")),
        )
    };
    let options = "patch_out_wifi: true, vga: false";
    let good = rvloader("2.1", &sha(&DOL), options);

    let recipe = read(format!("{good}\npriiloader: {{stop_disc: true}}"))?;
    let loaded = recipe.rvloader.as_ref().context("expected RVLoader")?;
    assert_eq!(
        (&*loaded.bootloader, &*loaded.wifi_module),
        (&DOL[..], &WD[..])
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
        rvloader("2.1", &sha(&DOL), "patch_out_wifi: false, vga: false")
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
    let vga = rvloader("2.1", &sha(&DOL), "patch_out_wifi: true, vga: true");
    for (setup, error) in [
        ("", "requires setup.display.progressive"),
        (
            "setup: {display: {pal60: true}}",
            "requires setup.display.progressive",
        ),
        (
            "setup: {display: {progressive: true}}",
            "requires setup.display.pal60",
        ),
    ] {
        let result = read(format!("{vga}\npriiloader: {{}}\n{setup}"));
        assert!(message(result).contains(error), "{setup}");
    }
    read(format!(
        "{vga}\npriiloader: {{}}\nsetup: {{display: {{progressive: true, pal60: true}}}}"
    ))?;

    let wrong = rvloader("2.1", &sha(&WAD), options);
    assert!(message(read(wrong)).contains("bootloader.dol does not match its SHA-256"));
    for version in ["2", "2.1.0", "v2.1", "2.x", "../2.1"] {
        assert!(
            read(rvloader(version, &sha(&DOL), options)).is_err(),
            "{version}"
        );
    }
    for missing in ["patch_out_wifi: true", "vga: false"] {
        assert!(
            read(rvloader("2.1", &sha(&DOL), missing)).is_err(),
            "{missing}"
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
fn card(device: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let recipe = read_recipe(&format!("nand: {{menu: 4.3E}}\nsd: {device}"))?;
    Ok((recipe.sd.iter())
        .flat_map(|sd| sd.files())
        .map(|(path, bytes)| (path.to_string(), bytes.clone()))
        .collect())
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
fn apps_take_loose_files_or_an_extract_mapping() -> Result<()> {
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

    // Two app folders: extract picks one and leaves the rest out.
    let release = zip_of(
        &[
            ("pkg/apps/Main/boot.dol", b"main"),
            ("pkg/apps/Main/lang/fr.lang", b"fr"),
            ("pkg/apps/Old/boot.dol", b"old"),
        ],
        &[],
    );
    let extract = |pairs: &str| {
        card(&format!(
            "{{apps: {{Main: {{{}, extract: {{{pairs}}}}}}}}}",
            inline_file(&release)
        ))
    };
    assert_eq!(
        paths(&extract("'pkg/apps/Main/**': /")?),
        ["/apps/Main/boot.dol", "/apps/Main/lang/fr.lang"]
    );
    assert!(
        message(extract("'pkg/apps/Main/lang/*': /lang/")).contains("gets no boot.dol"),
        "the result needs a boot file"
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
fn device_files_must_not_collide() -> Result<()> {
    let to = |bytes: &[u8], path: &str| format!("{{{}, to: '{path}'}}", inline_file(bytes));
    let bundles = |entries: &[String]| card(&format!("{{bundles: [{}]}}", entries.join(", ")));
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
    assert!(message(card(&app_and_bundle)).contains("different files"));
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
fn exclusive_hacks_are_refused() -> Result<()> {
    for hacks in [
        "[no_background_music, no_sfx_and_background_music]",
        "[usb_gecko_report, usb_gecko_report_geckoos]",
        "[force_disc_ios249, force_disc_ios250]",
    ] {
        let result = read(&format!("menu: 4.3E\npriiloader: {{hacks: {hacks}}}"));
        assert!(
            message(result).contains("cannot be enabled together"),
            "{hacks}"
        );
    }
    read(
        "menu: 4.3E\npriiloader: {hacks: [force_disc_ios249, no_sfx_and_background_music, remove_ios16_disc_error]}",
    )?;
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
            format!("inline: {}, {target}", STANDARD.encode(WAD)),
            "needs sha256",
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
