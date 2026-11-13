// SPDX-License-Identifier: GPL-2.0-only
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use serde_json::{Map, Value};
use yaml_rust2::{
    parser::{Event, Parser},
    scanner::TScalarStyle,
};

use crate::{Menu, Priiloader, Recipe, RecipeFile, Setup, Title, titles::allowlisted_title_ids};

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_EVENTS: usize = 20_000;
pub const MAX_DEPTH: usize = 24;

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
    let value = parse_json_subset(source)?;
    let file: RecipeFile =
        serde_json::from_value(value).map_err(|e| error(format!("invalid recipe: {e}")))?;

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
            country.as_u8(),
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

fn parse_json_subset(source: &str) -> Result<Value, RecipeError> {
    let mut parser = Parser::new_from_str(source);
    let mut events = Vec::new();
    let mut depth = 0usize;
    loop {
        let (event, _) = parser
            .next_token()
            .map_err(|e| error(format!("invalid YAML: {e}")))?;
        if events.len() >= MAX_EVENTS {
            return Err(error(format!(
                "recipe contains more than {MAX_EVENTS} YAML events"
            )));
        }
        match event {
            Event::SequenceStart(..) | Event::MappingStart(..) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(error(format!("recipe nesting exceeds {MAX_DEPTH} levels")));
                }
            }
            Event::SequenceEnd | Event::MappingEnd => depth = depth.saturating_sub(1),
            _ => {}
        }
        let done = event == Event::StreamEnd;
        events.push(event);
        if done {
            break;
        }
    }
    let mut decoder = EventDecoder { events, offset: 0 };
    decoder.expect(Event::StreamStart, "missing YAML stream start")?;
    decoder.expect(Event::DocumentStart, "missing YAML document start")?;
    let value = decoder.value(0)?;
    decoder.expect(Event::DocumentEnd, "unexpected YAML document structure")?;
    if decoder.peek() != Some(&Event::StreamEnd) {
        return Err(error("recipe must contain exactly one YAML document"));
    }
    decoder.expect(Event::StreamEnd, "unexpected YAML stream structure")?;
    if decoder.offset != decoder.events.len() {
        return Err(error("unexpected YAML events after document"));
    }
    Ok(value)
}

struct EventDecoder {
    events: Vec<Event>,
    offset: usize,
}

impl EventDecoder {
    fn peek(&self) -> Option<&Event> {
        self.events.get(self.offset)
    }

    fn take(&mut self) -> Result<Event, RecipeError> {
        let event = self
            .events
            .get(self.offset)
            .cloned()
            .ok_or_else(|| error("unexpected end of YAML events"))?;
        self.offset += 1;
        match event {
            Event::Alias(_) => Err(error("YAML aliases are forbidden")),
            Event::Nothing => Err(error("unsupported YAML event")),
            _ => Ok(event),
        }
    }

