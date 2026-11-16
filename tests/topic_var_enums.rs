//! Topic var enums: a topic var that names a model enum (`enum = "<namespace>::<enum>"`) gets
//! that enum's codes as `DMT_E_<NAMESPACE>_<ENUM>_<VALUE>` in dmt_constants.h, once per enum and
//! as the variant resolves it; contract.json carries the reference and the codes. The tables do
//! not depend on it.

mod common;

use common::*;
use std::fs;
use std::path::{Path, PathBuf};

/// The telemetry topic of the generic model, the text the cases replace.
const TELEMETRY: &str = "template = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"";

/// The error topic of the generic model, the text the cases replace.
const ERRORS: &str = "template = \"{stage}/{model}/{duid}/errors\"\ndirection = \"pub\"";

/// @brief The telemetry topic with one var `mode` of `max_len` 1 and `attrs` (e.g. an enum).
/// @version 1
fn telemetry_var(attrs: &str) -> String {
    format!(
        "template = \"{{stage}}/{{duid}}/telemetry/{{mode}}\"\ndirection = \"pub\"\nvars = [{{ name = \"mode\", max_len = 1{attrs} }}]"
    )
}

/// @brief The error topic with one var `m` naming drive::mode too (max_len 2).
/// @version 1
fn errors_var() -> String {
    "template = \"{stage}/{model}/{duid}/errors/{m}\"\ndirection = \"pub\"\nvars = [{ name = \"m\", max_len = 2, enum = \"drive::mode\" }]".to_string()
}

/// @brief Copy `version` into `dir/model` with the telemetry var of `attrs` and `more` edits
/// (file, from, to); returns the contract path.
/// @version 1
fn model_with(dir: &Path, version: &str, attrs: &str, more: &[(&str, &str, &str)]) -> PathBuf {
    let topic = telemetry_var(attrs);
    let mut edits = vec![("contract.toml", TELEMETRY, topic.as_str())];
    edits.extend_from_slice(more);
    let model_dir = dir.join("model");
    edited_copy(version, &model_dir, &edits);
    model_dir.join("contract.toml")
}

/// @brief Generate `contract` into `out` with `extra`, asserting success.
/// @version 1
fn generated(contract: &Path, out: &Path, extra: &[&str]) {
    let (ok, stderr) = generate(contract, out, extra);
    assert!(ok, "{stderr}");
}

