// SPDX-License-Identifier: GPL-2.0-only
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use crate::{
    Menu, Priiloader, Recipe, RecipeFile, Setup, Title, WadSource, titles::allowlisted_title_ids,
};

#[derive(Debug)]
pub struct RecipeError(String);

impl std::fmt::Display for RecipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RecipeError {}

fn error(message: impl Into<String>) -> RecipeError {
    RecipeError(message.into())
}

pub fn load_recipe(path: &Path, menu_override: Option<Menu>) -> Result<Recipe, RecipeError> {
    let recipe_path = fs::canonicalize(path).map_err(|e| {
        error(format!(
            "cannot resolve recipe path {}: {e}",
            path.display()
        ))
    })?;
    let input = open_regular_file(&recipe_path)
        .map_err(|e| error(format!("cannot read recipe {}: {e}", recipe_path.display())))?;
    parse_recipe_reader(input, &recipe_path, menu_override)
}

pub fn parse_recipe(
    bytes: &[u8],
    recipe_path: &Path,
    menu_override: Option<Menu>,
) -> Result<Recipe, RecipeError> {
    parse_recipe_reader(Cursor::new(bytes), recipe_path, menu_override)
}

/// Parse one recipe document from any reader. `recipe_path` supplies the base
/// directory for relative WAD and Priiloader paths (for stdin, use a path in
/// the current directory).
fn parse_recipe_reader<R: Read>(
    reader: R,
    recipe_path: &Path,
    menu_override: Option<Menu>,
) -> Result<Recipe, RecipeError> {
    let file: RecipeFile = serde_saphyr::from_reader_with_options(reader, crate::yaml::options())
        .map_err(|e| error(format!("invalid recipe YAML: {e}")))?;

    let declared_menu = file.menu;
    validate_setup_for(file.setup.as_ref(), declared_menu)?;
    let remove_inputs = file.remove.clone();
    let remove = resolve_removals(&remove_inputs, declared_menu)?;

    let base_dir = recipe_path
        .parent()
        .ok_or_else(|| error("recipe path has no parent directory"))?;
    let absolute_base;
    let base_dir = if base_dir.is_absolute() {
        base_dir
    } else {
        absolute_base = std::env::current_dir()
            .map_err(|e| error(format!("cannot resolve current directory: {e}")))?
            .join(base_dir);
        &absolute_base
    };
    let titles = file
        .titles
        .into_iter()
        .enumerate()
        .map(|(index, title)| {
            let source = match title.source {
                WadSource::File(path) => {
                    let label = format!("titles[{index}].path");
                    let candidate = if path.is_absolute() {
                        path
                    } else {
                        base_dir.join(path)
                    };
                    WadSource::File(resolve_regular_file(&candidate).map_err(|e| {
                        error(format!(
                            "{label} must resolve to an existing regular file: {e}"
                        ))
                    })?)
                }
                inline => inline,
            };
            Ok(Title {
                source,
                sha256: title.sha256,
                signature_policy: title.signature_policy,
            })
        })
        .collect::<Result<Vec<_>, RecipeError>>()?;

    let priiloader = file
        .priiloader
        .map(|mut input| -> Result<Priiloader, RecipeError> {
            let app = input
                .app
                .take()
                .map(|path| {
                    let candidate = if path.is_absolute() {
                        path
                    } else {
                        base_dir.join(path)
                    };
                    resolve_regular_file(&candidate).map_err(|e| {
                        error(format!(
                            "priiloader.app must resolve to an existing regular file: {e}"
                        ))
                    })
                })
                .transpose()?;
            Ok(Priiloader {
                app,
                settings: input.settings,
                hacks: input.hacks,
            })
        })
        .transpose()?;

    let recipe = Recipe {
        menu: declared_menu,
        remove,
        setup: file.setup,
        display: file.display,
        titles,
        priiloader,
        remove_inputs,
    };
    match menu_override {
        Some(menu) => recipe.with_menu(menu),
        None => Ok(recipe),
    }
}

