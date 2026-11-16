// SPDX-License-Identifier: GPL-2.0-only
use std::{
    collections::{BTreeSet, HashSet},
    io::Read,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Cios, Hbc, Menu, Priiloader, RemovableChannel, ResolvedPath, Setup, Title};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Stock System Menu target (4.3E, 4.3U, or 4.3J).
    pub menu: Menu,
    /// Stock channels to remove.
    #[serde(default)]
    pub remove: BTreeSet<RemovableChannel>,
    /// Supply a mapping to complete first-run setup with these settings and defaults.
    /// Omit this section or use null to run the Wii setup wizard.
    #[serde(default)]
    pub setup: Option<Setup>,
    /// WADs to install from a local file, inline bytes, or an HTTPS URL.
    #[serde(default)]
    pub titles: Vec<Title>,
    /// Prebuilt, hash-pinned Wii d2x cIOS WADs.
    #[serde(default)]
    pub cios: Vec<Cios>,
    /// Optional Homebrew Channel from a tagged Reborn release or a custom WAD.
    #[serde(default)]
    pub hbc: Option<Hbc>,
    /// Optional pinned Priiloader install with its settings and hacks.
    #[serde(default)]
    pub priiloader: Option<Priiloader>,
}

impl Recipe {
    /// Reads a recipe for `menu_override`, or for the menu it names. What the
    /// types cannot express is checked here, and local paths become relative
    /// to the recipe's directory.
    pub fn load(path: &ResolvedPath, menu_override: Option<Menu>) -> Result<Self> {
        let path = path.as_ref();
        Self::read(
            crate::open_regular_file(path)?.0,
            path.parent()
                .context("recipe path has no parent directory")?,
            menu_override,
        )
    }

    fn read(input: impl Read, directory: &Path, menu_override: Option<Menu>) -> Result<Self> {
        let mut recipe: Self =
            serde_saphyr::from_reader_with_options(input, crate::yaml::options())
                .context("invalid recipe YAML")?;
        let menu = menu_override.unwrap_or(recipe.menu);
        recipe.menu = menu;

        if let Some(setup) = &recipe.setup {
            ensure!(
                menu.allows_language(setup.language_for(menu)),
                "setup language is not supported by menu {menu}"
            );
            let country = setup.country_for(menu);
            ensure!(
                menu.allows_country(country),
                "setup country {} is not supported by menu {menu}",
                country.as_alpha2()
            );
        }
        let mut slots = HashSet::new();
        for cios in &recipe.cios {
            let slot = cios.target.slot;
            ensure!(slots.insert(slot), "recipe lists cIOS slot {slot} twice");
        }

        let sources = (recipe.titles.iter_mut().map(|title| &mut title.source))
            .chain(recipe.cios.iter_mut().map(|cios| &mut cios.source))
            .chain(match &mut recipe.hbc {
                Some(Hbc::Custom { source, .. }) => Some(source),
                _ => None,
            });
        for source in sources {
            source.rebase(directory);
        }
        Ok(recipe)
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, fs};

    use super::*;
    use crate::{Aspect, Autoboot, Data, Hack, TitleId};

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn read(yaml: &str, menu: Option<Menu>) -> Result<Recipe> {
        Recipe::read(yaml.as_bytes(), Path::new("."), menu)
    }

    fn no_fetch(_: &str, _: usize) -> Result<Vec<u8>> {
        panic!("local payload resolution should not fetch")
    }

