//! Recipe-delta checks over authenticated native NAND inventory reports.
//!
//! Native report schemas intentionally remain JSON values at this boundary;
//! this module validates the fields it consumes and reports malformed input
//! instead of panicking. Package rows are the verified `inspect-wad` reports.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

const MENU_ID: &str = "0000000100000002";
const MENU_ROOT: &str = "/title/00000001/00000002";
const SETTINGS_PATH: &str = "/title/00000001/00000002/data/loader.ini";
const ATTR_FIELDS: &[&str] = &["type", "uid", "gid", "mode", "attr", "x3"];
const MENU_OWNED: &[(&str, u64)] = &[("uid", 0x1000), ("gid", 1), ("mode", 0), ("x3", 0)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError(pub String);

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VerifyError {}

pub type VerifyResult<T = ()> = Result<T, VerifyError>;

fn fail<T>(message: impl Into<String>) -> VerifyResult<T> {
    Err(VerifyError(message.into()))
}

fn check(ok: bool, message: impl Into<String>) -> VerifyResult {
    if ok { Ok(()) } else { fail(message) }
}

fn object<'a>(value: &'a Value, label: &str) -> VerifyResult<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| VerifyError(format!("{label} must be an object")))
}

fn array<'a>(value: &'a Value, label: &str) -> VerifyResult<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| VerifyError(format!("{label} must be an array")))
}

fn field<'a>(value: &'a Value, key: &str) -> VerifyResult<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| VerifyError(format!("missing field {key}")))
}

fn string<'a>(value: &'a Value, label: &str) -> VerifyResult<&'a str> {
    value
        .as_str()
        .ok_or_else(|| VerifyError(format!("{label} must be a string")))
}

fn u64_field(value: &Value, key: &str) -> VerifyResult<u64> {
    field(value, key)?
        .as_u64()
        .ok_or_else(|| VerifyError(format!("{key} must be an unsigned integer")))
}

fn index<'a>(
    rows: &'a [Value],
    key: &str,
    label: &str,
) -> VerifyResult<BTreeMap<String, &'a Value>> {
    let mut result = BTreeMap::new();
    for row in rows {
        let value = string(field(row, key)?, key)?;
        if result.insert(value.to_owned(), row).is_some() {
            return fail(format!("duplicate {label} {key}"));
        }
    }
    Ok(result)
}

fn rows<'a>(value: &'a Value, key: &str) -> VerifyResult<&'a [Value]> {
    array(field(value, key)?, key)
}

fn select(value: &Value, keys: &[&str]) -> VerifyResult<Map<String, Value>> {
    let mut selected = Map::new();
    for key in keys {
        selected.insert((*key).to_owned(), field(value, key)?.clone());
    }
    Ok(selected)
}

fn valid_title_id(tid: &str) -> bool {
    tid.len() == 16
        && tid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn title_dir(tid: &str) -> String {
    format!("/title/{}/{}", &tid[..8], &tid[8..])
}
fn ticket_path(tid: &str) -> String {
    format!("/ticket/{}/{}.tik", &tid[..8], &tid[8..])
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn verify_file(
    entries: &BTreeMap<String, &Value>,
    path: &str,
    digest: &str,
    size: Option<u64>,
) -> VerifyResult {
    let row = entries
        .get(path)
        .ok_or_else(|| VerifyError(format!("missing file at {path}")))?;
    let size_matches = match size {
        Some(wanted) => field(row, "size")?.as_u64() == Some(wanted),
        None => true,
    };
    check(
        field(row, "type")?.as_str() == Some("file")
            && field(row, "sha256")?.as_str() == Some(digest)
            && size_matches,
        format!("file mismatch at {path}"),
    )
}

fn verify_reported_file(
    entries: &BTreeMap<String, &Value>,
    row: &Value,
    attr: Option<u64>,
) -> VerifyResult {
    let path = string(field(row, "path")?, "file path")?;
    let size = u64_field(row, "size")?;
    let digest = string(field(row, "sha256")?, "file sha256")?;
    check(
        size > 0 && is_sha256(digest),
        format!("invalid file evidence at {path}"),
    )?;
    verify_file(entries, path, digest, Some(size))?;
    if let Some(attr) = attr {
        let mut keys: Vec<&str> = MENU_OWNED.iter().map(|(key, _)| *key).collect();
        keys.push("attr");
        let actual = select(entries[path], &keys)?;
        let mut expected = Map::new();
        for (key, value) in MENU_OWNED {
            expected.insert((*key).into(), Value::from(*value));
        }
        expected.insert("attr".into(), Value::from(attr));
        check(
            actual == expected,
            format!("unexpected menu file metadata at {path}"),
        )?;
    }
    Ok(())
}

fn map_rows<'a>(inventory: &'a Value, name: &str) -> VerifyResult<&'a [Value]> {
    let map = field(field(inventory, "maps")?, name)?;
    check(
        field(map, "present")?.as_bool() == Some(true),
        format!("missing {name} map"),
    )?;
    rows(map, "records")
}

