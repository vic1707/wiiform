// SPDX-License-Identifier: GPL-2.0-only
use std::fs;

use base64::{Engine, engine::general_purpose::STANDARD};

use super::*;
use crate::{
    priiloader::{Autoboot, Hack},
    setup::Aspect,
    wad::tests::with_fetch,
};

const WAD: [u8; 0x40] = [7; 0x40];

fn read(yaml: &str) -> Result<Recipe> {
    Recipe::read(yaml.as_bytes(), Path::new("."))
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::of(bytes).0)
}

/// The keys of a WAD entry carrying `WAD` inline.
fn inline() -> String {
    format!("inline: {}, sha256: {}", STANDARD.encode(WAD), sha(&WAD))
}

fn message(result: Result<Recipe>) -> String {
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
    let title = |fields: &str| read(&format!("menu: 4.3E\ntitles:\n  - {fields}\n"));
    let encoded = STANDARD.encode(WAD);
    let digest = sha(&WAD);
    let (first, second) = encoded.split_at(40);
    let block = format!("sha256: {digest}\n    inline: |\n      {first}\n      {second}");
    assert_eq!(&*title(&block)?.titles[0].wad, WAD);

    let wrong = sha(b"something else");
    let small = STANDARD.encode([7; 0x3f]);
    assert!(message(title(&format!("inline: {encoded}\n    sha256: {wrong}"))).contains("SHA-256"));
    assert!(
        message(title(&format!(
            "inline: {small}\n    sha256: {}",
            sha(&[7; 0x3f])
        )))
        .contains("size")
    );
    for bad in [
        format!("inline: {encoded}\n    path: a.wad\n    sha256: {digest}"),
        format!("inline: {encoded}\n    url: https://example.com/a.wad\n    sha256: {digest}"),
        format!("sha256: {digest}"),
        format!("inline: {encoded}"),
        format!("inline: '***'\n    sha256: {digest}"),
        format!("inline: !!binary {encoded}\n    sha256: {digest}"),
        format!("path: null\n    sha256: {digest}"),
        format!("inline: {encoded}\n    sha256: {digest}\n    unexpected: true"),
    ] {
        assert!(title(&bad).is_err(), "{bad}");
    }
    Ok(())
}

#[test]
fn a_url_source_is_fetched_within_the_wad_size_limit() -> Result<()> {
    const URL: &str = "https://example.com/package.wad";
    let yaml = format!(
        "menu: 4.3E\ntitles: [{{url: {URL}, sha256: {}}}]",
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
    assert_eq!(&*recipe.titles[0].wad, WAD);
    assert!(with_fetch(|_, _| Ok(vec![]), || read(&yaml)).is_err());
    assert!(with_fetch(|_, _| anyhow::bail!("offline"), || read(&yaml)).is_err());
    Ok(())
}

#[test]
fn titles_and_cios_accept_only_their_own_fields() -> Result<()> {
    let target = "target: {slot: 249, base: 56, revision: 65535}";
    let wad = inline();
    let titles = |fields: &str| read(&format!("menu: 4.3E\ntitles: [{{{fields}}}]"));
    let cios = |fields: &str| read(&format!("menu: 4.3E\ncios: [{{{fields}}}]"));

    assert!(!titles(&wad)?.titles[0].allow_unsigned);
    assert!(titles(&format!("{wad}, allow_unsigned: true"))?.titles[0].allow_unsigned);
    let recipe = cios(&format!("{wad}, {target}"))?;
    assert_eq!(
        Some(recipe.cios[0].target.title_id()),
        TitleId::new(0x0000_0001_0000_00f9)
    );
    assert_eq!(&*recipe.cios[0].wad, WAD);

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
    let recipe_path = dir.join("recipe.yaml");
    let digest = sha(&WAD);
    let title = |path: &str| format!("menu: 4.3E\ntitles: [{{path: {path}, sha256: {digest}}}]");

    fs::write(&recipe_path, title("linked.wad"))?;
    let recipe = Recipe::try_from(&ResolvedPath::try_from(recipe_path)?)?;
    assert_eq!(&*recipe.titles[0].wad, WAD);
    for bad in [".", "missing.wad"] {
        assert!(Recipe::read(title(bad).as_bytes(), &dir).is_err(), "{bad}");
    }
    assert!(read(&title("linked.wad")).is_err(), "another directory");
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn removals_and_setup_follow_the_menu_after_an_override() -> Result<()> {
    let mut recipe = read("menu: 4.3E\nremove: [news-channel, forecast-channel, news-channel]")?;
    let ids = |recipe: &Recipe| -> Vec<u64> {
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
    assert_eq!(config.autoboot, Autoboot::HomebrewChannel);
    assert_eq!(config.hacks, [Hack::Wiimmfi, Hack::Fix480p]);

    let recipe = read("menu: 4.3E\npriiloader: {}")?;
    assert_eq!(recipe.priiloader, Some(Priiloader::default()));
    assert_eq!(Priiloader::default().autoboot, Autoboot::SystemMenu);
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
    let recipe = read(&example)?;
    assert_eq!(recipe.remove.len(), 2);
    assert_eq!(&*recipe.titles[0].wad, WAD);
    assert_eq!(recipe.priiloader.unwrap().hacks, [Hack::Fix480p]);
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
    let recipe = |entries: &[String]| read(&format!("menu: 4.3E\ncios: [{}]", entries.join(", ")));
    assert!(recipe(&[cios(249, 56), cios(250, 57)]).is_ok());
    assert!(recipe(&[cios(247, 56)]).is_err());
    assert!(recipe(&[cios(249, 55)]).is_err());
    assert!(message(recipe(&[cios(249, 56), cios(249, 57)])).contains("twice"));
}
