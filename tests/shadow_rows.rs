//! Forms of the shadow mapping beyond one path per key and one command per row: declared
//! outbound many-to-one (report_as), per-variant paths, argument forms and commandless rows.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Keys added to the command class of the 1.0 model for the argument forms.
const ARG_KEYS: &str = "id = \"map_payload\"\nkey_index = 9\ntype = \"string\"\nmax_size = 512\nintroduced_in = \"1.0\"\ndoc = \"Base64 text of a map request, passed through as text.\"\n\n[[classes.keys]]\nid = \"drive_mode\"\nkey_index = 10\ntype = \"uint8\"\nenum = \"mode\"\nintroduced_in = \"1.0\"\n\n[[classes.keys]]\nid = \"plan_blob\"\nkey_index = 11\ntype = \"binary\"\nmax_size = 96\nintroduced_in = \"1.0\"\n\n[[classes.keys]]\nid = \"path_from\"\nkey_index = 12\ntype = \"int16\"\nintroduced_in = \"1.0\"";

/// The go row of the 1.0 model.
const GO_ROW: &str =
    "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"";

/// The stop row of the 1.0 model.
const STOP_ROW: &str =
    "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"";

/// @brief Generate the edited 1.0 model for `variant` and return the output directory.
/// @version 1
fn generated(
    dir: &Path,
    tag: &str,
    variant: &str,
    edits: &[(&str, &str, &str)],
) -> std::path::PathBuf {
    let case = dir.join(tag);
    edited_copy("v1_0", &case, edits);
    let out = case.join(variant);
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
    assert!(ok, "{tag} ({variant}): {stderr}");
    out
}

/// @brief A parsed generated JSON file.
/// @version 1
fn read(gen: &Path, file: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(gen.join(file)).unwrap()).unwrap()
}