fn verify_maps(
    before: &Value,
    after: &Value,
    packages: &[Value],
) -> VerifyResult<(BTreeMap<String, u64>, Vec<Value>)> {
    let old_uids = map_rows(before, "uid")?;
    let old_shared = map_rows(before, "shared")?;
    let mut uids: BTreeMap<String, Value> = index(old_uids, "title_id", "inventory")?
        .into_iter()
        .map(|(k, v)| (k, v.clone()))
        .collect();
    let mut shared_by_hash: BTreeMap<String, Value> = index(old_shared, "sha1", "inventory")?
        .into_iter()
        .map(|(k, v)| (k, v.clone()))
        .collect();
    let mut shared_order: Vec<String> = old_shared
        .iter()
        .map(|row| string(field(row, "sha1")?, "sha1").map(str::to_owned))
        .collect::<VerifyResult<_>>()?;

    for package in packages {
        let tid = string(field(package, "title_id")?, "title_id")?;
        if !uids.contains_key(tid) {
            let next = 0x1000_u64
                .checked_add(uids.len() as u64)
                .ok_or_else(|| VerifyError("UID overflow".into()))?;
            uids.insert(
                tid.to_owned(),
                serde_json::json!({"title_id": tid, "uid": next}),
            );
        }
        for content in array(field(package, "contents")?, "package contents")? {
            if u64_field(content, "type")? == 0x8001 {
                let digest = string(field(content, "sha1")?, "shared content sha1")?;
                if !shared_by_hash.contains_key(digest) {
                    let record = serde_json::json!({"name": format!("{:08x}", shared_by_hash.len()), "sha1": digest});
                    shared_by_hash.insert(digest.to_owned(), record);
                    shared_order.push(digest.to_owned());
                }
            }
        }
    }

    let after_uids = index(map_rows(after, "uid")?, "title_id", "inventory")?;
    let expected_uids: BTreeMap<String, Value> = uids;
    let actual_uids: BTreeMap<String, Value> = after_uids
        .into_iter()
        .map(|(k, v)| (k, v.clone()))
        .collect();
    check(
        actual_uids == expected_uids,
        "UID map changes differ from the recipe",
    )?;
    let expected_shared: Vec<Value> = shared_order
        .iter()
        .map(|key| shared_by_hash[key].clone())
        .collect();
    check(
        map_rows(after, "shared")? == expected_shared.as_slice(),
        "shared-content map changes differ from the recipe",
    )?;

    let mut sizes = BTreeMap::new();
    if expected_uids.len() != old_uids.len() {
        sizes.insert("/sys/uid.sys".into(), (expected_uids.len() as u64) * 12);
    }
    if shared_by_hash.len() != old_shared.len() {
        sizes.insert(
            "/shared1/content.map".into(),
            (shared_by_hash.len() as u64) * 28,
        );
    }
    Ok((sizes, expected_shared))
}