/// @brief The `#define DMT_E_` lines of dmt_constants.h in `gen`, whitespace collapsed.
/// @version 1
fn defines(gen: &Path) -> Vec<String> {
    let h = fs::read_to_string(gen.join("dmt_constants.h")).unwrap();
    h.lines()
        .filter(|l| l.starts_with("#define DMT_E_"))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

/// @brief Compile a use of `constant` from dmt_constants.h in `gen` as strict C11 and as C++.
/// @version 1
fn compiles_with(gen: &Path, constant: &str) {
    let src = gen.join("use_var_enum.c");
    let body = format!("#include \"dmt_constants.h\"\nint main(void) {{ return {constant}; }}\n");
    fs::write(&src, body).unwrap();
    let strict = ["-fsyntax-only", "-Wall", "-Wextra", "-pedantic", "-Werror"];
    let file = src.to_str().unwrap();
    compile(
        "cc",
        &[&strict[..], &["-std=c11", "-x", "c", file]].concat(),
    );
    compile("c++", &[&strict[..], &["-x", "c++", file]].concat());
}

#[test]
fn a_var_s_enum_is_defined_once_as_its_variant_resolves_it() {
    let dir = unique_dir("var_enum_variant");
    let errors = errors_var();
    let more = [("contract.toml", ERRORS, errors.as_str())];
    let contract = model_with(&dir, "v1_1", ", enum = \"drive::mode\"", &more);
    let (a, b) = (dir.join("sku_a"), dir.join("sku_b"));
    generated(&contract, &a, &["--variant", "sku_a"]);
    generated(&contract, &b, &["--variant", "sku_b"]);
    let of = |gen: &Path| {
        let all = defines(gen);
        let mine: Vec<String> = all
            .iter()
            .filter(|d| d.starts_with("#define DMT_E_DRIVE_MODE_"))
            .cloned()
            .collect();
        assert_eq!(all[all.len() - mine.len()..], mine[..], "last, contiguous");
        mine
    };
    let on_a = [
        "#define DMT_E_DRIVE_MODE_OFF 0",
        "#define DMT_E_DRIVE_MODE_ECO 1",
        "#define DMT_E_DRIVE_MODE_SPORT 2",
    ];
    assert_eq!(of(&a), on_a, "once although two vars name it");
    let on_b = [
        "#define DMT_E_DRIVE_MODE_OFF 0",
        "#define DMT_E_DRIVE_MODE_SAVER 1",
        "#define DMT_E_DRIVE_MODE_TURBO 3",
    ];
    assert_eq!(of(&b), on_b);
    compiles_with(&a, "DMT_E_DRIVE_MODE_SPORT + DMT_E_DRIVE_SETTINGS_MODE_ECO");
    compiles_with(&b, "DMT_E_DRIVE_MODE_TURBO");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_carries_the_reference_and_the_codes() {
    let dir = unique_dir("var_enum_json");
    let contract = model_with(&dir, "v1_1", ", enum = \"drive::mode\"", &[]);
    let gen = dir.join("gen");
    generated(&contract, &gen, &["--variant", "sku_b"]);
    let doc = contract_json(&gen);
    let topics = doc["shadow"]["topics"].as_array().unwrap();
    let telemetry = topics.iter().find(|t| t["id"] == "telemetry").unwrap();
    assert_eq!(
        telemetry["vars"],
        serde_json::json!([{"name": "mode", "max_len": 1, "enum": "drive::mode"}])
    );
    let codes = serde_json::json!({"drive::mode": {"off": 0, "saver": 1, "turbo": 3}});
    assert_eq!(doc["var_enums"], codes);
    let h = fs::read_to_string(gen.join("dmt_constants.h")).unwrap();
    assert!(h.contains(" * Topic var enums: "), "{h}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_a_var_enum_nothing_names_one() {
    let dir = unique_dir("var_enum_none");
    let contract = model_with(&dir, "v1_1", "", &[]);
    let gen = dir.join("gen");
    generated(&contract, &gen, &[]);
    let doc = contract_json(&gen);
    assert!(doc.get("var_enums").is_none(), "absent without one");
    let topics = doc["shadow"]["topics"].as_array().unwrap();
    let telemetry = topics.iter().find(|t| t["id"] == "telemetry").unwrap();
    assert_eq!(
        telemetry["vars"],
        serde_json::json!([{"name": "mode", "max_len": 1}])
    );
    let h = fs::read_to_string(gen.join("dmt_constants.h")).unwrap();
    assert!(!h.contains("Topic var enums"), "{h}");
    assert!(!defines(&gen)
        .iter()
        .any(|d| d.contains("DMT_E_DRIVE_MODE_")));
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The DSH_CONTRACT_HASH (SHA-256 of the resolved keyspace body) of dsh_tables.h in `gen`.
/// @version 1
fn contract_hash(gen: &Path) -> String {
    let h = fs::read_to_string(gen.join("dsh_tables.h")).unwrap();
    let line = h
        .lines()
        .find(|l| l.starts_with("#define DSH_CONTRACT_HASH"))
        .unwrap();
    line.split('"').nth(1).unwrap().to_string()
}

#[test]
fn the_tables_do_not_depend_on_a_var_s_enum() {
    let dir = unique_dir("var_enum_tables");
    let (with, without) = (dir.join("with"), dir.join("without"));
    let named = model_with(&dir.join("a"), "v1_1", ", enum = \"drive::mode\"", &[]);
    let plain = model_with(&dir.join("b"), "v1_1", "", &[]);
    generated(&named, &with, &[]);
    generated(&plain, &without, &[]);
    assert_same_store_tables(&with, &without);
    let (hash_with, hash_without) = (contract_hash(&with), contract_hash(&without));
    assert_ne!(
        hash_with, hash_without,
        "the keyspace carries the reference"
    );
    for f in ["dsh_tables.c", "dsh_tables.h"] {
        let read = |d: &Path, hash: &str| fs::read_to_string(d.join(f)).unwrap().replace(hash, "H");
        assert_eq!(
            read(&with, &hash_with),
            read(&without, &hash_without),
            "{f}"
        );
    }
    walk_dsh_tables(&with);
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Assert the v1_1 model with the telemetry var of `attrs` and `more` edits is refused
/// on `variant` with `expected` in the log.
/// @version 1
fn refused(tag: &str, attrs: &str, more: &[(&str, &str, &str)], variant: &str, expected: &str) {
    let dir = unique_dir(tag);
    let contract = model_with(&dir, "v1_1", attrs, more);
    let (ok, stderr) = generate(&contract, &dir.join("gen"), &["--variant", variant]);
    assert!(!ok, "'{expected}' must be refused");
    assert!(
        stderr.contains(expected),
        "expected '{expected}' in:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The drive outcome enum's last base value, the text the code cases extend.
const BUSY: (&str, &str) = ("ns/drive.toml", "busy = 2\n");

#[test]
fn an_undefined_enum_is_refused() {
    let undefined = "topic telemetry: var mode: undefined enum";
    refused(
        "ve_undef",
        ", enum = \"drive::nope\"",
        &[],
        "sku_a",
        undefined,
    );
    refused("ve_bare", ", enum = \"mode\"", &[], "sku_a", undefined);
    refused("ve_dot", ", enum = \"drive.mode\"", &[], "sku_a", undefined);
}

#[test]
fn a_negative_or_too_long_code_is_refused() {
    let outcome = ", enum = \"drive::drive_outcome\"";
    let negative = [(BUSY.0, BUSY.1, "busy = 2\nbelow = -1\n")];
    let expected =
        "topic telemetry: var mode: enum drive::drive_outcome value below has negative code -1";
    refused("ve_neg", outcome, &negative, "sku_a", expected);
    let long = [(BUSY.0, BUSY.1, "busy = 2\nlate = 10\n")];
    let expected =
        "topic telemetry: var mode: enum drive::drive_outcome value late code 10 is longer than max_len 1";
    refused("ve_long", outcome, &long, "sku_a", expected);
}

#[test]
fn a_code_only_one_variant_has_is_refused_on_that_variant() {
    // every variant is checked on every run: the refusal names sku_b, never sku_a
    let turbo = [("ns/drive.toml", "turbo = 3\n", "turbo = 30\n")];
    let dir = unique_dir("ve_variant");
    let contract = model_with(&dir, "v1_1", ", enum = \"drive::mode\"", &turbo);
    let (ok, stderr) = generate(&contract, &dir.join("gen"), &["--variant", "sku_a"]);
    let expected = "variant sku_b: topic telemetry: var mode: enum drive::mode value turbo code 30 is longer than max_len 1";
    assert!(!ok && stderr.contains(expected), "{stderr}");
    assert!(
        !stderr.contains("variant sku_a: topic telemetry"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_constant_name_checks_apply_to_a_var_s_enum() {
    let enum_of = |name: &str, values: &str| {
        let added = format!("{}[enums.{name}.values]\n{values}\n", BUSY.1);
        (", enum = \"drive::".to_string() + name + "\"", added)
    };
    let (attrs, added) = enum_of("ev", "\"a-b\" = 1");
    let expected = "drive::ev: enum constant DMT_E_DRIVE_EV_A-B is not a C identifier";
    refused(
        "ve_ident",
        &attrs,
        &[(BUSY.0, BUSY.1, &added)],
        "sku_a",
        expected,
    );
    let long = "a_value_name_long_enough_to_push_the_constant_past_the_limit_x = 1";
    let (attrs, added) = enum_of("ev", long);
    let expected = "is longer than 63 characters";
    refused(
        "ve_63",
        &attrs,
        &[(BUSY.0, BUSY.1, &added)],
        "sku_a",
        expected,
    );
    let (attrs, added) = enum_of("settings_mode", "off = 0");
    let expected = "drive::settings_mode: enum constant DMT_E_DRIVE_SETTINGS_MODE_OFF is also drive.settings.mode's";
    refused(
        "ve_twice",
        &attrs,
        &[(BUSY.0, BUSY.1, &added)],
        "sku_a",
        expected,
    );
}

#[test]
fn a_placeholder_naming_an_enum_is_refused() {
    let topic = "[[placeholders]]\nname = \"sku\"\nmax_len = 2\nenum = \"drive::mode\"\n\n[[topics]]\nid = \"by_sku\"\ntemplate = \"{stage}/{sku}/by\"\ndirection = \"pub\"\n\n[[topics]]\nid = \"telemetry\"";
    let more = [("contract.toml", "[[topics]]\nid = \"telemetry\"", topic)];
    let expected = "placeholder sku names an enum: only a topic var may";
    refused("ve_placeholder", "", &more, "sku_a", expected);
}

/// @brief Generate v1_0 with the telemetry var of `prev_attrs` into `dir/prev`, then v1_1 with
/// `attrs` and `more` against it; returns (success, stderr).
/// @version 1
fn against_released(
    dir: &Path,
    prev_attrs: &str,
    attrs: &str,
    more: &[(&str, &str, &str)],
) -> (bool, String) {
    let prev = dir.join("prev");
    generated(
        &model_with(&dir.join("p"), "v1_0", prev_attrs, &[]),
        &prev,
        &[],
    );
    let contract = model_with(&dir.join("c"), "v1_1", attrs, more);
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    generate(&contract, &dir.join("gen"), &args)
}

#[test]
fn a_minor_may_name_an_enum_for_the_first_time_and_keep_it() {
    let dir = unique_dir("ve_minor_ok");
    let tone = ", enum = \"drive::tone\"";
    let (ok, stderr) = against_released(&dir, "", tone, &[]);
    assert!(ok, "first naming: {stderr}");
    let (ok, stderr) = against_released(&dir.join("kept"), tone, tone, &[]);
    assert!(ok, "kept: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_minor_may_not_drop_rename_or_renumber_a_var_s_enum() {
    let dir = unique_dir("ve_minor_refused");
    let tone = ", enum = \"drive::tone\"";
    let changed = "topic telemetry: var mode changed or dropped its enum in a minor";
    for (tag, attrs) in [("dropped", ""), ("renamed", ", enum = \"drive::mode\"")] {
        let (ok, stderr) = against_released(&dir.join(tag), tone, attrs, &[]);
        assert!(!ok && stderr.contains(changed), "{tag}: {stderr}");
    }
    let renumbered = [(
        "ns/drive.toml",
        "chime = { value = 1,",
        "chime = { value = 2,",
    )];
    let (ok, stderr) = against_released(&dir.join("renumbered"), tone, tone, &renumbered);
    let expected =
        "enum drive::tone: a value a topic var carries was removed or changed its code in a minor";
    assert!(!ok && stderr.contains(expected), "renumbered: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}