/// @brief The wire rows of command `name` in a keyspace document.
/// @version 1
fn wire_rows(doc: &Value, name: &str) -> Vec<Value> {
    doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap()["wire"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn two_codes_report_one_wire_string_when_one_declares_it() {
    let dir = unique_dir("rows_report_as");
    for (tag, target) in [("by_name", "boost"), ("by_wire", "high")] {
        let line = format!("boost = {{ value = 2, wire = \"high\", accept = [\"max\"] }}\nboosted = {{ value = 3, report_as = \"{target}\", accept = [\"turbo\"] }}");
        let anchor = "boost = { value = 2, wire = \"high\", accept = [\"max\"] }";
        let gen = generated(&dir, tag, "sku_b", &[("ns/drive.toml", anchor, &line)]);
        let fan = &read(&gen, "keyspace.sku_b.json")["enums"][0];
        assert_eq!(fan["name"], "drive::fan");
        assert_eq!(
            fan["values"][3],
            json!({"code": 3, "name": "boosted", "wire": "high", "report_as": "boost", "accept": ["turbo"]}),
            "{tag}"
        );
        assert!(fan["values"][2].get("report_as").is_none());
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_wire_string_names_the_member_that_reports_it_itself() {
    let dir = unique_dir("rows_report_by_wire");
    let anchor = "boost = { value = 2, wire = \"high\", accept = [\"max\"] }";
    let line = format!("{anchor}\naaa = {{ value = 3, report_as = \"boost\" }}\nbbb = {{ value = 4, report_as = \"high\" }}");
    let gen = generated(&dir, "ab", "sku_b", &[("ns/drive.toml", anchor, &line)]);
    let fan = &read(&gen, "keyspace.sku_b.json")["enums"][0];
    assert_eq!(fan["values"][4]["name"], "bbb");
    assert_eq!(
        fan["values"][4]["report_as"], "boost",
        "not aaa, which reports high too"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_path_can_exist_on_one_variant() {
    let dir = unique_dir("rows_variant_paths");
    let paths = "shadow = [{ path = \"state.docked\", variants = [\"sku_a\"] }, { path = \"state.isDocked\", variants = [\"sku_b\"] }]\nforced";
    let edits = [("ns/drive.toml", "shadow = \"state.docked\"\nforced", paths)];
    for (variant, path) in [("sku_a", "state.docked"), ("sku_b", "state.isDocked")] {
        let gen = generated(&dir, variant, variant, &edits);
        let key = key_json(&gen, "drive.state.docked");
        assert_eq!(key["shadow"], json!([path]), "{variant}");
        assert_eq!(
            key["shadow_forms"][0]["variants"],
            json!([variant]),
            "{variant}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The edits that add the argument keys and every argument form to the 1.0 model.
/// @version 1
fn argument_edits() -> Vec<(&'static str, String, String)> {
    let anchor = "id = \"map_payload\"\nkey_index = 9\ntype = \"string\"\nmax_size = 512\nintroduced_in = \"1.0\"\ndoc = \"Base64 text of a map request, passed through as text.\"";
    let go =
        format!("{GO_ROW}\nargs = [ {{ key = \"drive.command.drive_mode\", value = \"off\" }} ]");
    let stop = format!("{STOP_ROW}\nargs = [\n  {{ field = \"mode\", key = \"drive.command.drive_mode\", absent = \"off\" }},\n  {{ field = \"plan\", key = \"drive.command.plan_blob\", omit_when_absent = true, encoding = \"base64\" }},\n  {{ field = \"pathRange\", element = 0, key = \"drive.command.path_from\", absent = -1, encoding = \"decimal_text\" }},\n  {{ field = \"speed\", key = \"drive.state.speed\", omit_when_absent = true }},\n]");
    vec![
        ("ns/drive.toml", anchor.to_string(), ARG_KEYS.to_string()),
        ("ns/drive.toml", GO_ROW.to_string(), go),
        ("ns/drive.toml", STOP_ROW.to_string(), stop),
    ]
}

#[test]
fn constant_named_state_element_and_binary_arguments_generate() {
    let dir = unique_dir("rows_args");
    let owned = argument_edits();
    let edits: Vec<(&str, &str, &str)> = owned
        .iter()
        .map(|(f, a, b)| (*f, a.as_str(), b.as_str()))
        .collect();
    let gen = generated(&dir, "args", "sku_b", &edits);
    let rows = wire_rows(&read(&gen, "keyspace.sku_b.json"), "drive.command.drive");
    let constant = &rows[0]["args"][0];
    assert_eq!(constant["key"], "drive.command.drive_mode");
    assert_eq!(constant["value"], "off");
    assert!(constant.get("field").is_none() && constant.get("element").is_none());
    let stop = &rows[1]["args"];
    assert_eq!(stop[0]["absent"], "off");
    assert_eq!(stop[1]["encoding"], "base64");
    assert_eq!(stop[2]["element"], 0);
    assert_eq!(stop[2]["encoding"], "decimal_text");
    assert_eq!(stop[3]["key"], "drive.state.speed");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rows_without_a_command_answer_stay_internal_or_write_state() {
    let dir = unique_dir("rows_commandless");
    let rows = format!("{GO_ROW}\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"ping\" }}\nanswer = 3\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"getlog\" }}\ninternal = \"the library uploads its own log\"\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"mobile\" }}\nargs = [ {{ field = \"label\", key = \"drive.state.label\", omit_when_absent = true }} ]");
    let gen = generated(
        &dir,
        "commandless",
        "sku_b",
        &[("ns/drive.toml", GO_ROW, &rows)],
    );
    let commands = &read(&gen, "keyspace.sku_b.json")["shadow"]["commands"];
    let find = |wire: &str| -> Value {
        commands
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["selector"]["wire"] == wire)
            .unwrap()
            .clone()
    };
    assert_eq!(find("ping")["answer"], 3);
    assert_eq!(
        find("getlog")["internal"],
        "the library uploads its own log"
    );
    assert_eq!(find("mobile")["args"][0]["key"], "drive.state.label");
    assert!(find("go").get("answer").is_none() && find("go").get("internal").is_none());
    let _ = fs::remove_dir_all(&dir);
}

/// The anchor `argument_edits` appends the argument keys after.
const ARG_ANCHOR: &str = "id = \"map_payload\"\nkey_index = 9\ntype = \"string\"\nmax_size = 512\nintroduced_in = \"1.0\"\ndoc = \"Base64 text of a map request, passed through as text.\"";

#[test]
fn a_variant_row_s_values_resolve_in_its_variants_only() {
    let dir = unique_dir("rows_variant_values");
    let plan =
        "args = [ { field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 } ]";
    let named = "args = [\n  { field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 },\n  { key = \"drive.command.drive_mode\", value = \"saver\" },\n  { field = \"mode\", key = \"drive.command.drive_mode\", absent = 3 },\n]";
    let edits = [
        ("ns/drive.toml", ARG_ANCHOR, ARG_KEYS),
        ("ns/drive.toml", plan, named),
    ];
    for variant in ["sku_a", "sku_b"] {
        generated(&dir, variant, variant, &edits);
    }
    let rows = wire_rows(
        &read(&dir.join("sku_b/sku_b"), "keyspace.sku_b.json"),
        "drive.command.drive",
    );
    assert_eq!(rows[2]["args"][1]["value"], "saver");
    let everywhere =
        format!("{GO_ROW}\nargs = [ {{ key = \"drive.command.drive_mode\", value = \"saver\" }} ]");
    let case = dir.join("everywhere");
    edited_copy(
        "v1_0",
        &case,
        &[
            ("ns/drive.toml", ARG_ANCHOR, ARG_KEYS),
            ("ns/drive.toml", GO_ROW, &everywhere),
        ],
    );
    let (ok, stderr) = generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_b"],
    );
    assert!(!ok && stderr.contains("variant sku_a: shadow_commands command_topic state=go: arg drive.command.drive_mode: saver is not a value name"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

/// The speed limit twin's path in the 1.0 model.
const LIMIT_PATH: &str = "shadow = \"settings.speedLimit\"\napplies";

#[test]
fn one_key_takes_one_path_with_a_scope_per_variant() {
    let dir = unique_dir("rows_path_per_variant");
    let split = "shadow = [ { path = \"settings.speedLimit\", variants = [\"sku_a\"] }, { path = \"settings.speedLimit\", scope = \"delta\", variants = [\"sku_b\"] } ]\napplies";
    for variant in ["sku_a", "sku_b"] {
        generated(
            &dir,
            variant,
            variant,
            &[("ns/drive.toml", LIMIT_PATH, split)],
        );
    }
    let all = read(&dir.join("sku_b/sku_b"), "keyspace.json");
    let text = serde_json::to_string(&all).unwrap();
    assert!(text.contains(r#""shadow":["settings.speedLimit"],"shadow_forms":[{"path":"settings.speedLimit","variants":["sku_a"]},{"path":"settings.speedLimit","scope":"delta","variants":["sku_b"]}]"#), "{text}");
    let resolved = read(&dir.join("sku_b/sku_b"), "keyspace.sku_b.json");
    let text = serde_json::to_string(&resolved).unwrap();
    assert!(text.contains(r#""shadow_forms":[{"path":"settings.speedLimit","scope":"delta","variants":["sku_b"]}]"#), "{text}");
    let both = "shadow = [ { path = \"settings.speedLimit\", variants = [\"sku_a\", \"sku_b\"] }, { path = \"settings.speedLimit\", scope = \"delta\", variants = [\"sku_b\"] } ]\napplies";
    let case = dir.join("overlap");
    edited_copy("v1_0", &case, &[("ns/drive.toml", LIMIT_PATH, both)]);
    let (ok, stderr) = generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_a"],
    );
    assert!(
        !ok && stderr.contains(
            "variant sku_b: drive.state.speed_limit_applied: a shadow path is listed twice"
        ),
        "{stderr}"
    );
    assert!(
        !stderr.contains("variant sku_a: drive.state.speed_limit_applied: a shadow path"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The echo row of the 1.0 model that the per-variant echo test limits.
const PLAN_ECHO: &str = "path = \"settings.planExists\"         # app metadata: echoed back so the delta clears, no key\ntype = \"bool\"";

/// @brief The edits that echo settings.planExists on sku_a only and apply it to keys on sku_b.
/// @version 1
fn echo_edits(only: &str) -> Vec<(&'static str, String, String)> {
    let setting = "[[classes.keys]]\nid = \"plan_exists\"\nkey_index = 4\ntype = \"bool\"\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\nvariants = [\"sku_b\"]\nshadow = \"settings.planExists\"\n\n[[classes.keys]]\nid = \"fan\"\nkey_index = 5";
    let twin = "[[classes.keys]]\nid = \"plan_exists_applied\"\nkey_index = 14\ntype = \"bool\"\nintroduced_in = \"1.0\"\nvariants = [\"sku_b\"]\nshadow = \"settings.planExists\"\napplies = \"drive.settings.plan_exists\"\n\n[[classes]]\nid = \"map\"";
    vec![
        (
            "contract.toml",
            PLAN_ECHO.to_string(),
            format!("{PLAN_ECHO}\nvariants = [\"{only}\"]"),
        ),
        (
            "ns/drive.toml",
            "[[classes.keys]]\nid = \"fan\"\nkey_index = 5".to_string(),
            setting.to_string(),
        ),
        (
            "ns/drive.toml",
            "[[classes]]\nid = \"map\"".to_string(),
            twin.to_string(),
        ),
    ]
}

/// @brief Generate the 1.0 model with `echo_edits(only)` for `variant` into `dir/tag`; returns
/// (success, stderr, output directory).
/// @version 1
fn echo_case(dir: &Path, tag: &str, only: &str, variant: &str) -> (bool, String, PathBuf) {
    let owned = echo_edits(only);
    let edits: Vec<(&str, &str, &str)> = owned
        .iter()
        .map(|(f, a, b)| (*f, a.as_str(), b.as_str()))
        .collect();
    let case = dir.join(tag);
    edited_copy("v1_0", &case, &edits);
    let out = case.join("out");
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
    (ok, stderr, out)
}

/// @brief The echo rows of a keyspace document.
/// @version 1
fn echo_rows(doc: &Value) -> Vec<Value> {
    doc["shadow"]["echo"].as_array().unwrap().clone()
}

#[test]
fn an_echo_path_on_one_variant_and_a_key_path_on_another() {
    let dir = unique_dir("rows_echo_variants");
    let plan = |r: &Value| r["path"] == "settings.planExists";
    let (ok, stderr, a) = echo_case(&dir, "a", "sku_a", "sku_a");
    assert!(ok, "{stderr}");
    assert!(echo_rows(&read(&a, "keyspace.sku_a.json")).iter().any(plan));
    let (ok, stderr, b) = echo_case(&dir, "b", "sku_a", "sku_b");
    assert!(ok, "{stderr}");
    assert!(!echo_rows(&read(&b, "keyspace.sku_b.json")).iter().any(plan));
    let all = echo_rows(&read(&b, "keyspace.json"));
    let echo = all.iter().find(|r| plan(r)).unwrap();
    assert_eq!(echo["variants"], json!(["sku_a"]));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_echo_row_names_declared_variants_only() {
    let dir = unique_dir("rows_echo_undeclared");
    let (ok, stderr, _) = echo_case(&dir, "z", "sku_z", "sku_a");
    let refusal = "shadow_echo settings.planExists: variant sku_z is not declared";
    assert!(!ok && stderr.contains(refusal), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_all_variants_size_bounds_every_variant() {
    let dir = unique_dir("rows_size_bound");
    let fan = "shadow = [{ path = \"state.fanSpeed\", as = \"integer\", variants = [\"sku_a\"] }, { path = \"state.fanSpeed\", variants = [\"sku_b\"] }, \"settings.fanSpeed\"]";
    let limit = "shadow = [ { path = \"settings.speedLimit\", variants = [\"sku_a\"] }, { path = \"settings.speedLimit\", scope = \"delta\", variants = [\"sku_b\"] } ]\napplies";
    let edits = [
        (
            "ns/drive.toml",
            "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]",
            fan,
        ),
        ("ns/drive.toml", LIMIT_PATH, limit),
    ];
    let bytes = |doc: &Value| doc["shadow"]["full_max_bytes"].as_u64().unwrap();
    let a = generated(&dir, "a", "sku_a", &edits);
    let b = generated(&dir, "b", "sku_b", &edits);
    let all = bytes(&read(&b, "keyspace.json"));
    for (gen, file) in [(&a, "keyspace.sku_a.json"), (&b, "keyspace.sku_b.json")] {
        assert!(all >= bytes(&read(gen, file)), "{file}: {all}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_without_variants_checks_its_paths_in_its_only_pass() {
    let dir = unique_dir("rows_no_variants");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract/no_unix");
    copy_dir(&fixture, &dir);
    let file = dir.join("contract.toml");
    let twice = "\n[[shadow_constant]]\npath = \"sv\"\nvalue = \"a\"\n\n[[shadow_constant]]\npath = \"sv\"\nvalue = \"b\"\n";
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("{text}{twice}")).unwrap();
    let out = dir.join("out");
    let args = [
        "--contract",
        file.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ];
    let (ok, stderr) = run_ingot(&[&args[..], &["--allow-dirty-ingot"]].concat());
    assert!(
        !ok && stderr.contains("shadow_constant: shadow path sv is taken by shadow_constant"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The all-variants keyspace's shadow sizes of the 1.0 model after `edits`.
/// @version 1
fn all_sizes(dir: &Path, tag: &str, edits: &[(&str, &str, &str)]) -> (u64, u64) {
    let doc = read(&generated(dir, tag, "sku_b", edits), "keyspace.json");
    let size = |k: &str| doc["shadow"][k].as_u64().unwrap();
    (size("full_max_bytes"), size("delta_forced_bytes"))
}

#[test]
fn per_variant_entries_at_one_path_count_once_at_their_widest() {
    let dir = unique_dir("rows_widest");
    let fan_anchor = "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]";
    let split_fan = "shadow = [{ path = \"state.fanSpeed\", as = \"integer\", variants = [\"sku_a\"] }, { path = \"state.fanSpeed\", variants = [\"sku_b\"] }, \"settings.fanSpeed\"]";
    let docked = "shadow = \"state.docked\"\nforced";
    let both =
        "shadow = [\"state.docked\", { path = \"state.dockedB\", variants = [\"sku_b\"] }]\nforced";
    let split = "shadow = [{ path = \"state.docked\", variants = [\"sku_a\"] }, { path = \"state.docked\", scope = \"full\", variants = [\"sku_b\"] }, { path = \"state.dockedB\", variants = [\"sku_b\"] }]\nforced";
    let plain = all_sizes(&dir, "plain", &[("ns/drive.toml", docked, both)]);
    let widest = all_sizes(
        &dir,
        "split",
        &[
            ("ns/drive.toml", fan_anchor, split_fan),
            ("ns/drive.toml", docked, split),
        ],
    );
    assert_eq!(
        widest, plain,
        "the enum form is the widest; a delta entry on one variant puts the path in the delta"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_minor_may_add_paths_aliases_and_rows() {
    let dir = unique_dir("rows_minor_growth");
    let prev = dir.join("prev");
    generate_ok("v1_0", &prev, &[]);
    let case = dir.join("grown");
    let edits = [
        ("ns/drive.toml", "accept = [\"max\"] }", "accept = [\"max\", \"turbo\"] }"),
        ("ns/drive.toml", "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]", "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\", \"state.fanLevel\"]"),
        ("contract.toml", "[[shadow_constant]]\npath = \"sv\"", "[[shadow_echo]]\npath = \"settings.grown\"\ntype = \"bool\"\n\n[[shadow_constant]]\npath = \"sv\""),
        ("ns/drive.toml", "id = \"odometer\"\nkey_index = 1", "id = \"odometer\"\nkey_index = 1\nshadow = \"state.odometer\"\ntelem = \"odo\""),
    ];
    edited_copy("v1_1", &case, &edits);
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &args);
    assert!(ok, "growth is a minor: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

/// The trim echo row of the 1.0 model.
const TRIM_ECHO: &str = "path = \"settings.trim\"\ntype = \"float\"";

/// A STATE key reporting settings.trim in the full document only (the gain's applied twin).
const TRIM_KEY: &str = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"gain_applied\"\nkey_index = 14\ntype = \"float32\"\nintroduced_in = \"1.0\"\nshadow = { path = \"settings.trim\", scope = \"full\" }";

#[test]
fn a_delta_echo_shares_a_path_with_a_full_only_key() {
    let dir = unique_dir("rows_echo_scope");
    let delta = format!("{TRIM_ECHO}\nscope = \"delta\"");
    let edits = [
        ("contract.toml", TRIM_ECHO, delta.as_str()),
        ("ns/drive.toml", "telem = \"dock\"", TRIM_KEY),
    ];
    let gen = generated(&dir, "split", "sku_a", &edits);
    let c = fs::read_to_string(gen.join("dsh_tables.c")).unwrap();
    let rows: Vec<&str> = c
        .lines()
        .filter(|l| l.contains(".path = \"settings.trim\"") && l.contains(".kind = DSH_RK_"))
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows.iter()
            .any(|r| r.contains("DSH_RK_ECHO") && r.contains(".scope = DSH_SC_DELTA,")),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|r| r.contains("DSH_RK_KEY") && r.contains(".scope = DSH_SC_FULL,")),
        "{rows:?}"
    );
    let case = dir.join("both");
    edited_copy(
        "v1_0",
        &case,
        &[("ns/drive.toml", "telem = \"dock\"", TRIM_KEY)],
    );
    let (ok, stderr) = generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_a"],
    );
    assert!(
        !ok && stderr.contains("shadow path settings.trim is taken by"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_payload_member_is_frozen_within_a_major() {
    let dir = unique_dir("rows_payload_frozen");
    let resp = "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"";
    let member = |v: &str| {
        format!(
            "{resp}\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nvalue = \"{v}\""
        )
    };
    let (old, new) = (member("1.0"), member("1.1"));
    let prev = generated(&dir, "prev", "sku_a", &[("contract.toml", resp, &old)]);
    let case = dir.join("cur");
    edited_copy("v1_0", &case, &[("contract.toml", resp, &new)]);
    let previous = prev.join("contract.json");
    let args = [
        "--variant",
        "sku_a",
        "--previous-contract",
        previous.to_str().unwrap(),
    ];
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &args);
    assert!(
        !ok && stderr.contains("topic_payload command_resp sv changed or was removed in a minor"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A STATE key reporting settings.trim in both documents.
const TRIM_KEY_BOTH: &str = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"gain_applied\"\nkey_index = 14\ntype = \"float32\"\nintroduced_in = \"1.0\"\nshadow = \"settings.trim\"";

/// @brief Generate the 1.0 model (sku_a) with the trim echo made `echo` and `key` added.
/// @version 1
fn trim_case(dir: &Path, tag: &str, echo: &str, key: &str) -> (bool, String) {
    let case = dir.join(tag);
    edited_copy(
        "v1_0",
        &case,
        &[
            ("contract.toml", TRIM_ECHO, echo),
            ("ns/drive.toml", "telem = \"dock\"", key),
        ],
    );
    generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_a"],
    )
}

#[test]
fn a_delta_echo_and_a_key_share_one_delta_member() {
    let dir = unique_dir("rows_echo_shares");
    let delta = format!("{TRIM_ECHO}\nscope = \"delta\"");
    let (ok, stderr) = trim_case(&dir, "share", &delta, TRIM_KEY_BOTH);
    assert!(
        ok,
        "an echo and a key of one encoding share the delta member: {stderr}"
    );
    let bool_echo = delta.replace("type = \"float\"", "type = \"bool\"");
    let (ok, stderr) = trim_case(&dir, "mismatch", &bool_echo, TRIM_KEY_BOTH);
    assert!(
        !ok && stderr.contains("shadow path settings.trim is taken by drive.state.gain_applied"),
        "{stderr}"
    );
    let twice = format!(
        "{delta}\n\n[[shadow_echo]]\npath = \"settings.trim\"\ntype = \"float\"\nscope = \"delta\""
    );
    let (ok, stderr) = trim_case(&dir, "twice", &twice, TRIM_KEY_BOTH);
    assert!(
        !ok && stderr.contains("shadow path settings.trim is taken by"),
        "two echo rows: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_full_only_echo_leaves_the_delta_to_a_key() {
    let dir = unique_dir("rows_echo_full");
    let full =
        format!("{TRIM_ECHO}\nscope = \"full\"").replace("type = \"float\"", "type = \"bool\"");
    let key = TRIM_KEY.replace("scope = \"full\"", "scope = \"delta\"");
    let (ok, stderr) = trim_case(&dir, "split", &full, &key);
    assert!(
        ok,
        "a bool echo in the full document, a float key in the delta: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_echo_shares_only_with_a_key_in_the_delta() {
    let dir = unique_dir("rows_echo_full_key");
    let delta = format!("{TRIM_ECHO}\nscope = \"delta\"");
    let constant = "[[shadow_constant]]\npath = \"settings.trim\"\nvalue = 1.0\nforced = true\n\n[[shadow_constant]]\npath = \"sv\"";
    let case = dir.join("constant");
    let edits = [
        ("contract.toml", TRIM_ECHO, delta.as_str()),
        (
            "contract.toml",
            "[[shadow_constant]]\npath = \"sv\"",
            constant,
        ),
        ("ns/drive.toml", "telem = \"dock\"", TRIM_KEY),
    ];
    edited_copy("v1_0", &case, &edits);
    let (ok, stderr) = generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_a"],
    );
    assert!(
        !ok && stderr.contains("shadow path settings.trim is taken by shadow_echo"),
        "a full-only key does not take the echo's delta member: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_config_key_path_conflicts_like_a_state_key_path() {
    let dir = unique_dir("rows_config_conflict");
    let case = dir.join("conflict");
    let serial = "id = \"serial\"";
    edited_copy(
        "v1_0",
        &case,
        &[(
            "ns/mfg.toml",
            serial,
            "id = \"serial\"\nshadow = \"state.docked\"",
        )],
    );
    let (ok, stderr) = generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_a"],
    );
    assert!(
        !ok && stderr.contains("shadow path state.docked is taken by"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn two_topics_may_carry_one_payload_path() {
    let dir = unique_dir("rows_payload_topics");
    let resp = "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"";
    let member =
        |t: &str| format!("\n\n[[topic_payload]]\ntopic = \"{t}\"\npath = \"sv\"\nvalue = \"1\"");
    let both = format!("{resp}{}{}", member("command_resp"), member("mapjob_resp"));
    generated(&dir, "both", "sku_a", &[("contract.toml", resp, &both)]);
    let _ = fs::remove_dir_all(&dir);
}