fn verify_packages(
    actual: &BTreeMap<String, &Value>,
    packages: &[Value],
    shared: &[Value],
) -> VerifyResult<BTreeSet<String>> {
    let mut paths = BTreeSet::new();
    let shared_map: BTreeMap<String, &Value> = index(shared, "sha1", "shared-content")?;
    for package in packages {
        let tid = string(field(package, "title_id")?, "title_id")?;
        check(valid_title_id(tid), "invalid package title ID")?;
        let root = title_dir(tid);
        let content = format!("{root}/content");
        let ticket = ticket_path(tid);
        let ticket_parent = Path::new(&ticket)
            .parent()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let root_parent = Path::new(&root)
            .parent()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for path in [
            "/title".to_owned(),
            root_parent,
            root.clone(),
            format!("{root}/data"),
            content.clone(),
            "/ticket".to_owned(),
            ticket_parent,
        ] {
            let row = actual
                .get(&path)
                .ok_or_else(|| VerifyError(format!("missing expected directory at {path}")))?;
            check(
                field(row, "type")?.as_str() == Some("directory"),
                format!("expected a directory at {path}"),
            )?;
            paths.insert(path);
        }
        let tmd = format!("{content}/title.tmd");
        verify_file(
            actual,
            &ticket,
            string(
                field(package, "prepared_ticket_sha256")?,
                "prepared ticket hash",
            )?,
            None,
        )?;
        verify_file(
            actual,
            &tmd,
            string(field(package, "tmd_sha256")?, "TMD hash")?,
            None,
        )?;
        paths.insert(ticket);
        paths.insert(tmd);
        for row in array(field(package, "contents")?, "package contents")? {
            let ctype = u64_field(row, "type")?;
            let path = match ctype {
                1 => format!(
                    "{content}/{}.app",
                    string(field(row, "cid")?, "content CID")?
                ),
                0x8001 => {
                    let digest = string(field(row, "sha1")?, "shared content sha1")?;
                    let map_row = shared_map
                        .get(digest)
                        .ok_or_else(|| VerifyError(format!("missing shared content {digest}")))?;
                    format!(
                        "/shared1/{}.app",
                        string(field(map_row, "name")?, "shared content name")?
                    )
                }
                _ => return fail(format!("unsupported content type for {tid}")),
            };
            verify_file(
                actual,
                &path,
                string(field(row, "sha256")?, "content hash")?,
                Some(u64_field(row, "size")?),
            )?;
            paths.insert(path);
        }
    }
    Ok(paths)
}

/// Values derived from the recipe and WAD inspection reports for one stage.
#[derive(Default)]
pub struct PreparedExpectations<'a> {
    pub removals: &'a [String],
    pub packages: &'a [Value],
    pub display: Option<&'a Value>,
    pub display_report: Option<&'a Value>,
    pub setup: Option<&'a Value>,
}