    #[test]
    fn hbc_resolves_a_semver_using_the_release_digest_or_a_supplied_hash() -> Result<()> {
        for (version, name, sha256, digest) in [
            (
                "2.3.4",
                "hbc-reborn-2.3.4-OHBC.wad",
                "",
                format!("\"sha256:{SHA}\""),
            ),
            (
                "1.12.1",
                "hbc-1.12.1-OHBC.wad",
                &format!(", sha256: {SHA}"),
                "null".to_owned(),
            ),
        ] {
            let recipe = read(
                &format!("menu: 4.3E\nhbc: {{reborn: {{version: {version}{sha256}}}}}"),
                None,
            )?;
            let hbc = recipe.hbc.unwrap();
            let (wad, checksum) = hbc.resolve(|url, _| {
                if url.starts_with("https://api.github.com/") {
                    assert!(url.ends_with(&format!("/releases/tags/v{version}")));
                    Ok(format!(
                        r#"{{"assets":[{{"name":"other.wad"}},{{"name":"{name}","digest":{digest}}}]}}"#
                    ).into_bytes())
                } else {
                    assert!(url.ends_with(&format!("/v{version}/{name}")));
                    Ok(vec![0; 0x40])
                }
            })?;
            assert_eq!(&*wad, &[0; 0x40]);
            assert_eq!(checksum, SHA.parse()?);
        }
        let hbc = read("menu: 4.3E\nhbc: {reborn: {version: 2.3.4}}", None)?
            .hbc
            .unwrap();
        for metadata in [
            r#"{"assets":[{"name":"hbc-reborn-2.3.4-LULZ.wad"}]}"#,
            r#"{"assets":[{"name":"hbc-reborn-2.3.4-OHBC.wad"}]}"#,
        ] {
            assert!(
                hbc.resolve(|url, _| {
                    assert!(url.starts_with("https://api.github.com/"));
                    Ok(metadata.as_bytes().to_vec())
                })
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn hbc_takes_exactly_one_variant() -> Result<()> {
        let recipe = read(
            &format!("menu: 4.3E\nhbc: {{custom: {{source: {{path: file.wad}}, sha256: {SHA}}}}}"),
            None,
        )?;
        assert!(
            matches!(recipe.hbc, Some(Hbc::Custom { source: Data::Path(path), .. }) if path == Path::new("./file.wad"))
        );
        for fields in [
            "reborn: {version: '1.2'}".to_owned(),
            "classic: {}".to_owned(),
            "custom: {source: {path: file.wad}}".to_owned(),
            format!("custom: {{version: 1.16.0, source: {{path: file.wad}}, sha256: {SHA}}}"),
            "reborn: {version: 1.16.0, source: {path: file.wad}}".to_owned(),
            format!(
                "reborn: {{version: 1.16.0}}, custom: {{source: {{path: file.wad}}, sha256: {SHA}}}"
            ),
        ] {
            assert!(
                read(&format!("menu: 4.3E\nhbc: {{{fields}}}"), None).is_err(),
                "{fields}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_source_is_one_path_url_or_binary_block() -> Result<()> {
        let yaml = format!(
            "menu: 4.3E\ntitles:\n  - sha256: {SHA}\n    source:\n      inline: !!binary |\n        /wAB\n        /v8=\n"
        );
        let source = &read(&yaml, None)?.titles[0].source;
        assert!(matches!(
            source.resolve(0..=usize::MAX, no_fetch)?,
            Cow::Borrowed([255, 0, 1, 254, 255])
        ));
        assert!(source.resolve_wad(no_fetch).is_err(), "too small for a WAD");
        for bad in [
            format!("source: {{path: a.wad, inline: !!binary /wAB}}\n    sha256: {SHA}"),
            format!("source: {{inline: /wAB}}\n    sha256: {SHA}"),
            format!("source: {{url: https://example.com/a.wad, path: a.wad}}\n    sha256: {SHA}"),
            format!("source: {{path: null}}\n    sha256: {SHA}"),
            format!("sha256: {SHA}"),
            "source: {inline: !!binary /wAB}".to_owned(),
        ] {
            assert!(
                read(&format!("menu: 4.3E\ntitles:\n  - {bad}\n"), None).is_err(),
                "{bad}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_url_source_is_fetched_within_the_wad_size_limit() -> Result<()> {
        let url = "https://example.com/package.wad";
        let yaml = format!("menu: 4.3E\ntitles: [{{source: {{url: {url}}}, sha256: {SHA}}}]");
        let title = &read(&yaml, None)?.titles[0];
        let bytes = title.source.resolve_wad(|requested, limit| {
            assert_eq!(requested, url);
            assert_eq!(limit, 0x2100_0000);
            Ok(vec![7; 0x40])
        })?;
        assert_eq!(&*bytes, &[7; 0x40]);
        assert_eq!(title.sha256.0, [0xaa; 32]);
        assert!(title.source.resolve_wad(|_, _| Ok(vec![])).is_err());
        Ok(())
    }

    #[test]
    fn titles_and_cios_accept_only_their_own_fields() -> Result<()> {
        let target = "target: {slot: 249, base: 56, revision: 65535}";
        let inline = "source: {inline: !!binary /wAB}";
        let titles = |fields: &str| read(&format!("menu: 4.3E\ntitles: [{{{fields}}}]"), None);
        let cios = |fields: &str| read(&format!("menu: 4.3E\ncios: [{{{fields}}}]"), None);

        assert!(!titles(&format!("{inline}, sha256: {SHA}"))?.titles[0].allow_unsigned);
        assert!(
            titles(&format!("{inline}, sha256: {SHA}, allow_unsigned: true"))?.titles[0]
                .allow_unsigned
        );
        let recipe = cios(&format!("{inline}, sha256: {SHA}, {target}"))?;
        assert_eq!(
            Some(recipe.cios[0].target.title_id()),
            TitleId::new(0x0000_0001_0000_00f9)
        );

        for fields in [
            format!("{inline}, sha256: {SHA}, unexpected: true"),
            format!("{inline}, sha256: {SHA}, {target}"),
            format!("{inline}, sha256: {SHA}, allow_unsigned: false, allow_unsigned: true"),
        ] {
            assert!(titles(&fields).is_err(), "{fields}");
        }
        for fields in [
            format!("{inline}, {target}"),
            format!("{inline}, sha256: {SHA}"),
            format!("{inline}, sha256: {SHA}, {target}, allow_unsigned: true"),
        ] {
            assert!(cios(&fields).is_err(), "{fields}");
        }
        Ok(())
    }

    #[test]
    fn file_path_is_relative_and_rejects_nonfiles() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("wiiform-path-test-{}", std::process::id()));
        fs::create_dir(&dir)?;
        let wad = dir.join("payload.wad");
        let recipe_path = dir.join("recipe.yaml");
        let contents = [0xff; 0x40];
        fs::write(&wad, contents)?;
        std::os::unix::fs::symlink("payload.wad", dir.join("linked.wad"))?;
        fs::write(
            &recipe_path,
            format!("menu: 4.3E\ntitles: [{{source: {{path: linked.wad}}, sha256: {SHA}}}]"),
        )?;
        let recipe = Recipe::load(&ResolvedPath::try_from(recipe_path)?, None)?;
        let source = &recipe.titles[0].source;
        assert!(matches!(source, Data::Path(path) if path.ends_with("linked.wad")));
        assert_eq!(source.resolve_wad(no_fetch)?.as_ref(), contents);
        for bad in [".", "missing.wad"] {
            let yaml = format!("menu: 4.3E\ntitles: [{{source: {{path: {bad}}}, sha256: {SHA}}}]");
            let recipe = Recipe::read(yaml.as_bytes(), &dir, None)?;
            assert!(recipe.titles[0].source.resolve_wad(no_fetch).is_err());
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn removals_and_setup_follow_the_effective_menu() -> Result<()> {
        let ids = |menu| -> Result<Vec<u64>> {
            let recipe = read(
                "menu: 4.3E\nremove: [news-channel, forecast-channel, news-channel]",
                menu,
            )?;
            Ok((recipe.remove.iter())
                .flat_map(|channel| channel.title_ids(recipe.menu))
                .map(TitleId::as_u64)
                .collect())
        };
        assert_eq!(
            ids(None)?,
            [
                0x0001_0002_4841_4650,
                0x0001_0002_4841_4641,
                0x0001_0002_4841_4750,
                0x0001_0002_4841_4741
            ]
        );
        assert_eq!(ids(Some(Menu::J43))?[0], 0x0001_0002_4841_464a);

        assert!(read("menu: 4.3E\nsetup: {language: ja}", Some(Menu::J43)).is_ok());
        for yaml in [
            "menu: 4.3E\nsetup: {country: '110'}",
            "menu: 4.3E\nsetup: {language: en}",
            "menu: 4.3E\nsetup: {country: GB}",
            "menu: 4.3E\nremove: [disc-channel]",
            "menu: 4.3E\nremove: ['0001000248414650']",
        ] {
            assert!(read(yaml, Some(Menu::J43)).is_err(), "{yaml}");
        }
        assert!(read("menu: 4.3E\nsetup: {language: ja}", None).is_err());
        Ok(())
    }

    #[test]
    fn display_is_part_of_first_run_setup() -> Result<()> {
        for yaml in ["menu: 4.3E", "menu: 4.3E\nsetup: null"] {
            assert!(read(yaml, None)?.setup.is_none());
        }
        assert_eq!(
            read("menu: 4.3E\nsetup: {}", None)?.setup,
            Some(Setup::default())
        );

        let recipe = read(
            "menu: 4.3E\nsetup: {nickname: 'Éééééééééé', display: {aspect: '16:9', progressive: true}}",
            None,
        )?;
        let setup = recipe.setup.context("expected setup settings")?;
        assert_eq!(setup.nickname.as_ref(), "Éééééééééé");
        assert_eq!(setup.display.aspect, Aspect::SixteenNine);
        assert!(setup.display.progressive);
        assert!(!setup.display.pal60);
        assert!(read("menu: 4.3E\ndisplay: {}", None).is_err());
        Ok(())
    }

    #[test]
    fn priiloader_takes_settings_and_a_list_of_hacks() -> Result<()> {
        let recipe = read(
            "menu: 4.3E\npriiloader: {stop_disc: true, autoboot: homebrew_channel, hacks: [wiimmfi, fix_480p]}",
            None,
        )?;
        let config = recipe.priiloader.context("expected Priiloader settings")?;
        assert!(config.stop_disc && !config.white_background);
        assert_eq!(config.autoboot, Autoboot::HomebrewChannel);
        assert_eq!(config.hacks, [Hack::Wiimmfi, Hack::Fix480p]);

        let recipe = read("menu: 4.3E\npriiloader: {}", None)?;
        assert_eq!(recipe.priiloader, Some(Priiloader::default()));
        assert_eq!(Priiloader::default().autoboot, Autoboot::SystemMenu);
        assert!(
            read("menu: 4.3E\npriiloader: null", None)?
                .priiloader
                .is_none()
        );
        for priiloader in [
            "{app: override.app}",
            "{hacks: [unknown]}",
            "{hacks: {fix_480p: true}}",
        ] {
            assert!(
                read(&format!("menu: 4.3E\npriiloader: {priiloader}"), None).is_err(),
                "{priiloader}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_documented_example_is_a_valid_recipe() -> Result<()> {
        let example = include_str!("../../../docs/recipe.example.yaml");
        let recipe = read(example, None)?;
        assert_eq!(recipe.remove.len(), 2);
        assert_eq!(recipe.priiloader.unwrap().hacks, [Hack::Fix480p]);
        Ok(())
    }

    #[test]
    fn cios_slots_and_bases_are_checked_when_the_recipe_is_read() {
        let cios = |slot: u8, base: u8| {
            format!(
                "{{source: {{inline: !!binary /wAB}}, sha256: {SHA}, target: {{slot: {slot}, base: {base}, revision: 1}}}}"
            )
        };
        let recipe =
            |entries: &[String]| read(&format!("menu: 4.3E\ncios: [{}]", entries.join(", ")), None);
        assert!(recipe(&[cios(249, 56), cios(250, 57)]).is_ok());
        assert!(recipe(&[cios(247, 56)]).is_err());
        assert!(recipe(&[cios(249, 55)]).is_err());
        let duplicate = recipe(&[cios(249, 56), cios(249, 57)]).unwrap_err();
        assert!(format!("{duplicate:#}").contains("twice"), "{duplicate:#}");
    }
}
