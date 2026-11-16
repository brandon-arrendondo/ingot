//! The shadow wire mapping: what contract.json and the keyspace files carry, the
//! worst-case document size, and that the mapping never changes the store tables or digests.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// Key attributes of the mapping, removed line by line to build the unmapped model.
const WIRE_ATTRIBUTES: &[&str] = &[
    "shadow =",
    "applies =",
    "apply_group =",
    "implies =",
    "on_reject =",
    "inbound_unknown =",
    "outbound_unknown =",
    "forced =",
    "render =",
    "telem =",
    "internal =",
];

/// @brief `text` without the mapping: no attribute lines, no accept aliases, nothing after the
/// contract-level marker.
/// @version 1
fn unmapped(text: &str) -> String {
    let head = text
        .split("\n# -- shadow wire mapping")
        .next()
        .unwrap_or_default();
    let mut out: String = head
        .lines()
        .filter(|l| !WIRE_ATTRIBUTES.iter().any(|a| l.starts_with(a)))
        .map(|l| format!("{l}\n"))
        .collect();
    while let Some(start) = out.find(", accept = [") {
        let end = start + out[start..].find(']').unwrap() + 1;
        out.replace_range(start..end, "");
    }
    out
}

/// @brief The JSON object of key `path` in contract.json.
/// @version 1
fn key(gen: &Path, path: &str) -> serde_json::Value {
    contract_json(gen)["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["path"] == path)
        .unwrap_or_else(|| panic!("key {path}"))
        .clone()
}

/// @brief Copy `version` of the generic model into `dir` without its mapping, asserting every
/// mapped file carried one.
/// @version 1
fn unmapped_copy(version: &str, dir: &Path) {
    copy_dir(&model(version), dir);
    for file in ["contract.toml", "ns/drive.toml", "ns/core.toml"] {
        let path = dir.join(file);
        let text = fs::read_to_string(&path).unwrap();
        assert_ne!(unmapped(&text), text, "{file} carries a mapping");
        fs::write(&path, unmapped(&text)).unwrap();
    }
}

/// @brief A generated keyspace file of `gen`, parsed.
/// @version 1
fn read_json(gen: &Path, file: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(gen.join(file)).unwrap()).unwrap()
}