pub(super) fn retarget_recipe(mut recipe: Recipe, menu: Menu) -> Result<Recipe, RecipeError> {
    validate_setup_for(recipe.setup.as_ref(), menu)?;
    recipe.remove = resolve_removals(&recipe.remove_inputs, menu)?;
    recipe.menu = menu;
    Ok(recipe)
}

fn validate_setup_for(setup: Option<&Setup>, menu: Menu) -> Result<(), RecipeError> {
    let Some(setup) = setup else { return Ok(()) };
    if !menu.allows_language(setup.language_for(menu)) {
        return Err(error(format!(
            "setup language is not supported by menu {}",
            menu.as_str()
        )));
    }
    let country = setup.country_for(menu);
    if !menu.allows_country(country) {
        return Err(error(format!(
            "setup country {} is not supported by menu {}",
            country.as_alpha2(),
            menu.as_str()
        )));
    }
    Ok(())
}

fn resolve_removals(
    inputs: &[crate::RemovalInput],
    menu: Menu,
) -> Result<Vec<crate::TitleId>, RecipeError> {
    let mut remove = Vec::new();
    let mut seen = HashSet::new();
    let allowed = allowlisted_title_ids();
    for item in inputs {
        if !item.is_allowed() {
            return Err(error(
                "remove contains a title ID outside the supported channel allowlist",
            ));
        }
        for id in item.expand(menu) {
            if !allowed.contains(id.as_str()) {
                return Err(error("remove expansion produced an unsupported title ID"));
            }
            if !seen.insert(id.clone()) {
                return Err(error("duplicate removal after channel alias expansion"));
            }
            remove.push(id);
        }
    }
    Ok(remove)
}

/// Resolve a path to an existing regular file. Metadata follows symlinks, so a
/// symlink is accepted when its target is a regular file.
pub fn resolve_regular_file(path: &Path) -> anyhow::Result<PathBuf> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| anyhow::anyhow!("cannot resolve {}: {e}", path.display()))?;
    anyhow::ensure!(
        canonical.is_file(),
        "{} is not a regular file",
        path.display()
    );
    Ok(canonical)
}