/// Assert the complete expected title, map, file, settings, and identity delta.
pub fn verify_inventory_delta(
    before: &Value,
    after: &Value,
    expected: &PreparedExpectations<'_>,
) -> VerifyResult {
    check(
        field(before, "identity")? == field(after, "identity")?,
        "console identity changed",
    )?;
    let mut titles = index(rows(before, "titles")?, "title_id", "inventory")?
        .into_iter()
        .map(|(k, v)| (k, v.clone()))
        .collect::<BTreeMap<_, _>>();
    for tid in expected.removals {
        check(valid_title_id(tid), "invalid removal title ID")?;
        check(
            titles.remove(tid).is_some(),
            format!("requested removal {tid} is missing"),
        )?;
    }
    for package in expected.packages {
        let tid = string(field(package, "title_id")?, "title_id")?;
        check(
            !titles.contains_key(tid),
            format!("requested title {tid} already exists"),
        )?;
        titles.insert(
            tid.to_owned(),
            serde_json::json!({
                "title_id": tid,
                "path": format!("{}/content/title.tmd", title_dir(tid)),
                "version": field(package, "version")?,
                "ios": field(package, "required_ios")?,
                "content_count": array(field(package, "contents")?, "package contents")?.len(),
                "tmd_sha256": field(package, "tmd_sha256")?,
            }),
        );
    }
    let actual_titles: BTreeMap<String, Value> =
        index(rows(after, "titles")?, "title_id", "inventory")?
            .into_iter()
            .map(|(k, v)| (k, v.clone()))
            .collect();
    check(actual_titles.keys().eq(titles.keys()), "title set mismatch")?;
    check(
        actual_titles == titles,
        "title metadata differs from the recipe",
    )?;

    let (sizes, shared) = verify_maps(before, after, expected.packages)?;
    let old = index(rows(before, "entries")?, "path", "inventory")?;
    let actual = index(rows(after, "entries")?, "path", "inventory")?;
    let mut removed: BTreeSet<String> = expected
        .removals
        .iter()
        .map(|tid| ticket_path(tid))
        .collect();
    let content_dirs: Vec<String> = expected
        .removals
        .iter()
        .map(|tid| format!("{}/content", title_dir(tid)))
        .collect();
    removed.extend(content_dirs.iter().cloned());
    let retained: BTreeSet<String> = old
        .keys()
        .filter(|path| {
            !removed.contains(*path)
                && !content_dirs
                    .iter()
                    .any(|dir| path.starts_with(&format!("{dir}/")))
        })
        .cloned()
        .collect();
    for path in &retained {
        let mut fields = ATTR_FIELDS.to_vec();
        if !sizes.contains_key(path) {
            fields.extend(["size", "sha256"]);
        }
        check(
            select(old[path], &fields)?
                == select(
                    actual
                        .get(path)
                        .ok_or_else(|| VerifyError(format!("missing retained path {path}")))?,
                    &fields,
                )?,
            format!("retained entry changed at {path}"),
        )?;
        if let Some(size) = sizes.get(path) {
            check(
                u64_field(actual[path], "size")? == *size,
                format!("map size mismatch at {path}"),
            )?;
        }
    }
    let mut wanted = retained;
    wanted.extend(verify_packages(&actual, expected.packages, &shared)?);

    if expected.display.is_some() || expected.setup.is_some() {
        let path = "/shared2/sys/SYSCONF";
        check(
            !old.contains_key(path),
            "SYSCONF initialization requires a fresh image",
        )?;
        let report = expected
            .display_report
            .ok_or_else(|| VerifyError("settings report is required".into()))?;
        let evidence = serde_json::json!({"path": path, "size": 0x4000, "sha256": field(report, "sysconf_sha256")?});
        verify_reported_file(&actual, &evidence, Some(0xfd))?;
        check(
            field(report, "initialized")?.as_bool() == Some(true)
                && field(report, "changed")?.as_bool() == Some(true),
            "settings report did not initialize SYSCONF",
        )?;
        if let Some(display) = expected.display {
            for (key, report_key) in [
                ("aspect", "aspect_ratio"),
                ("progressive", "progressive_scan"),
                ("pal60", "pal60"),
            ] {
                if let Some(value) = display.get(key) {
                    check(
                        field(report, report_key)? == value,
                        format!("display setting {key} differs"),
                    )?;
                }
            }
        }
        if let Some(setup) = expected.setup {
            check(
                field(report, "setup_complete")?.as_bool() == Some(true),
                "imported Wii setup is incomplete",
            )?;
            for (key, value) in object(setup, "setup")? {
                check(
                    field(report, key)? == value,
                    format!("setup setting {key} differs"),
                )?;
            }
        }
        wanted.insert(path.to_owned());
    }
    let actual_paths: BTreeSet<String> = actual.keys().cloned().collect();
    check(
        wanted == actual_paths,
        "filesystem path changes differ from the recipe",
    )
}

fn verify_retained(
    old: &BTreeMap<String, &Value>,
    new: &BTreeMap<String, &Value>,
    changes: &[String],
    additions: &[String],
) -> VerifyResult {
    let old_keys: BTreeSet<&String> = old.keys().collect();
    let new_keys: BTreeSet<&String> = new.keys().collect();
    let added: BTreeSet<String> = new_keys
        .difference(&old_keys)
        .map(|s| (*s).clone())
        .collect();
    check(
        added == additions.iter().cloned().collect() && old_keys.is_subset(&new_keys),
        "unexpected Priiloader paths",
    )?;
    for path in old.keys().filter(|path| !changes.contains(path)) {
        check(
            old[path] == new[path],
            format!("Priiloader changed unrelated entry {path}"),
        )?;
    }
    Ok(())
}

