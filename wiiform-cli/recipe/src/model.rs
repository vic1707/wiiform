// SPDX-License-Identifier: GPL-2.0-only
use std::{collections::HashSet, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Cios, Menu, Priiloader, RemovalInput, ResolvedPath, Setup, Title, TitleId};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Stock System Menu target (4.3E, 4.3U, or 4.3J).
    pub menu: Menu,
    /// Channels to remove. Names expand using the effective menu's region.
    #[serde(default)]
    remove: Vec<RemovalInput>,
    /// Supply a mapping to complete first-run setup with these settings and defaults.
    /// Omit this section or use null to run the Wii setup wizard.
    #[serde(default)]
    pub setup: Option<Setup>,
    /// WADs to install from a file or inline bytes.
    #[serde(default)]
    pub titles: Vec<Title>,
    /// Prebuilt, hash-pinned Wii d2x cIOS WADs.
    #[serde(default)]
    pub cios: Vec<Cios>,
    /// Optional pinned Priiloader install with settings and hacks.
    #[serde(default)]
    pub priiloader: Option<Priiloader>,
}

impl Recipe {
    pub fn load(path: &ResolvedPath, menu_override: Option<Menu>) -> Result<(Self, Vec<TitleId>)> {
        let path = path.as_ref();
        Self::read(
            crate::open_regular_file(path)?.0,
            path.parent()
                .context("recipe path has no parent directory")?,
            menu_override,
        )
    }

    fn read(
        input: impl Read,
        directory: &Path,
        menu_override: Option<Menu>,
    ) -> Result<(Self, Vec<TitleId>)> {
        let mut recipe: Self =
            serde_saphyr::from_reader_with_options(input, crate::yaml::options())
                .context("invalid recipe YAML")?;
        let mut remove = recipe.validate_menu(recipe.menu)?;
        if let Some(menu) = menu_override.filter(|menu| *menu != recipe.menu) {
            remove = recipe.validate_menu(menu)?;
            recipe.menu = menu;
        }

        for title in &mut recipe.titles {
            title.source.rebase(directory);
        }
        for cios in &mut recipe.cios {
            cios.source.rebase(directory);
        }
        Ok((recipe, remove))
    }

    fn validate_menu(&self, menu: Menu) -> Result<Vec<TitleId>> {
        if let Some(setup) = &self.setup {
            ensure!(
                menu.allows_language(setup.language_for(menu)),
                "setup language is not supported by menu {}",
                menu.as_str()
            );
            let country = setup.country_for(menu);
            ensure!(
                menu.allows_country(country),
                "setup country {} is not supported by menu {}",
                country.as_alpha2(),
                menu.as_str()
            );
        }

        let mut seen = HashSet::new();
        let mut remove = Vec::new();
        for input in &self.remove {
            for id in input.expand(menu)? {
                ensure!(
                    seen.insert(id),
                    "duplicate removal after channel alias expansion"
                );
                remove.push(id);
            }
        }
        Ok(remove)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::Data;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn read(yaml: &str, menu: Option<Menu>) -> Result<(Recipe, Vec<TitleId>)> {
        Recipe::read(yaml.as_bytes(), Path::new("."), menu)
    }

    #[test]
    fn binary_and_source_shape() -> Result<()> {
        let yaml = format!(
            "menu: 4.3E\ntitles:\n  - sha256: {SHA}\n    source:\n      inline: !!binary |\n        /wAB\n        /v8=\n"
        );
        let (recipe, _) = read(&yaml, None)?;
        let bytes = recipe.titles[0].source.resolve(0..=usize::MAX)?;
        assert!(matches!(
            bytes,
            std::borrow::Cow::Borrowed([255, 0, 1, 254, 255])
        ));
        assert!(recipe.titles[0].source.resolve(crate::WAD_SIZE).is_err());
        for bad in [
            format!("source: {{path: a.wad, inline: !!binary /wAB}}\n    sha256: {SHA}"),
            format!("source: {{inline: /wAB}}\n    sha256: {SHA}"),
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
        let recipe_path = ResolvedPath::try_from(recipe_path)?;
        let (recipe, _) = Recipe::load(&recipe_path, None)?;
        let source = &recipe.titles[0].source;
        assert!(matches!(source, Data::Path(path) if path.ends_with("linked.wad")));
        assert_eq!(source.resolve(0x40..=0x40)?.as_ref(), contents);
        for bad in [".", "missing.wad"] {
            let yaml = format!("menu: 4.3E\ntitles: [{{source: {{path: {bad}}}, sha256: {SHA}}}]");
            let (recipe, _) = Recipe::read(yaml.as_bytes(), &dir, None)?;
            assert!(recipe.titles[0].source.resolve(crate::WAD_SIZE).is_err());
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn menu_override_rechecks_settings_and_removals() -> Result<()> {
        let (_, ids) = read("menu: 4.3E\nremove: [forecast-channel]", Some(Menu::J43))?;
        assert_eq!(
            ids.iter().map(|id| id.as_u64()).collect::<Vec<_>>(),
            [0x0001_0002_4841_464a, 0x0001_0002_4841_4641]
        );
        for yaml in [
            "menu: 4.3E\nsetup: {country: '110'}",
            "menu: 4.3E\nsetup: {language: ja}",
            "menu: 4.3E\nsetup: {country: GB}",
            "menu: 4.3E\nremove: [forecast-channel, '0001000248414650']",
            "menu: 4.3E\nremove: [forecast-channel, '000100024841464a']",
            "menu: 4.3E\nremove: ['0000000100000002']",
        ] {
            assert!(read(yaml, Some(Menu::J43)).is_err(), "{yaml}");
        }
        Ok(())
    }

    #[test]
    fn display_is_part_of_first_run_setup() -> Result<()> {
        for yaml in ["menu: 4.3E", "menu: 4.3E\nsetup: null"] {
            assert!(read(yaml, None)?.0.setup.is_none());
        }
        assert_eq!(
            read("menu: 4.3E\nsetup: {}", None)?.0.setup,
            Some(Setup::default())
        );

        let (recipe, _) = read(
            "menu: 4.3E\nsetup: {nickname: 'Éééééééééé', display: {aspect: '16:9', progressive: true}}",
            None,
        )?;
        let setup = recipe.setup.context("expected setup settings")?;
        assert_eq!(setup.nickname.as_ref(), "Éééééééééé");
        assert_eq!(setup.display.aspect, crate::Aspect::SixteenNine);
        assert!(setup.display.progressive);
        assert!(!setup.display.pal60);
        assert!(read("menu: 4.3E\ndisplay: {}", None).is_err());
        Ok(())
    }

    #[test]
    fn priiloader_accepts_only_settings_and_hacks() -> Result<()> {
        let (recipe, _) = read(
            "menu: 4.3E\npriiloader: {settings: {stop_disc: true}, hacks: {fix_480p: true}}",
            None,
        )?;
        let config = recipe.priiloader.context("expected Priiloader settings")?;
        assert!(
            config.settings.stop_disc && config.settings.black_background && config.hacks.fix_480p
        );
        for priiloader in ["{}", "{settings: null, hacks: null}"] {
            let (recipe, _) = read(&format!("menu: 4.3E\npriiloader: {priiloader}"), None)?;
            assert_eq!(recipe.priiloader, Some(Priiloader::default()));
        }

        let (recipe, _) = read("menu: 4.3E\npriiloader: null", None)?;
        assert!(recipe.priiloader.is_none());
        assert!(read("menu: 4.3E\npriiloader: {app: override.app}", None).is_err());
        Ok(())
    }
}