/// Open a path without blocking on a FIFO, then verify the opened object is a
/// regular file. Symlinks to regular files are accepted.
pub fn open_regular_file(path: &Path) -> anyhow::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|e| anyhow::anyhow!("cannot inspect {}: {e}", path.display()))?;
    anyhow::ensure!(
        metadata.is_file(),
        "{} is not a regular file",
        path.display()
    );
    Ok(file)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::yaml::MAX_DEPTH;

    fn parse(source: &str, menu_override: Option<Menu>) -> Result<Recipe, RecipeError> {
        parse_recipe(
            source.as_bytes(),
            Path::new("/recipes/example.yaml"),
            menu_override,
        )
    }

    #[test]
    fn omission_defaults_settings_and_null_is_rejected() {
        let minimal = parse("menu: 4.3E", None).unwrap();
        assert!(minimal.setup().is_none());
        assert!(minimal.display().is_none());
        assert!(minimal.priiloader().is_none());

        let prepared = parse(
            "menu: 4.3E\nsetup: {}\ndisplay: {}\npriiloader: {settings: {}}",
            Some(Menu::J43),
        )
        .unwrap();
        let setup = prepared.setup().unwrap();
        assert_eq!(setup.nickname.as_str(), "Wii");
        assert_eq!(setup.country_for(Menu::J43), crate::CountryCode::Japan);
        assert_eq!(setup.language_for(Menu::J43), crate::Language::Ja);
        assert_eq!(setup.sensor_bar, crate::SensorBar::Above);
        assert_eq!(prepared.display(), Some(&crate::Display::default()));
        let priiloader = prepared.priiloader().unwrap();
        assert!(!priiloader.settings.stop_disc);
        assert!(priiloader.settings.black_background);
        assert_eq!(priiloader.hacks.enabled().next(), None);

        for fields in [
            "setup: null",
            "display: null",
            "priiloader: null",
            "priiloader: {}",
            "priiloader: {settings: null}",
            "setup: {nickname: null}",
            "setup: {nickname: ''}",
            "setup: {language: null}",
            "setup: {country: null}",
            "display: {progressive: null}",
            "priiloader: {settings: {stop_disc: null}}",
        ] {
            assert!(
                parse(&format!("menu: 4.3E\n{fields}"), None).is_err(),
                "{fields}"
            );
        }

        let schema: serde_json::Value =
            serde_json::from_str(&crate::json_schema().unwrap()).unwrap();
        assert_eq!(schema["required"], serde_json::json!(["menu"]));
        let title_schema = &schema["$defs"]["TitleFile"];
        assert_eq!(title_schema["oneOf"].as_array().unwrap().len(), 2);
        assert_eq!(
            title_schema["oneOf"][0]["required"],
            serde_json::json!(["path", "sha256"])
        );
        assert_eq!(
            title_schema["oneOf"][1]["required"],
            serde_json::json!(["inline", "sha256"])
        );
        assert_eq!(
            title_schema["oneOf"][1]["properties"]["inline"]["contentEncoding"],
            "base64"
        );
        let setup_schema = &schema["$defs"]["Setup"];
        assert!(setup_schema.get("required").is_none());
        assert_eq!(setup_schema["properties"]["nickname"]["default"], "Wii");
        assert_eq!(setup_schema["properties"]["nickname"]["type"], "string");
        assert!(
            setup_schema["properties"]["language"]
                .get("default")
                .is_none()
        );
        assert_eq!(
            schema["$defs"]["PriiloaderFile"]["required"],
            serde_json::json!(["settings"])
        );
        assert!(
            schema["$defs"]["PriiloaderSettings"]
                .get("required")
                .is_none()
        );
    }

    #[test]
    fn yaml_parser_rejects_ambiguous_or_expanded_input() {
        for source in [
            "menu: 4.3E\nmenu: 4.3U",
            "menu: 4.3E\n---\nmenu: 4.3U",
            "menu: 4.3E\nremove: &items [forecast-channel]\nremove: *items",
            "menu: 4.3E\nsetup: {nickname: !local value}",
            "menu: 4.3E\nsetup: {nickname: 123}",
            "menu: 4.3E\nremove: [yes]",
            "menu: 4.3E\nsetup: {country: true}",
        ] {
            assert!(
                parse(source, None).is_err(),
                "unexpectedly accepted YAML {source:?}"
            );
        }

        // YAML 1.2 treats `NO` as a string; it is Norway's valid ISO country code.
        assert!(parse("menu: 4.3E\nsetup: {country: NO}", None).is_ok());
        assert!(parse("menu: 4.3E\nsetup: {nickname: true}", None).is_err());
        assert!(parse("menu: 4.3E\nsetup: {nickname: '123'}", None).is_ok());

        let nested = format!(
            "menu: 4.3E\nsetup: {}",
            "[".repeat(MAX_DEPTH + 1) + &"]".repeat(MAX_DEPTH + 1)
        );
        assert!(parse(&nested, None).is_err());
    }

    #[test]
    fn yaml_parser_accepts_large_documents_but_keeps_structural_budgets() {
        let source = format!("menu: 4.3E\n#{}", "x".repeat(2 * 1024 * 1024));
        assert!(parse(&source, None).is_ok());
    }

    #[test]
    fn title_source_requires_one_source_and_a_sha256() {
        let sha = "0".repeat(64);
        for source in [
            format!("menu: 4.3E\ntitles: [{{sha256: \"{sha}\"}}]"),
            format!(
                "menu: 4.3E\ntitles: [{{path: x.wad, inline: !!binary YQ==, sha256: \"{sha}\"}}]"
            ),
            format!(
                "menu: 4.3E\ntitles: [{{path: null, inline: !!binary YQ==, sha256: \"{sha}\"}}]"
            ),
            format!("menu: 4.3E\ntitles: [{{inline: null, sha256: \"{sha}\"}}]"),
            format!(
                "menu: 4.3E\ntitles: [{{path: x.wad, sha256: \"{sha}\", signature_policy: null}}]"
            ),
            "menu: 4.3E\ntitles: [{path: x.wad}]".to_owned(),
            format!("menu: 4.3E\ntitles: [{{inline: YQ==, sha256: \"{sha}\"}}]"),
        ] {
            assert!(
                parse(&source, None).is_err(),
                "unexpectedly accepted {source:?}"
            );
        }

        let recipe = parse(
            &format!(
                "menu: 4.3E\ntitles:\n  - inline: !!binary |\n      YWJj\n    sha256: \"{sha}\"\n"
            ),
            None,
        )
        .unwrap();
        let title = &recipe.titles()[0];
        assert_eq!(title.source, WadSource::Inline(b"abc".to_vec()));
        assert_eq!(title.sha256.as_str(), sha);
    }

    #[test]
    fn title_paths_resolve_relative_and_follow_regular_symlinks() {
        let dir = std::env::temp_dir().join(format!(
            "wiiform-recipe-path-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let wad = dir.join("payload.wad");
        fs::write(&wad, b"fixture").unwrap();
        let link = dir.join("linked.wad");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&wad, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&wad, &link).unwrap();

        let sha = "0".repeat(64);
        let source = format!("menu: 4.3E\ntitles: [{{path: linked.wad, sha256: \"{sha}\"}}]");
        let recipe = parse_recipe(source.as_bytes(), &dir.join("recipe.yaml"), None).unwrap();
        let expected = fs::canonicalize(&wad).unwrap();
        assert_eq!(recipe.titles()[0].source, WadSource::File(expected));

        let missing = format!("menu: 4.3E\ntitles: [{{path: missing.wad, sha256: \"{sha}\"}}]");
        assert!(parse_recipe(missing.as_bytes(), &dir.join("recipe.yaml"), None).is_err());
        let non_file = format!("menu: 4.3E\ntitles: [{{path: ., sha256: \"{sha}\"}}]");
        assert!(parse_recipe(non_file.as_bytes(), &dir.join("recipe.yaml"), None).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn menu_override_revalidates_source_settings_and_retargets_aliases() {
        let recipe = parse(
            "menu: 4.3E\nremove: [forecast-channel]\nsetup: {country: GB, sensor_bar: below}\n",
            None,
        )
        .unwrap();
        assert_eq!(recipe.menu(), Menu::E43);
        assert_eq!(recipe.setup().unwrap().country_for(Menu::E43).as_u8(), 110);
        assert_eq!(recipe.setup().unwrap().sensor_bar, crate::SensorBar::Below);
        assert_eq!(
            recipe
                .remove()
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            ["0001000248414650", "0001000248414641"]
        );

        assert!(
            recipe.with_menu(Menu::J43).is_err(),
            "explicit UK country cannot retarget to Japan"
        );
        assert!(parse("menu: 4.3E\nsetup: {language: ja}\n", Some(Menu::J43)).is_err());
        assert!(parse("menu: 4.3E\nsetup: {language: de}\n", Some(Menu::U43)).is_err());
        assert!(parse("menu: 4.3E\nsetup: {country: US}\n", None).is_err());
        let norway = parse("menu: 4.3E\nsetup: {country: 'NO'}\n", None).unwrap();
        assert_eq!(
            norway.setup().unwrap().country,
            Some(crate::CountryCode::Norway)
        );

        let retargeted =
            parse("menu: 4.3E\nremove: [forecast-channel]\n", Some(Menu::J43)).unwrap();
        assert_eq!(retargeted.menu(), Menu::J43);
        assert_eq!(
            retargeted
                .remove()
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            ["000100024841464a", "0001000248414641"]
        );
    }
}