/// Validate Priiloader's exact installation footprint and preserved menu identity.
pub fn verify_priiloader_install(before: &Value, after: &Value, report: &Value) -> VerifyResult {
    check(
        field(report, "status")?.as_str() == Some("installed")
            && field(report, "release")?.as_str() == Some("0.10.0"),
        "unexpected Priiloader release",
    )?;
    for key in ["identity", "maps"] {
        check(
            field(before, key)? == field(after, key)?,
            format!("Priiloader changed {key}"),
        )?;
    }
    let files = object(field(report, "files")?, "Priiloader files")?;
    let roles: BTreeSet<&str> = files.keys().map(String::as_str).collect();
    check(
        roles
            == ["boot_app", "tmd", "backup_app", "backup_tmd", "ticket"]
                .into_iter()
                .collect(),
        "unexpected Priiloader file set",
    )?;
    let content = format!("{MENU_ROOT}/content/");
    let boot = string(field(&files["boot_app"], "path")?, "boot app path")?;
    let tmd = format!("{MENU_ROOT}/content/title.tmd");
    let cid = boot
        .strip_prefix(&content)
        .and_then(|v| v.strip_suffix(".app"))
        .ok_or_else(|| VerifyError("invalid boot app path".into()))?;
    check(
        cid.len() == 8
            && cid.starts_with('0')
            && cid
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            && boot == format!("{content}{cid}.app"),
        "invalid boot app path",
    )?;
    let backups = BTreeMap::from([
        (
            "backup_app",
            (format!("{content}1{}.app", &cid[1..]), boot.to_owned()),
        ),
        (
            "backup_tmd",
            (format!("{content}title_or.tmd"), tmd.clone()),
        ),
        ("ticket", (format!("{content}ticket"), ticket_path(MENU_ID))),
    ]);
    check(
        field(&files["tmd"], "path")?.as_str() == Some(tmd.as_str()),
        "unexpected active TMD path",
    )?;
    let old = index(rows(before, "entries")?, "path", "inventory")?;
    let new = index(rows(after, "entries")?, "path", "inventory")?;
    let mut additions = Vec::new();
    for (role, (path, source)) in &backups {
        let row = &files[*role];
        check(
            field(row, "path")?.as_str() == Some(path.as_str())
                && select(row, &["size", "sha256"])?
                    == select(
                        old.get(source).ok_or_else(|| {
                            VerifyError(format!("missing original Priiloader source {source}"))
                        })?,
                        &["size", "sha256"],
                    )?,
            format!("Priiloader {role} differs from the original"),
        )?;
        additions.push(path.clone());
    }
    verify_retained(&old, &new, &[boot.to_owned(), tmd.clone()], &additions)?;
    for (role, row) in files {
        verify_reported_file(
            &new,
            row,
            Some(if role == "tmd" || role == "backup_tmd" {
                0xf1
            } else {
                0xfd
            }),
        )?;
    }
    check(
        field(&files["boot_app"], "sha256")? == field(report, "app_sha256")?
            && field(
                old.get(boot)
                    .ok_or_else(|| VerifyError("missing original boot app".into()))?,
                "sha256",
            )? != field(
                new.get(boot)
                    .ok_or_else(|| VerifyError("missing installed boot app".into()))?,
                "sha256",
            )?
            && field(
                old.get(&tmd)
                    .ok_or_else(|| VerifyError("missing original TMD".into()))?,
                "sha256",
            )? != field(
                new.get(&tmd)
                    .ok_or_else(|| VerifyError("missing installed TMD".into()))?,
                "sha256",
            )?,
        "Priiloader did not replace the boot app and TMD as reported",
    )?;
    let mut titles: BTreeMap<String, Value> =
        index(rows(before, "titles")?, "title_id", "inventory")?
            .into_iter()
            .map(|(k, v)| (k, v.clone()))
            .collect();
    check(
        u64_field(
            titles
                .get(MENU_ID)
                .ok_or_else(|| VerifyError("System Menu title is missing".into()))?,
            "version",
        )? == u64_field(report, "menu_version")?,
        "Priiloader changed the menu version",
    )?;
    let row = titles.get_mut(MENU_ID).unwrap();
    row.as_object_mut()
        .unwrap()
        .insert("tmd_sha256".into(), field(&files["tmd"], "sha256")?.clone());
    let actual: BTreeMap<String, Value> = index(rows(after, "titles")?, "title_id", "inventory")?
        .into_iter()
        .map(|(k, v)| (k, v.clone()))
        .collect();
    check(actual == titles, "unexpected Priiloader title changes")
}

