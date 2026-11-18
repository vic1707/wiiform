// SPDX-License-Identifier: GPL-2.0-only
//! Editor help the derived JSON Schema cannot give on its own.
use schemars::{JsonSchema, Schema};
use serde_json::Value;

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