#[test]
fn store_tables_and_digests_do_not_depend_on_the_mapping() {
    let dir = unique_dir("shadow_neutral");
    let (mapped, model_dir, plain) = (dir.join("mapped"), dir.join("model"), dir.join("plain"));
    unmapped_copy("v1_1", &model_dir);
    for variant in ["sku_a", "sku_b"] {
        generate_ok("v1_1", &mapped, &["--variant", variant]);
        let (ok, stderr) = generate(
            &model_dir.join("contract.toml"),
            &plain,
            &["--variant", variant],
        );
        assert!(ok, "{stderr}");
        assert_same_store_tables(&mapped, &plain);
        assert!(contract_json(&mapped).get("shadow").is_some());
        assert!(contract_json(&plain).get("shadow").is_none());
        assert!(key(&plain, "drive.settings.fan").get("shadow").is_none());
        assert!(key(&plain, "drive.settings.fan")
            .get("enum_accept")
            .is_none());
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_carries_the_key_attributes_and_rows() {
    let dir = unique_dir("shadow_json");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &["--variant", "sku_b"]);
    let fan = key(&gen, "drive.settings.fan");
    assert_eq!(fan["shadow"], serde_json::json!(["settings.fanSpeed"]));
    assert_eq!(fan["apply_group"], "motion");
    assert_eq!(fan["inbound_unknown"], "quiet");
    assert_eq!(
        fan["on_reject"], "echo",
        "a shadowed setting echoes by default"
    );
    assert_eq!(
        fan["implies"],
        serde_json::json!([{"key": "drive.settings.speed_limit", "value": 500}])
    );
    assert_eq!(
        fan["enum_accept"],
        serde_json::json!({"0": ["quiet"], "2": ["max"]})
    );
    assert_eq!(
        key(&gen, "drive.settings.speed_limit")["on_reject"],
        "clear_desired"
    );
    assert!(key(&gen, "drive.settings.mode").get("on_reject").is_none());
    let twin = key(&gen, "drive.state.fan_applied");
    assert_eq!(
        twin["shadow"],
        serde_json::json!(["state.fanSpeed", "settings.fanSpeed"])
    );
    assert_eq!(twin["applies"], "drive.settings.fan");
    assert_eq!(twin["outbound_unknown"], "normal");
    let tones = key(&gen, "drive.state.tones");
    assert_eq!(
        tones["render"],
        serde_json::json!({"kind": "csv_index_list", "enum": "tone", "max_items": 4})
    );
    assert_eq!(tones["telem"], "tones");
    assert_eq!(key(&gen, "drive.state.docked")["forced"], true);
    let shadow = &contract_json(&gen)["shadow"];
    assert_eq!(shadow["apply_groups"][0]["id"], "motion");
    assert_eq!(shadow["echo"][1]["command_key"], "drive.settings.gain");
    assert_eq!(shadow["constants"][0]["forced"], true);
    assert_eq!(shadow["constants"][3]["image"], "contract_hash");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_keyspace_files_carry_the_mapping() {
    let dir = unique_dir("shadow_keyspace");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &["--variant", "sku_b"]);
    let read = |f: &str| -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(gen.join(f)).unwrap()).unwrap()
    };
    let (body, all) = (read("keyspace.sku_b.json"), read("keyspace.json"));
    let fan = &body["enums"][0];
    assert_eq!(fan["name"], "drive::fan");
    assert_eq!(fan["values"][0]["accept"], serde_json::json!(["quiet"]));
    assert!(fan["values"][1].get("accept").is_none());
    let overrides = &all["enums"][0]["overrides"]["sku_b"]["0"];
    assert_eq!(
        *overrides,
        serde_json::json!({"name": "whisper", "wire": "whisper", "accept": ["quiet"]})
    );
    let text = fs::read_to_string(gen.join("keyspace.sku_b.json")).unwrap();
    assert!(text.contains("\"applies\":\"drive.settings.fan\""));
    assert_eq!(body["shadow"], contract_json(&gen)["shadow"]);
    assert_eq!(all["shadow"]["echo"], body["shadow"]["echo"]);
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The worst-case reported document of the 1.0 model for a variant, written out by hand:
/// fan's longest wire string, its error ids, and every other row at its longest value.
/// @version 1
fn worst_document(fan: &str, errors: &str) -> String {
    let float = "-340282346638528859811704183484516925440.000000";
    let hash = "0".repeat(64);
    format!(
        concat!(
            r#"{{"state":{{"reported":{{"config":{{"cv":"1.0.0","cvHash":"{hash}"}},"#,
            r#""settings":{{"fanSpeed":"{fan}","planExists":false,"speedLimit":65535,"trim":{float}}},"#,
            r#""state":{{"connected":true,"docked":false,"errors":{errors},"fanSpeed":"{fan}","#,
            r#""tones":["chime","chime","chime","chime"]}},"sv":"1.0-gen"}}}}}}"#
        ),
        hash = hash,
        fan = fan,
        float = float,
        errors = errors
    )
}