/// Validate Priiloader settings and hack readback against a recipe-derived expectation.
pub fn verify_priiloader_config(
    before: &Value,
    after: &Value,
    inspection: &Value,
    report: &Value,
    settings: &Value,
    hacks: &Value,
    menu: &str,
) -> VerifyResult {
    for key in ["identity", "maps", "titles"] {
        check(
            field(before, key)? == field(after, key)?,
            format!("Priiloader configuration changed {key}"),
        )?;
    }
    check(
        field(inspection, "image_sha256")? == field(field(after, "image")?, "sha256")?,
        "Priiloader inspection is for another image",
    )?;
    let old = index(rows(before, "entries")?, "path", "inventory")?;
    let new = index(rows(after, "entries")?, "path", "inventory")?;
    let settings_obj = object(settings, "Priiloader settings")?;
    let hacks_obj = object(hacks, "Priiloader hacks")?;
    let mut added = Vec::new();
    if !settings_obj.is_empty() || !hacks_obj.is_empty() {
        let observed = field(inspection, "settings")?;
        check(
            observed == field(report, "settings")?
                && field(observed, "status")?.as_str() == Some("decoded")
                && field(observed, "format")?.as_str() == Some("0.10.0"),
            "Priiloader settings readback disagrees with the writer",
        )?;
        let values = object(field(observed, "values")?, "Priiloader setting values")?;
        for (key, value) in settings_obj {
            check(
                values.get(key) == Some(value),
                "Priiloader settings differ from the recipe",
            )?;
        }
        let row = field(field(inspection, "artifacts")?, "loader.ini")?;
        check(
            field(row, "path")?.as_str() == Some(SETTINGS_PATH) && u64_field(row, "size")? == 32,
            "invalid Priiloader settings file",
        )?;
        verify_reported_file(&new, row, Some(0xfd))?;
        added.push(SETTINGS_PATH.to_owned());
    }
    if !hacks_obj.is_empty() {
        let observed = object(field(inspection, "hacks")?, "Priiloader hacks readback")?;
        let reported = object(field(report, "hacks")?, "Priiloader hacks report")?;
        let mut expected = Map::new();
        for (alias, enabled) in hacks_obj {
            let title = hack_title(alias, menu)?;
            expected.insert(title, enabled.clone());
        }
        check(
            reported == &expected,
            "Priiloader hack report differs from the recipe",
        )?;
        for (title, enabled) in &expected {
            check(
                observed.get(title) == Some(enabled),
                "Priiloader hack readback differs from the recipe",
            )?;
        }
        for path in [
            "/title/00000001/00000002/data/hackshas.ini",
            "/title/00000001/00000002/data/hacksh_s.ini",
        ] {
            let name = Path::new(path).file_name().unwrap().to_string_lossy();
            let row = field(field(inspection, "artifacts")?, &name)?;
            check(
                field(row, "path")?.as_str() == Some(path),
                "invalid Priiloader hack file path",
            )?;
            verify_reported_file(&new, row, Some(0xfd))?;
            added.push(path.to_owned());
        }
    }
    verify_retained(&old, &new, &[], &added)
}

