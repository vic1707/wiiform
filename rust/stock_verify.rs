//! Validation helpers for the composed stock builder's native reports.
//! These checks do not create or modify images; hashes and file sizes are
//! supplied by the caller after hashing the relevant input/output paths.

use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockVerifyError(pub String);

impl fmt::Display for StockVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for StockVerifyError {}

type Result<T = ()> = std::result::Result<T, StockVerifyError>;

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(StockVerifyError(message.into()))
}
fn check(ok: bool, message: impl Into<String>) -> Result {
    if ok { Ok(()) } else { fail(message) }
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| StockVerifyError(format!("missing field {key}")))
}
fn string<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| StockVerifyError(format!("{label} must be a string")))
}
fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Parse the output of `wii-nand stock-titles`, requiring sorted unique IDs.
pub fn verify_stock_title_table(value: &Value) -> Result<Vec<(String, u16)>> {
    let rows = value
        .as_array()
        .ok_or_else(|| StockVerifyError("native stock title profile must be an array".into()))?;
    if rows.is_empty() {
        return fail("native stock title profile is empty or malformed");
    }
    let mut result = Vec::with_capacity(rows.len());
    let mut previous: Option<&str> = None;
    for row in rows {
        let pair = row
            .as_array()
            .ok_or_else(|| StockVerifyError("stock title row must be a pair".into()))?;
        check(pair.len() == 2, "stock title row must be a pair")?;
        let id = string(&pair[0], "stock title ID")?;
        check(
            id.len() == 16
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && id.bytes().any(|b| b != b'0'),
            "native stock title profile contains an invalid record",
        )?;
        let version = pair[1]
            .as_u64()
            .ok_or_else(|| StockVerifyError("stock title version must be an integer".into()))?;
        check(
            version > 0 && version < u16::MAX as u64,
            "native stock title profile contains an invalid record",
        )?;
        if let Some(last) = previous {
            check(
                last < id,
                "native stock title profile is not uniquely sorted by title ID",
            )?;
        }
        previous = Some(id);
        result.push((id.to_owned(), version as u16));
    }
    Ok(result)
}

/// Expected values are derived by the caller from menu, seed/reference files,
/// and the candidate image. The input hash map must contain exactly these files.
pub struct StockExpectation<'a> {
    pub boot_policy: &'a str,
    pub region: &'a str,
    pub setting_overrides: &'a Value,
    pub reference_requested: bool,
    pub seed_sha256: &'a str,
    pub reference_sha256: Option<&'a str>,
    pub candidate_sha256: &'a str,
}

/// Bind a successful native preflight to the selected build inputs and policy.
pub fn verify_preflight(report: &Value, expected: &StockExpectation<'_>) -> Result {
    check(
        field(report, "schema_version")?.as_u64() == Some(1)
            && field(report, "host_status")?.as_str() == Some("pass"),
        "candidate failed native preflight; inspect private evidence",
    )?;
    check(
        field(report, "boot_policy")?.as_str() == Some(expected.boot_policy),
        "preflight report did not bind expected boot policy",
    )?;
    check(
        field(report, "region")?.as_str() == Some(expected.region),
        "preflight report did not bind expected region",
    )?;
    check(
        field(report, "setting_overrides")? == expected.setting_overrides,
        "preflight report did not bind expected setting overrides",
    )?;
    check(
        field(report, "reference_requested")?.as_bool() == Some(expected.reference_requested),
        "preflight report did not bind reference selection",
    )?;

    for digest in [
        Some(expected.seed_sha256),
        expected.reference_sha256,
        Some(expected.candidate_sha256),
    ]
    .into_iter()
    .flatten()
    {
        check(
            is_digest(digest),
            "expected input hash is not a lowercase SHA-256 digest",
        )?;
    }
    check(
        expected.reference_requested == expected.reference_sha256.is_some(),
        "reference hash presence does not match preflight mode",
    )?;
    let mut wanted = Map::new();
    wanted.insert("seed".into(), Value::String(expected.seed_sha256.into()));
    wanted.insert(
        "candidate".into(),
        Value::String(expected.candidate_sha256.into()),
    );
    if let Some(reference) = expected.reference_sha256 {
        wanted.insert("reference".into(), Value::String(reference.into()));
    }
    check(
        field(report, "input_sha256")?.as_object() == Some(&wanted),
        "preflight report input hashes do not match the build inputs",
    )?;

    let expected_authenticity = match expected.boot_policy {
        "stock" => "verified-stock",
        "preserve-source" => "preserved-source-unverified",
        _ => return fail("boot policy must be stock or preserve-source"),
    };
    check(
        field(report, "boot_authenticity")?.as_str() == Some(expected_authenticity),
        "preflight report did not bind requested boot authenticity policy",
    )?;
    for key in ["settings_sha256", "boot_area_sha256"] {
        let digest = string(field(report, key)?, key)?;
        check(
            is_digest(digest),
            format!("preflight report omitted a valid {key}"),
        )?;
    }
    Ok(())
}

