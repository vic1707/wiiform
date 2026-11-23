// SPDX-License-Identifier: GPL-2.0-only
//! Editor help the derived JSON Schema cannot give on its own.
use schemars::{JsonSchema, Schema};
use serde_json::{Value, json};

use crate::nand::{
    menu::Menu,
    setup::{CountryCode, Language},
};

/// Appends to a list key's description every value `T` accepts, with the
/// first sentence of each value's own description. Editors show a key's
/// description on hover, but a value's only once it is typed.
pub(crate) fn list_values<T: JsonSchema>(schema: &mut Schema) {
    let values = schemars::schema_for!(T);
    let variants = (values.get("oneOf").and_then(Value::as_array))
        .unwrap_or_else(|| panic!("{} is not an enum of described values", T::schema_name()));
    let lines: Vec<String> = (variants.iter())
        .map(|variant| {
            let name = variant.get("const").and_then(Value::as_str);
            let help = variant.get("description").and_then(Value::as_str);
            let (Some(name), Some(help)) = (name, help) else {
                panic!(
                    "every {} value needs a name and a description",
                    T::schema_name()
                );
            };
            format!("- {name}: {}", first_sentence(help))
        })
        .collect();
    let description = schema.get("description").and_then(Value::as_str);
    let description = format!(
        "{}\n\nValues:\n{}",
        description.unwrap_or_default(),
        lines.join("\n")
    );
    schema.insert("description".into(), description.into());
}

/// The rules between `nand` keys that the schema states, for editors. The recipe
/// checks the same rules when it is read: a schema is not run by the CLI.
pub(crate) fn nand_rules(schema: &mut Schema) {
    let mut rules = vec![
        // wiimotes is written into SYSCONF, which setup makes.
        json!({
            "if": {"required": ["wiimotes"]},
            "then": {"required": ["setup"], "properties": {"setup": {"type": "object"}}}
        }),
        // bootmii_ios needs the bootmii key.
        json!({
            "if": {
                "required": ["priiloader"],
                "properties": {"priiloader": {"type": "object", "required": ["autoboot"], "properties": {"autoboot": {"const": "bootmii_ios"}}}}
            },
            "then": {"required": ["bootmii"]}
        }),
        // rvloader needs a priiloader section, and that section leaves out
        // autoboot and return_to.
        json!({"dependentRequired": {"rvloader": ["priiloader"]}}),
        json!({
            "if": {"required": ["rvloader"], "properties": {"rvloader": {"type": "object"}}},
            "then": {"properties": {"priiloader": {"properties": {"autoboot": false, "return_to": false}}}}
        }),
    ];
    // rvloader.vga needs setup.display.progressive, and pal60 on 4.3E.
    let vga = json!({"required": ["rvloader"], "properties": {"rvloader": {"type": "object", "required": ["vga"], "properties": {"vga": {"const": true}}}}});
    let vga_on_e43 = json!({
        "required": ["rvloader", "menu"],
        "properties": {"menu": {"const": Menu::E43.to_string()}, "rvloader": {"type": "object", "required": ["vga"], "properties": {"vga": {"const": true}}}}
    });
    rules.push(display_rule(vga, "progressive"));
    rules.push(display_rule(vga_on_e43, "pal60"));
    // Each menu takes its own languages and countries.
    for menu in [Menu::E43, Menu::U43, Menu::J43] {
        let languages: Vec<Value> = (Language::ALL.iter())
            .filter(|language| menu.allows_language(**language))
            .filter_map(|language| serde_json::to_value(language).ok())
            .collect();
        let countries: Vec<Value> = (CountryCode::ALL.iter())
            .filter(|country| menu.allows_country(**country))
            .map(|country| json!(country.as_alpha2()))
            .collect();
        rules.push(json!({
            "if": {"required": ["menu"], "properties": {"menu": {"const": menu.to_string()}}},
            "then": {"properties": {"setup": {"properties": {
                "language": {"enum": languages},
                "country": {"enum": countries}
            }}}}
        }));
    }
    schema.insert("allOf".into(), Value::Array(rules));
}

/// A rule that, when `condition` holds, needs `setup.display.<flag>` to be true.
fn display_rule(condition: Value, flag: &str) -> Value {
    json!({
        "if": condition,
        "then": {"required": ["setup"], "properties": {"setup": {
            "type": "object",
            "required": ["display"],
            "properties": {"display": {
                "type": "object",
                "required": [flag],
                "properties": {flag: {"const": true}}
            }}
        }}}
    })
}

/// Up to the first full stop that ends a sentence, on one line.
fn first_sentence(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.find(". ") {
        Some(end) => text[..=end].to_owned(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::first_sentence;

    #[test]
    fn keeps_the_first_sentence_on_one_line() {
        assert_eq!(
            first_sentence("Lets it copy\nto the SD card. It does not move them."),
            "Lets it copy to the SD card."
        );
        assert_eq!(
            first_sentence("Re-enables it on 4.3. Not needed."),
            "Re-enables it on 4.3."
        );
        assert_eq!(first_sentence("One sentence."), "One sentence.");
    }
}