fn hack_title<'a>(alias: &str, menu: &str) -> VerifyResult<String> {
    const HACKS: &[(&str, &str)] = &[
        ("fix_480p", "480p graphics fix in system menu"),
        ("block_online_updates", "Block Online Updates"),
        ("block_disc_updates", "Block Disc Updates"),
        ("block_disc_autoboot", "Block Disc Autoboot"),
        ("auto_press_a", "Auto-Press A at Health Screen"),
        ("remove_deflicker", "Remove Deflicker"),
        ("move_disc_channel", "Move Disc Channel"),
        (
            "remove_nocopy_save_protection",
            "Remove NoCopy Save File Protection",
        ),
        (
            "replace_health_screen",
            "Replace Health Screen with Backmenu",
        ),
        ("no_background_music", "No System Menu Background Music"),
        ("wiimmfi", "Wiimmfi Patch v4"),
        ("region_free_everything", "Region Free EVERYTHING"),
    ];
    if alias == "region_free_everything" && menu == "4.3J" {
        return Ok("Region Free EVERYTHING JP".into());
    }
    HACKS
        .iter()
        .find(|(key, _)| *key == alias)
        .map(|(_, title)| (*title).to_owned())
        .ok_or_else(|| VerifyError(format!("unknown Priiloader hack alias {alias}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(path: &str, digest: &str, size: u64) -> Value {
        json!({"path":path,"type":"file","uid":4096,"gid":1,"mode":0,"attr":253,"x3":0,
              "size":size,"sha256":digest})
    }

    #[test]
    fn channel_removal_requires_an_existing_title_and_only_removes_its_ticket() {
        let tid = "0001000248414641";
        let before = json!({
            "identity":{"keys_sha256":"a".repeat(64)},
            "titles":[{"title_id":tid,"version":1}],
            "entries":[file(&ticket_path(tid), &"b".repeat(64), 32)],
            "maps":{"uid":{"present":true,"records":[]},"shared":{"present":true,"records":[]}}
        });
        let after = json!({"identity":before["identity"],"titles":[],"entries":[],
            "maps":{"uid":{"present":true,"records":[]},"shared":{"present":true,"records":[]}}});
        let removals = [tid.to_owned()];
        let expected = PreparedExpectations {
            removals: &removals,
            ..Default::default()
        };
        assert!(verify_inventory_delta(&before, &after, &expected).is_ok());

        let extra = json!({"identity":before["identity"],"titles":[],
            "entries":[file("/unexpected", &"c".repeat(64), 1)],
            "maps":{"uid":{"present":true,"records":[]},"shared":{"present":true,"records":[]}}});
        assert!(verify_inventory_delta(&before, &extra, &expected).is_err());
    }

    #[test]
    fn wad_install_binds_ticket_tmd_content_uid_and_shared_map() {
        let tid = "000100014e414350";
        let menu = json!({"title_id":MENU_ID,"path":format!("{MENU_ROOT}/content/title.tmd"),
            "version":514,"ios":80,"content_count":1,"tmd_sha256":"f".repeat(64)});
        let package = json!({"title_id":tid,"version":1,"required_ios":58,"tmd_sha256":"1".repeat(64),
        "prepared_ticket_sha256":"2".repeat(64),"contents":[
            {"type":1,"cid":"1234abcd","sha256":"3".repeat(64),"size":4},
            {"type":32769,"sha1":"0123456789abcdef0123456789abcdef01234567",
             "sha256":"4".repeat(64),"size":5}
        ]});
        let before = json!({"identity":{"keys_sha256":"a".repeat(64)},"titles":[menu],
            "maps":{"uid":{"present":true,"records":[{"title_id":MENU_ID,"uid":4096}]},
                   "shared":{"present":true,"records":[]}},
            "entries":[file("/sys/uid.sys",&"5".repeat(64),12),file("/shared1/content.map",&"6".repeat(64),0)]});
        let expected_title = json!({"title_id":tid,"path":format!("{}/content/title.tmd",title_dir(tid)),
            "version":1,"ios":58,"content_count":2,"tmd_sha256":"1".repeat(64)});
        let root = title_dir(tid);
        let content = format!("{root}/content");
        let ticket = ticket_path(tid);
        let after = json!({"identity":before["identity"],"titles":[before["titles"][0],expected_title],
        "maps":{"uid":{"present":true,"records":[{"title_id":MENU_ID,"uid":4096},{"title_id":tid,"uid":4097}]},
               "shared":{"present":true,"records":[{"name":"00000000","sha1":"0123456789abcdef0123456789abcdef01234567"}]}},
        "entries":[
            file("/sys/uid.sys",&"7".repeat(64),24),file("/shared1/content.map",&"8".repeat(64),28),
            {"path":"/title","type":"directory"},
            {"path":"/title/00010001","type":"directory"},
            {"path":root,"type":"directory"},
            {"path":format!("{root}/data"),"type":"directory"},
            {"path":content,"type":"directory"},
            {"path":"/ticket","type":"directory"},
            {"path":"/ticket/00010001","type":"directory"},
            file(&ticket,&"2".repeat(64),32),
            file(&format!("{content}/title.tmd"),&"1".repeat(64),64),
            file(&format!("{content}/1234abcd.app"),&"3".repeat(64),4),
            file("/shared1/00000000.app",&"4".repeat(64),5)
        ]});
        let packages = [package];
        let expected = PreparedExpectations {
            packages: &packages,
            ..Default::default()
        };
        assert!(verify_inventory_delta(&before, &after, &expected).is_ok());

        let mut forged = after.clone();
        forged["titles"][1]["ios"] = json!(9);
        assert!(verify_inventory_delta(&before, &forged, &expected).is_err());
    }
}
