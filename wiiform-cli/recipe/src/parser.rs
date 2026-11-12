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

use crate::{
    Menu,
    types::{Display, Priiloader, Recipe, RecipeFile, Title, allowlisted_title_ids},
};

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
    load_recipe_bytes(path, menu_override).map(|(recipe, _)| recipe)
}

pub fn load_recipe_bytes(
    path: &Path,
    menu_override: Option<Menu>,
) -> Result<(Recipe, Vec<u8>), RecipeError> {
    let recipe_path = fs::canonicalize(path).map_err(|e| {
        error(format!(
            "cannot resolve recipe path {}: {e}",
            path.display()
        ))
    })?;
    let bytes = read_bounded(&recipe_path)?;
    let recipe = parse_recipe(&bytes, &recipe_path, menu_override)?;
    Ok((recipe, bytes))
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

    let menu = menu_override.unwrap_or(file.menu);
    if let Some(setup) = &file.setup {
        if let Some(language) = setup.language
            && !menu.allows_language(language)
        {
            return Err(error(format!(
                "setup language is not supported by menu {menu:?}"
            )));
        }
        if let Some(name) = &setup.nickname {
            let chars = name.chars().count();
            if !(1..=10).contains(&chars)
                || name.trim().is_empty()
                || name.chars().any(|c| c as u32 > 0xffff || c.is_control())
            {
                return Err(error(
                    "setup.nickname must have 1–10 printable BMP characters",
                ));
            }
        }
    }
    if let Some(display) = &file.display {
        validate_display(display)?;
    }

    let mut remove = Vec::new();
    let mut seen = HashSet::new();
    let allowed = allowlisted_title_ids();
    for item in &file.remove {
        if !item.is_allowed() {
            return Err(error(
                "remove contains a title ID outside the supported channel allowlist",
            ));
        }
        for id in item.expand(menu) {
            if !allowed.contains(id.as_str()) {
                return Err(error("remove expansion produced an unsupported title ID"));
            }
            if !seen.insert(id.as_str().to_owned()) {
                return Err(error("duplicate removal after channel alias expansion"));
            }
            remove.push(id);
        }
    }

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

    Ok(Recipe {
        menu,
        remove,
        setup: file.setup,
        display: file.display,
        titles,
        priiloader,
    })
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

fn validate_display(display: &Display) -> Result<(), RecipeError> {
    if display.is_empty() {
        return Err(error("display must set at least one field"));
    }
    Ok(())
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
    use crate::SignaturePolicy;

    fn parse(source: &str, menu_override: Option<Menu>) -> Result<Recipe, RecipeError> {
        parse_recipe(
            source.as_bytes(),
            Path::new("/recipes/example.yaml"),
            menu_override,
        )
    }

    #[test]
    fn valid_recipe_normalizes_sha_paths_and_removals() {
        let recipe = parse(
            "menu: 4.3E\nremove: [forecast-channel, news-channel, photo-channel]\nsetup:\n  nickname: \"Lab Wii\"\n  language: fr\ndisplay:\n  aspect: \"16:9\"\n  progressive: true\ntitles:\n  - path: ./wads/game.wad\n    sha256: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            None,
        ).unwrap();
        assert_eq!(recipe.menu, Menu::E43);
        assert_eq!(
            recipe
                .remove
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            [
                "0001000248414650",
                "0001000248414641",
                "0001000248414750",
                "0001000248414741",
                "0001000248414141",
                "0001000248415941",
            ]
        );
        assert_eq!(recipe.titles[0].path, Path::new("/recipes/wads/game.wad"));
        assert_eq!(recipe.titles[0].sha256.as_ref().as_str(), "a".repeat(64));
        assert_eq!(recipe.titles[0].signature_policy, SignaturePolicy::Nintendo);
    }

    #[test]
    fn menu_override_retargets_aliases_then_checks_language() {
        let recipe = parse(
            "menu: 4.3E\nremove: [forecast-channel]\nsetup: {language: ja}\n",
            Some(Menu::J43),
        )
        .unwrap();
        assert_eq!(recipe.menu, Menu::J43);
        assert_eq!(
            recipe
                .remove
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            ["000100024841464a", "0001000248414641"]
        );
        assert!(parse("menu: 4.3E\nsetup: {language: de}\n", Some(Menu::U43)).is_err());
    }

    #[test]
    fn rejects_unknown_legacy_and_missing_hash() {
        assert!(parse("menu: 4.3E\nlegacy: true\n", None).is_err());
        assert!(parse("menu: 4.3E\ntitles: [{path: a.wad}]\n", None).is_err());
        assert!(parse("menu: 4.3E\nunknown: true\n", None).is_err());
    }

    #[test]
    fn rejects_duplicate_keys_nested_aliases_tags_and_multiple_documents() {
        assert!(parse("menu: 4.3E\nmenu: 4.3U\n", None).is_err());
        assert!(
            parse(
                "menu: 4.3E\ndisplay: {aspect: '4:3', aspect: '16:9'}\n",
                None
            )
            .is_err()
        );
        assert!(parse("menu: 4.3E\nremove: &r [forecast-channel]\n", None).is_err());
        assert!(parse("menu: 4.3E\nremove: [!custom forecast-channel]\n", None).is_err());
        assert!(parse("menu: 4.3E\n---\nmenu: 4.3U\n", None).is_err());
    }

    #[test]
    fn rejects_implicit_types_except_lowercase_booleans() {
        for scalar in [
            "TRUE",
            "Yes",
            "no",
            "null",
            "12",
            "0x20",
            "1.5",
            "16:9",
            "2020-01-01",
        ] {
            assert!(
                parse(&format!("menu: {scalar}\n"), None).is_err(),
                "accepted {scalar}"
            );
        }
        assert!(parse("menu: 4.3E\ndisplay: {progressive: 'true'}\n", None).is_err());
        assert!(parse("menu: 4.3E\ndisplay: {progressive: true}\n", None).is_ok());
    }

    #[test]
    fn rejects_removal_collisions_and_unknown_titles() {
        assert!(
            parse(
                "menu: 4.3E\nremove: [forecast-channel, '0001000248414650']\n",
                None
            )
            .is_err()
        );
        assert!(parse("menu: 4.3E\nremove: ['0001000248414fff']\n", None).is_err());
    }

    #[test]
    fn validates_display_setup_and_priiloader_names() {
        assert!(parse("menu: 4.3E\ndisplay: {}\n", None).is_err());
        assert!(parse("menu: 4.3E\nsetup: {nickname: '😀'}\n", None).is_err());
        assert!(
            parse(
                "menu: 4.3E\npriiloader: {hacks: {fix_480p: true, invented: false}}\n",
                None
            )
            .is_err()
        );
        assert!(
            parse(
                "menu: 4.3E\npriiloader: {settings: {autoboot: unsupported}}\n",
                None
            )
            .is_err()
        );
        assert!(
            parse(
                "menu: 4.3E\npriiloader: {settings: {fix_480p: 'on'}}\n",
                None
            )
            .is_err()
        );
    }

    #[test]
    fn checked_example_is_a_valid_recipe() {
        let recipe = parse(include_str!("../../../docs/recipe.example.yaml"), None).unwrap();
        assert_eq!(recipe.menu, Menu::E43);
        assert_eq!(recipe.remove.len(), 4);
        assert!(recipe.priiloader.unwrap().hacks.unwrap().fix_480p.unwrap());
    }

    #[test]
    fn generated_schema_matches_strict_yaml_subset() {
        let schema = crate::json_schema().unwrap();
        assert!(schema.contains("^[0-9a-fA-F]{64}$"));
        assert!(schema.contains("^[0-9a-fA-F]{16}$"));
        assert!(schema.contains("\"additionalProperties\": false"));
        assert!(schema.contains("\"allow-unsigned\""));
        assert!(!schema.contains("\"null\""));
        assert!(!schema.contains("\"legacy\""));
    }

    #[test]
    fn enforces_byte_depth_and_event_limits() {
        assert!(parse_recipe(&vec![b' '; MAX_BYTES + 1], Path::new("/x.yaml"), None).is_err());
        let mut deep = String::from("menu: 4.3E\n");
        for level in 0..=MAX_DEPTH {
            deep.push_str(&format!("{}x:\n", " ".repeat(level * 2)));
        }
        deep.push_str(&format!("{}leaf: value\n", " ".repeat((MAX_DEPTH + 1) * 2)));
        assert!(parse(&deep, None).is_err());
        let many = format!("menu: 4.3E\nremove: [{}]\n", "x,".repeat(MAX_EVENTS + 2));
        assert!(parse(&many, None).is_err());
    }
}