/// Bind an inventory report to the independently hashed NAND it describes.
pub fn verify_inventory_image_binding(
    inventory: &Value,
    observed_sha256: &str,
    observed_size: u64,
) -> Result {
    check(is_digest(observed_sha256), "observed image hash is invalid")?;
    check(
        field(inventory, "kind")?.as_str() == Some("nand-inventory")
            && field(inventory, "schema_version")?.as_u64() == Some(1)
            && field(inventory, "status")?.as_str() == Some("pass"),
        "inventory is not an authenticated NAND inventory",
    )?;
    let image = field(inventory, "image")?;
    check(
        field(image, "sha256")?.as_str() == Some(observed_sha256)
            && field(image, "size")?.as_u64() == Some(observed_size),
        "inventory does not bind the observed NAND image",
    )
}

/// Bind the manifest's named stage path/hash to its authenticated inventory.
pub fn verify_stage_record(
    stage: &Value,
    expected_name: &str,
    expected_path: &str,
    expected_inventory_path: Option<&str>,
    inventory: &Value,
) -> Result {
    check(
        field(stage, "name")?.as_str() == Some(expected_name)
            && field(stage, "path")?.as_str() == Some(expected_path),
        "stage name or path differs from expectation",
    )?;
    if let Some(path) = expected_inventory_path {
        check(
            field(stage, "inventory")?.as_str() == Some(path),
            "stage inventory path differs from expectation",
        )?;
    }
    let stage_hash = string(field(stage, "sha256")?, "stage hash")?;
    check(is_digest(stage_hash), "stage hash is invalid")?;
    let image = field(inventory, "image")?;
    check(
        field(stage, "sha256")? == field(image, "sha256")?
            && field(stage, "size")? == field(image, "size")?,
        "stage record differs from its inventory",
    )
}

/// The stock manifest output and the immediately generated inventory must agree.
pub fn verify_stock_inventory_binding(stock_output: &Value, inventory: &Value) -> Result {
    check(
        field(stock_output, "sha256")? == field(field(inventory, "image")?, "sha256")?,
        "stock inventory differs from preflight",
    )
}

/// Ensure an input file still has the digest recorded before a build began.
pub fn verify_unchanged_digest(label: &str, before: &str, after: &str) -> Result {
    check(
        is_digest(before) && is_digest(after),
        format!("{label} hash is invalid"),
    )?;
    check(before == after, format!("{label} changed during the build"))
}

/// Ensure a candidate is byte-for-byte unchanged between preflight and publish.
pub fn verify_candidate_stable(preflight_sha256: &str, publish_sha256: &str) -> Result {
    verify_unchanged_digest("candidate", preflight_sha256, publish_sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stock_titles_require_sorted_unique_nonzero_ids_and_versions() {
        assert_eq!(
            verify_stock_title_table(&json!([["0000000100000002", 514], ["0000000100000004", 2]]))
                .unwrap()
                .len(),
            2
        );
        assert!(
            verify_stock_title_table(&json!([["0000000100000004", 2], ["0000000100000002", 514]]))
                .is_err()
        );
        assert!(
            verify_stock_title_table(&json!([["0000000100000002", 2], ["0000000100000002", 3]]))
                .is_err()
        );
        assert!(verify_stock_title_table(&json!([["0000000000000000", 2]])).is_err());
        assert!(verify_stock_title_table(&json!([["0000000100000002", true]])).is_err());
    }

    #[test]
    fn preflight_binds_every_selected_file_and_rejects_extras() {
        let report = json!({"schema_version":1,"host_status":"pass","boot_policy":"stock",
            "boot_authenticity":"verified-stock","region":"EUR","setting_overrides":{"AREA":"EUR"},
            "reference_requested":false,"input_sha256":{"seed":"a".repeat(64),"candidate":"b".repeat(64)},
            "settings_sha256":"c".repeat(64),"boot_area_sha256":"d".repeat(64)});
        let expected = StockExpectation {
            boot_policy: "stock",
            region: "EUR",
            setting_overrides: &json!({"AREA":"EUR"}),
            reference_requested: false,
            seed_sha256: &"a".repeat(64),
            reference_sha256: None,
            candidate_sha256: &"b".repeat(64),
        };
        assert!(verify_preflight(&report, &expected).is_ok());
        let mut forged = report.clone();
        forged["input_sha256"]["other"] = json!("e".repeat(64));
        assert!(verify_preflight(&forged, &expected).is_err());
    }

    #[test]
    fn inventory_and_stage_hashes_are_bound() {
        let inventory = json!({"kind":"nand-inventory","schema_version":1,"status":"pass",
            "image":{"sha256":"a".repeat(64),"size":42}});
        assert!(verify_inventory_image_binding(&inventory, &"a".repeat(64), 42).is_ok());
        assert!(verify_inventory_image_binding(&inventory, &"b".repeat(64), 42).is_err());
        let stage = json!({"name":"00-stock.bin","path":"/work/00-stock.bin","inventory":"/work/stock.json",
            "sha256":"a".repeat(64),"size":42});
        assert!(
            verify_stage_record(
                &stage,
                "00-stock.bin",
                "/work/00-stock.bin",
                Some("/work/stock.json"),
                &inventory
            )
            .is_ok()
        );
        assert!(
            verify_stage_record(
                &stage,
                "00-stock.bin",
                "/other/00-stock.bin",
                None,
                &inventory
            )
            .is_err()
        );
    }
}