#[test]
fn the_document_size_equals_an_independent_count() {
    let dir = unique_dir("shadow_size");
    let cases = [
        ("sku_a", "normal", r#"[{"ec":"e1"}]"#),
        ("sku_b", "whisper", r#"[{"ec":"e1"},{"ec":"e50"}]"#),
    ];
    for (variant, fan, errors) in cases {
        let gen = dir.join(variant);
        generate_ok("v1_0", &gen, &["--variant", variant]);
        let doc = worst_document(fan, errors);
        let _: serde_json::Value = serde_json::from_str(&doc).expect("the oracle is JSON");
        let shadow = &contract_json(&gen)["shadow"];
        assert_eq!(shadow["full_max_bytes"], doc.len(), "{variant}");
        let forced = r#"{"state":{"reported":{"state":{"connected":true,"docked":false}}}}"#;
        assert_eq!(shadow["delta_forced_bytes"], forced.len(), "{variant}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_rules_accept_aliases_reject_and_raw() {
    let dir = unique_dir("shadow_unknowns");
    let cases = [
        ("inbound_unknown = \"quiet\"", "inbound_unknown = \"max\""),
        (
            "inbound_unknown = \"quiet\"",
            "inbound_unknown = \"reject\"",
        ),
        (
            "outbound_unknown = \"normal\"",
            "outbound_unknown = \"raw\"",
        ),
        (
            "inbound_unknown = \"quiet\"",
            "inbound_unknown = \"ignore\"",
        ),
    ];
    for (i, (from, to)) in cases.iter().enumerate() {
        let case = dir.join(format!("case{i}"));
        edited_copy("v1_0", &case, &[("ns/drive.toml", from, to)]);
        for variant in ["sku_a", "sku_b"] {
            let out = case.join(variant);
            let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
            assert!(ok, "{to} ({variant}): {stderr}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn every_command_is_enumerated_with_its_wire_rows() {
    let dir = unique_dir("shadow_commands");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &["--variant", "sku_b"]);
    let tables = contract_json(&gen)["commands"].as_array().unwrap().len();
    for file in ["keyspace.sku_b.json", "keyspace.json"] {
        let doc = read_json(&gen, file);
        let commands = doc["commands"].as_array().unwrap();
        assert_eq!(commands.len(), tables, "{file}: every command is listed");
        for c in commands {
            assert!(c["id"].is_u64() && c["result"].is_u64() && c["dedupe"].is_u64());
            assert!(c["name"].is_string() && c["owner"].is_string());
            let rows = c["wire"].as_array().unwrap().len();
            assert!((rows > 0) != c["internal"].is_string(), "{}", c["name"]);
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The wire rows of drive.command.drive in a keyspace document.
/// @version 1
fn drive_rows(doc: &serde_json::Value) -> Vec<serde_json::Value> {
    doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "drive.command.drive")
        .unwrap()["wire"]
        .as_array()
        .unwrap()
        .clone()
}

/// @brief The selector wire strings of rows.
/// @version 1
fn verbs(rows: &[serde_json::Value]) -> Vec<&str> {
    rows.iter()
        .map(|r| r["selector"]["wire"].as_str().unwrap())
        .collect()
}

#[test]
fn rows_resolve_per_variant_with_their_arguments() {
    let dir = unique_dir("shadow_rows");
    let (a, b) = (dir.join("a"), dir.join("b"));
    generate_ok("v1_0", &a, &["--variant", "sku_a"]);
    generate_ok("v1_0", &b, &["--variant", "sku_b"]);
    let (sku_a, sku_b) = (
        read_json(&a, "keyspace.sku_a.json"),
        read_json(&b, "keyspace.sku_b.json"),
    );
    assert_eq!(
        verbs(&drive_rows(&sku_a)),
        ["go", "stop"],
        "approve is sku_b's"
    );
    assert_eq!(verbs(&drive_rows(&sku_b)), ["go", "stop", "approve"]);
    assert_eq!(
        verbs(&drive_rows(&read_json(&a, "keyspace.json"))),
        ["go", "stop", "approve"]
    );
    let plan = &drive_rows(&sku_b)[2];
    assert_eq!(plan["variants"], serde_json::json!(["sku_b"]));
    assert_eq!(plan["args"][0]["key"], "drive.command.plan_epoch");
    assert_eq!(plan["args"][0]["absent"], 0);
    let shadow = &sku_b["shadow"];
    assert_eq!(
        shadow["topics"][2]["envelope"]["source"]["absent"],
        "unspecified"
    );
    assert_eq!(shadow["commands"].as_array().unwrap().len(), 10);
    let split = &shadow["commands"][9];
    assert_eq!(split["protocol"], "command_array");
    assert_eq!(split["args"][1]["required"], true);
    assert_eq!(split["args"][1]["ec"], 5);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn accepted_topic_and_row_forms() {
    let dir = unique_dir("shadow_accepted");
    let cases: [&[(&str, &str, &str)]; 2] = [
        &[(
            "contract.toml",
            "shadow/name/{shadow_name}/update/delta\"",
            "shadow/name/device/update/delta\"\nliteral_shadow_name = true",
        )],
        &[(
            "ns/drive.toml",
            "args = [ { field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 } ]",
            "args = [ { field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 } ]\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nvariants = [\"sku_a\"]\ngrounded_in = \"the sku_a library build\"\nselector = { field = \"state\", wire = \"approve\" }\ncommand = \"drive.command.drive\"",
        )],
    ];
    for (i, edits) in cases.iter().enumerate() {
        let case = dir.join(format!("case{i}"));
        edited_copy("v1_0", &case, edits);
        for variant in ["sku_a", "sku_b"] {
            let out = case.join(variant);
            let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
            assert!(ok, "case {i} ({variant}): {stderr}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn any_one_row_kind_declares_a_mapping() {
    let dir = unique_dir("shadow_declared");
    let rows = [
        "[[apply_groups]]\nid = \"g\"\n",
        "[[shadow_echo]]\npath = \"settings.x\"\ntype = \"bool\"\n",
        "[[shadow_constant]]\npath = \"sv\"\nvalue = \"1\"\n",
        "[[topics]]\nid = \"t\"\ntemplate = \"{duid}/t\"\ndirection = \"pub\"\n",
    ];
    for (i, row) in rows.iter().enumerate() {
        let case = dir.join(format!("case{i}"));
        unmapped_copy("v1_0", &case);
        let contract = case.join("contract.toml");
        let text = fs::read_to_string(&contract).unwrap();
        fs::write(&contract, format!("{text}\n{row}")).unwrap();
        let (ok, stderr) = generate(&contract, &case.join("out"), &[]);
        assert!(ok, "case {i}: {stderr}");
        let shadow = contract_json(&case.join("out")).get("shadow").cloned();
        assert!(shadow.is_some(), "case {i}");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// @brief full_max_bytes of `v1_0` for sku_b after `edits`.
/// @version 1
fn full_bytes(dir: &Path, tag: &str, edits: &[(&str, &str, &str)]) -> u64 {
    let case = dir.join(tag);
    edited_copy("v1_0", &case, edits);
    let out = case.join("out");
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", "sku_b"]);
    assert!(ok, "{tag}: {stderr}");
    contract_json(&out)["shadow"]["full_max_bytes"]
        .as_u64()
        .unwrap()
}

#[test]
fn a_raw_code_and_an_unreported_group_change_the_size() {
    let dir = unique_dir("shadow_size_rules");
    let code = |rule: &str| {
        format!("telem = \"dock\"\n\n[[classes.keys]]\nid = \"code\"\nkey_index = 12\ntype = \"int32\"\nenum = \"tone\"\nintroduced_in = \"1.0\"\nshadow = \"state.code\"\noutbound_unknown = \"{rule}\"")
    };
    let (beep, raw) = (code("beep"), code("raw"));
    let named = full_bytes(
        &dir,
        "beep",
        &[("ns/drive.toml", "telem = \"dock\"", &beep)],
    );
    let coded = full_bytes(&dir, "raw", &[("ns/drive.toml", "telem = \"dock\"", &raw)]);
    assert_eq!(
        coded - named,
        "-2147483648".len() as u64 - "\"chime\"".len() as u64
    );
    let all = full_bytes(&dir, "all", &[]);
    let dock = "wire_bases = { sku_b = 50 }          # sku_b numbers its dock errors from 50\nwriter = \"integrator\"\nshadow = \"state.errors\"\n";
    let unreported = dock.replace("shadow = \"state.errors\"\n", "");
    let product = full_bytes(&dir, "product", &[("contract.toml", dock, &unreported)]);
    assert_eq!(all - product, ",{\"ec\":\"e50\"}".len() as u64);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_names_why_a_command_is_internal() {
    let dir = unique_dir("shadow_internal");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let commands = contract_json(&gen)["commands"].as_array().unwrap().clone();
    let internal = |path: &str| {
        commands
            .iter()
            .find(|c| c["path"] == path)
            .unwrap()
            .get("internal")
            .cloned()
    };
    let reason = "sent by the core before a factory reset; never requested on the wire";
    assert_eq!(
        internal("core.command.pre_reset"),
        Some(serde_json::json!(reason))
    );
    assert_eq!(internal("drive.command.drive"), None);
    let _ = fs::remove_dir_all(&dir);
}

/// fan_applied's two reported paths in the 1.0 model.
const FAN_PATHS: &str = "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]";

/// @brief contract.json of `v1_0` for sku_b after `edits`, asserting it generates.
/// @version 1
fn edited_json(dir: &Path, tag: &str, edits: &[(&str, &str, &str)]) -> serde_json::Value {
    let case = dir.join(tag);
    edited_copy("v1_0", &case, edits);
    let out = case.join("out");
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", "sku_b"]);
    assert!(ok, "{tag}: {stderr}");
    contract_json(&out)
}

/// @brief The JSON object of key `path` in a parsed contract.json or keyspace key list.
/// @version 1
fn find_key<'a>(keys: &'a serde_json::Value, path: &str, field: &str) -> &'a serde_json::Value {
    keys.as_array()
        .unwrap()
        .iter()
        .find(|k| k[field] == path)
        .unwrap_or_else(|| panic!("key {path}"))
}

#[test]
fn one_key_is_reported_as_a_string_and_as_an_integer() {
    let dir = unique_dir("shadow_forms");
    let base = edited_json(&dir, "base", &[]);
    let two = "shadow = [\"state.fanSpeed\", { path = \"state.fanLevel\", as = \"integer\" }, { path = \"settings.fanSpeed\", as = \"enum\" }]";
    let json = edited_json(&dir, "two", &[("ns/drive.toml", FAN_PATHS, two)]);
    let fan = find_key(&json["keys"], "drive.state.fan_applied", "path");
    assert_eq!(
        fan["shadow"],
        serde_json::json!(["state.fanSpeed", "state.fanLevel", "settings.fanSpeed"])
    );
    assert_eq!(
        fan["shadow_forms"],
        serde_json::json!([{"path": "state.fanSpeed"}, {"path": "state.fanLevel", "as": "integer"},
                           {"path": "settings.fanSpeed", "as": "enum"}])
    );
    let grown = json["shadow"]["full_max_bytes"].as_u64().unwrap()
        - base["shadow"]["full_max_bytes"].as_u64().unwrap();
    assert_eq!(
        grown,
        ",\"fanLevel\":255".len() as u64,
        "an integer path counts the type's digits"
    );
    let plain = find_key(&base["keys"], "drive.state.fan_applied", "path");
    assert!(
        plain.get("shadow_forms").is_none(),
        "no forms without a declared one"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_integer_only_enum_key_needs_no_wire_strings_or_unknown_rules() {
    let dir = unique_dir("shadow_integer_only");
    let code = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"mode_code\"\nkey_index = 14\ntype = \"uint8\"\nenum = \"mode\"\nintroduced_in = \"1.0\"\nshadow = { path = \"state.modeCode\", as = \"integer\" }";
    let fan_in = "shadow = { path = \"settings.fanSpeed\", as = \"integer\" }\napply_group";
    let json = edited_json(
        &dir,
        "code",
        &[
            ("ns/drive.toml", "telem = \"dock\"", code),
            ("ns/drive.toml", "shadow = \"settings.fanSpeed\"\napply_group", fan_in),
            ("ns/drive.toml", "inbound_unknown = \"quiet\"            # an unmatched string is quiet (code 0), as today\n", ""),
        ],
    );
    let key = find_key(&json["keys"], "drive.state.mode_code", "path");
    assert_eq!(key["shadow_forms"][0]["as"], "integer");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_path_takes_its_full_and_its_delta_value_from_two_keys() {
    let dir = unique_dir("shadow_scopes");
    let base = edited_json(&dir, "base", &[]);
    let accepted = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"speed_limit_accepted\"\nkey_index = 14\ntype = \"uint16\"\nintroduced_in = \"1.0\"\nshadow = { path = \"settings.speedLimit\", scope = \"delta\" }";
    let full_only = "shadow = { path = \"settings.speedLimit\", scope = \"full\" }\napplies";
    let json = edited_json(
        &dir,
        "split",
        &[
            ("ns/drive.toml", "telem = \"dock\"", accepted),
            (
                "ns/drive.toml",
                "shadow = \"settings.speedLimit\"\napplies",
                full_only,
            ),
        ],
    );
    assert_eq!(
        json["shadow"]["full_max_bytes"],
        base["shadow"]["full_max_bytes"]
    );
    let key = find_key(&json["keys"], "drive.state.speed_limit_accepted", "path");
    assert_eq!(key["shadow_forms"][0]["scope"], "delta");
    let gen = dir.join("split/out");
    let body = read_json(&gen, "keyspace.sku_b.json");
    let text = serde_json::to_string(&body).unwrap();
    assert!(text.contains(r#"{"path":"settings.speedLimit","scope":"full"}"#));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_scope_places_a_row_in_the_full_document_or_the_delta() {
    let dir = unique_dir("shadow_scope_sizes");
    let base = edited_json(&dir, "base", &[]);
    let docked = "shadow = { path = \"state.docked\", scope = \"delta\" }\nforced";
    let delta = edited_json(
        &dir,
        "delta",
        &[("ns/drive.toml", "shadow = \"state.docked\"\nforced", docked)],
    );
    let shrunk = base["shadow"]["full_max_bytes"].as_u64().unwrap()
        - delta["shadow"]["full_max_bytes"].as_u64().unwrap();
    assert_eq!(
        shrunk,
        ",\"docked\":false".len() as u64,
        "a delta-only row is not in the full document"
    );
    assert_eq!(
        delta["shadow"]["delta_forced_bytes"],
        base["shadow"]["delta_forced_bytes"]
    );
    let extra =
        "shadow = [\"state.docked\", { path = \"state.dockedFull\", scope = \"full\" }]\nforced";
    let full_extra = edited_json(
        &dir,
        "extra",
        &[("ns/drive.toml", "shadow = \"state.docked\"\nforced", extra)],
    );
    assert_eq!(
        full_extra["shadow"]["delta_forced_bytes"], base["shadow"]["delta_forced_bytes"],
        "a full-only path of a forced key is not forced"
    );
    let both = "shadow = { path = \"state.docked\", scope = \"both\" }\nforced";
    let explicit = edited_json(
        &dir,
        "both",
        &[("ns/drive.toml", "shadow = \"state.docked\"\nforced", both)],
    );
    assert_eq!(
        explicit["shadow"]["full_max_bytes"],
        base["shadow"]["full_max_bytes"]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_bool_path_as_integer_is_sized_per_document() {
    let dir = unique_dir("shadow_bool_integer_sizes");
    let base = edited_json(&dir, "base", &[]);
    let split = "shadow = [{ path = \"state.docked\", as = \"integer\", scope = \"full\" }, { path = \"state.docked\", scope = \"delta\" }]\nforced";
    let split = edited_json(
        &dir,
        "split",
        &[("ns/drive.toml", "shadow = \"state.docked\"\nforced", split)],
    );
    let shrunk = base["shadow"]["full_max_bytes"].as_u64().unwrap()
        - split["shadow"]["full_max_bytes"].as_u64().unwrap();
    assert_eq!(
        shrunk,
        ("false".len() - "0".len()) as u64,
        "0/1 in the full document"
    );
    assert_eq!(
        split["shadow"]["delta_forced_bytes"], base["shadow"]["delta_forced_bytes"],
        "true/false in the delta"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_conflict_in_both_documents_is_reported_once() {
    let dir = unique_dir("shadow_conflict_once");
    let case = dir.join("model");
    let taken = "shadow = \"state.fanSpeed\"\nforced";
    edited_copy(
        "v1_0",
        &case,
        &[
            ("ns/drive.toml", "shadow = \"state.docked\"\nforced", taken),
            ("ns/drive.toml", "telem = \"dock\"", "telem = \"tones\""),
        ],
    );
    let (ok, stderr) = generate(
        &case.join("contract.toml"),
        &case.join("out"),
        &["--variant", "sku_b"],
    );
    assert!(!ok);
    let conflict =
        "drive.state.docked: shadow path state.fanSpeed is taken by drive.state.fan_applied";
    for pass in ["] ", "variant sku_a: ", "variant sku_b: "] {
        let line = format!("{pass}{conflict}");
        assert_eq!(stderr.matches(&line).count(), 1, "{line}:\n{stderr}");
    }
    assert!(
        !stderr.contains(&format!("base: {conflict}")),
        "the all-variants view leaves path conflicts to each variant: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief `text` with its key wire attributes but without contract-level rows and groups.
/// @version 1
fn keys_only(text: &str) -> String {
    let head = text.split("\n# -- shadow wire mapping").next().unwrap();
    let grouped = |l: &&str| l.starts_with("apply_group =") || l.starts_with("implies =");
    let kept: Vec<&str> = head.lines().filter(|l| !grouped(l)).collect();
    format!("{}\n", kept.join("\n"))
}

#[test]
fn mapped_keys_alone_declare_a_mapping() {
    let dir = unique_dir("shadow_keys_only");
    let case = dir.join("model");
    copy_dir(&model("v1_0"), &case);
    for file in ["contract.toml", "ns/drive.toml", "ns/core.toml"] {
        let path = case.join(file);
        let text = fs::read_to_string(&path).unwrap();
        assert_ne!(keys_only(&text), text, "{file} has contract-level rows");
        fs::write(&path, keys_only(&text)).unwrap();
    }
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    assert!(ok, "{stderr}");
    let shadow = &contract_json(&case.join("out"))["shadow"];
    assert!(shadow["topics"].as_array().unwrap().is_empty());
    let _ = fs::remove_dir_all(&dir);
}