    fn expect(&mut self, expected: Event, message: &str) -> Result<(), RecipeError> {
        if self.take()? == expected {
            Ok(())
        } else {
            Err(error(message))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, RecipeError> {
        match self.take()? {
            Event::Scalar(value, style, anchor, tag) => {
                check_metadata(anchor, tag)?;
                scalar_to_json(&value, style)
            }
            Event::SequenceStart(anchor, tag) => {
                check_metadata(anchor, tag)?;
                let nested = depth + 1;
                if nested > MAX_DEPTH {
                    return Err(error(format!("recipe nesting exceeds {MAX_DEPTH} levels")));
                }
                let mut items = Vec::new();
                loop {
                    match self.peek() {
                        Some(Event::SequenceEnd) => {
                            self.offset += 1;
                            break;
                        }
                        Some(_) => items.push(self.value(nested)?),
                        None => return Err(error("unterminated YAML sequence")),
                    }
                }
                Ok(Value::Array(items))
            }
            Event::MappingStart(anchor, tag) => {
                check_metadata(anchor, tag)?;
                let nested = depth + 1;
                if nested > MAX_DEPTH {
                    return Err(error(format!("recipe nesting exceeds {MAX_DEPTH} levels")));
                }
                let mut object = Map::new();
                loop {
                    match self.peek() {
                        Some(Event::MappingEnd) => {
                            self.offset += 1;
                            break;
                        }
                        Some(_) => {
                            let key = self.map_key()?;
                            if object.contains_key(&key) {
                                return Err(error(format!(
                                    "duplicate key {key:?} in YAML mapping"
                                )));
                            }
                            let value = self.value(nested)?;
                            object.insert(key, value);
                        }
                        None => return Err(error("unterminated YAML mapping")),
                    }
                }
                Ok(Value::Object(object))
            }
            Event::Alias(_) => Err(error("YAML aliases are forbidden")),
            _ => Err(error(
                "recipe root and values must be YAML scalars, sequences or mappings",
            )),
        }
    }

    fn map_key(&mut self) -> Result<String, RecipeError> {
        match self.take()? {
            Event::Scalar(value, style, anchor, tag) => {
                check_metadata(anchor, tag)?;
                match scalar_to_json(&value, style)? {
                    Value::String(key) => Ok(key),
                    _ => Err(error("YAML mapping keys must be strings")),
                }
            }
            _ => Err(error("YAML mapping keys must be scalar strings")),
        }
    }
}

fn check_metadata(anchor: usize, tag: Option<yaml_rust2::parser::Tag>) -> Result<(), RecipeError> {
    if anchor != 0 {
        return Err(error("YAML anchors are forbidden"));
    }
    if tag.is_some() {
        return Err(error("explicit YAML tags are forbidden"));
    }
    Ok(())
}

fn scalar_to_json(value: &str, style: TScalarStyle) -> Result<Value, RecipeError> {
    if style != TScalarStyle::Plain {
        return Ok(Value::String(value.to_owned()));
    }
    if value == "true" {
        return Ok(Value::Bool(true));
    }
    if value == "false" {
        return Ok(Value::Bool(false));
    }
    if looks_like_implicit_typed_scalar(value) {
        return Err(error(
            "unsupported YAML scalar; quote numeric, null, and non-lowercase boolean strings",
        ));
    }
    Ok(Value::String(value.to_owned()))
}

fn looks_like_implicit_typed_scalar(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || matches!(
            lower.as_str(),
            "~" | "null" | "yes" | "no" | "on" | "off" | "true" | "false"
        )
    {
        return true;
    }
    if matches!(lower.as_str(), ".inf" | "+.inf" | "-.inf" | ".nan") {
        return true;
    }
    is_yaml_number(value) || looks_like_timestamp(value)
}

fn is_yaml_number(value: &str) -> bool {
    let cleaned = value.replace('_', "");
    let unsigned = cleaned
        .strip_prefix('+')
        .or_else(|| cleaned.strip_prefix('-'))
        .unwrap_or(&cleaned);
    for prefix in ["0x", "0X", "0o", "0O", "0b", "0B"] {
        if let Some(digits) = unsigned.strip_prefix(prefix)
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_hexdigit())
        {
            return true;
        }
    }
    if cleaned.parse::<i64>().is_ok()
        || cleaned.parse::<u64>().is_ok()
        || cleaned.parse::<f64>().is_ok()
    {
        return true;
    }
    let parts: Vec<_> = cleaned.split(':').collect();
    parts.len() > 1
        && parts.iter().all(|part| {
            let part = part
                .strip_prefix('+')
                .or_else(|| part.strip_prefix('-'))
                .unwrap_or(part);
            !part.is_empty() && part.parse::<f64>().is_ok()
        })
}

fn looks_like_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && (bytes.len() == 10 || matches!(bytes.get(10), Some(b't' | b'T' | b' ')))
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
    fn menu_override_revalidates_source_settings_and_retargets_aliases() {
        let recipe = parse(
            "menu: 4.3E\nremove: [forecast-channel]\nsetup: {country: '110', sensor_bar: below}\n",
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
        assert!(parse("menu: 4.3E\nsetup: {country: '49'}\n", None).is_err());

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
