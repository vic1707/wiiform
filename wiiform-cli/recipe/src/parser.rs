// SPDX-License-Identifier: GPL-2.0-only
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use serde_saphyr::{DuplicateKeyPolicy, MergeKeyPolicy, Options};

use crate::{Menu, Priiloader, Recipe, RecipeFile, Setup, Title, titles::allowlisted_title_ids};

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_EVENTS: usize = 20_000;
pub const MAX_DEPTH: usize = 24;
const MAX_NODES: usize = 8_192;

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
    let bytes = read_bounded(&recipe_path)?;
    parse_recipe(&bytes, &recipe_path, menu_override)
}

fn recipe_yaml_options() -> Options {
    let mut options = Options::default();
    options.duplicate_keys = DuplicateKeyPolicy::Error;
    options.merge_keys = MergeKeyPolicy::Error;
    options.reject_unsupported_tags = true;
    options.strict_booleans = true;
    options.no_schema = true;
    options.with_snippet = false;

    let mut budget = serde_saphyr::Budget::default();
    budget.max_reader_input_bytes = Some(MAX_BYTES);
    budget.max_events = MAX_EVENTS;
    budget.max_aliases = 0;
    budget.max_anchors = 0;
    budget.max_depth = MAX_DEPTH;
    budget.max_documents = 1;
    budget.max_nodes = MAX_NODES;
    budget.max_total_scalar_bytes = MAX_BYTES;
    budget.flow_nesting_limit = MAX_DEPTH;
    options.budget = Some(budget);
    options
}

pub fn parse_recipe(
    bytes: &[u8],
    recipe_path: &Path,
    menu_override: Option<Menu>,
) -> Result<Recipe, RecipeError> {
    if bytes.len() > MAX_BYTES {
        return Err(error(format!("recipe exceeds {MAX_BYTES} bytes")));
    }
    let source =
        std::str::from_utf8(bytes).map_err(|e| error(format!("recipe is not UTF-8: {e}")))?;
    let file: RecipeFile = serde_saphyr::from_str_with_options(source, recipe_yaml_options())
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
            let path = resolve_path(base_dir, title.path, &format!("titles[{index}].path"))?;
            Ok(Title {
                path,
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
                    let metadata = fs::symlink_metadata(&candidate).map_err(|e| {
                        error(format!("priiloader.app must be a regular file: {e}"))
                    })?;
                    if !metadata.file_type().is_file() {
                        return Err(error(
                            "priiloader.app must be a regular file, not a symlink",
                        ));
                    }
                    resolve_path(base_dir, candidate, "priiloader.app")
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

fn read_bounded(path: &Path) -> Result<Vec<u8>, RecipeError> {
    let file = fs::File::open(path).map_err(|e| error(format!("cannot read recipe: {e}")))?;
    let mut bytes = Vec::with_capacity(MAX_BYTES.min(8192));
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| error(format!("cannot read recipe: {e}")))?;
    if bytes.len() > MAX_BYTES {
        return Err(error(format!("recipe exceeds {MAX_BYTES} bytes")));
    }
    Ok(bytes)
}

fn resolve_path(base: &Path, path: PathBuf, where_: &str) -> Result<PathBuf, RecipeError> {
    let text = path
        .to_str()
        .ok_or_else(|| error(format!("{where_} must be a UTF-8 path")))?;
    if text.is_empty() || text.contains('\0') {
        return Err(error(format!(
            "{where_} must be a nonempty path without NUL"
        )));
    }
    let candidate = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    if let Ok(canonical) = fs::canonicalize(&candidate) {
        return Ok(canonical);
    }
    Ok(lexical_absolute(&candidate))
}

fn lexical_absolute(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

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
            "menu: 4.3E\nsetup: {}\ndisplay: {}\npriiloader: {}",
            Some(Menu::J43),
        )
        .unwrap();
        let setup = prepared.setup().unwrap();
        assert_eq!(setup.nickname.as_str(), "Wii");
        assert_eq!(setup.country_for(Menu::J43), crate::CountryCode::Japan);
        assert_eq!(setup.language_for(Menu::J43), crate::Language::Ja);
        assert_eq!(setup.sensor_bar, crate::SensorBar::Above);
        assert_eq!(prepared.display(), Some(&crate::Display::default()));
        assert_eq!(prepared.priiloader().unwrap().settings.stop_disc, None);

        for fields in [
            "setup: null",
            "display: null",
            "priiloader: null",
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
        let setup_schema = &schema["$defs"]["Setup"];
        assert!(setup_schema.get("required").is_none());
        assert_eq!(setup_schema["properties"]["nickname"]["default"], "Wii");
        assert_eq!(setup_schema["properties"]["nickname"]["type"], "string");
        assert!(
            setup_schema["properties"]["language"]
                .get("default")
                .is_none()
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
    fn yaml_parser_rejects_oversized_input() {
        let source = format!("menu: 4.3E\n#{}", "x".repeat(MAX_BYTES));
        assert!(parse(&source, None).is_err());
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
