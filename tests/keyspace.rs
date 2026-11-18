//! The keyspace files of contract mode: the resolved body, its gzip and head,
//! the all-variants file with overrides, the schema's trust rule, determinism and size.

mod common;

use common::*;
use std::fs;
use std::io::Read;
use std::path::Path;

/// @brief The file `name` in `gen`.
/// @version 1
fn file(gen: &Path, name: &str) -> Vec<u8> {
    fs::read(gen.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// @brief The JSON file `name` in `gen`.
/// @version 1
fn json(gen: &Path, name: &str) -> serde_json::Value {
    serde_json::from_slice(&file(gen, name)).unwrap()
}

#[test]
fn the_head_names_the_body_and_the_gzip_is_the_body() {
    use sha2::{Digest, Sha256};
    let dir = unique_dir("ks_head");
    let gen = dir.join("gen");
    generate_ok("v1_1", &gen, &["--variant", "sku_b"]);
    let body = file(&gen, "keyspace.sku_b.json");
    let head = json(&gen, "keyspace.sku_b.head.json");
    let hash: String = Sha256::digest(&body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(head["hash"], hash);
    assert_eq!(head["size"], body.len());
    assert_eq!(head["gz_size"], file(&gen, "keyspace.sku_b.json.gz").len());
    assert_eq!(
        (head["sku"].clone(), head["minor"].clone()),
        ("sku_b".into(), 1.into())
    );
    let mut unzipped = Vec::new();
    let gz = file(&gen, "keyspace.sku_b.json.gz");
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_end(&mut unzipped)
        .unwrap();
    assert_eq!(unzipped, body);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_resolved_body_declares_its_keys() {
    let dir = unique_dir("ks_body");
    let gen = dir.join("gen");
    generate_ok("v1_1", &gen, &["--variant", "sku_b"]);
    let body = json(&gen, "keyspace.sku_b.json");
    assert_eq!(body["schema_version"], "1.0");
    assert_eq!(body["contract"]["minor"], 1);
    let drive = body["namespaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "drive")
        .unwrap();
    let settings = &drive["classes"][0];
    assert_eq!(
        (settings["index"].clone(), settings["role"].clone()),
        (0.into(), "settings".into())
    );
    let limit = &settings["keys"][0];
    assert_eq!(limit["name"], "drive.settings.speed_limit");
    assert_eq!(
        limit["range"],
        serde_json::json!({"min": 0, "max": 2000, "step": 10})
    );
    assert_eq!(
        (limit["unit"].clone(), limit["present"].clone()),
        ("mm/s".into(), true.into())
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Whether some element of the array `v` satisfies `pred`.
/// @version 1
fn any(v: &serde_json::Value, pred: impl Fn(&serde_json::Value) -> bool) -> bool {
    v.as_array().unwrap().iter().any(pred)
}

#[test]
fn the_resolved_body_declares_enums_commands_rights_and_no_values() {
    let dir = unique_dir("ks_body2");
    let gen = dir.join("gen");
    generate_ok("v1_1", &gen, &["--variant", "sku_b"]);
    let body = json(&gen, "keyspace.sku_b.json");
    let codes: Vec<&serde_json::Value> = body["enums"][1]["values"] // enums sort by name: fan, mode
        .as_array()
        .unwrap()
        .iter()
        .map(|v| &v["code"])
        .collect();
    assert_eq!(codes, [0, 1, 3], "sku_b's resolved mode enum");
    assert!(any(&body["commands"], |c| c["name"]
        == "core.command.factory_reset"
        && c["owner"] == "core"));
    let net = body["rights"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["principal"] == "app.net")
        .unwrap();
    assert!(any(&net["keys"], |k| k["key"]
        == "net.credentials.wifi_psk"
        && k["access"] == "rw"));
    let text = String::from_utf8(file(&gen, "keyspace.sku_b.json")).unwrap();
    assert!(!text.contains("\"default\""), "no values in the keyspace");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_all_variants_file_overrides_the_base_per_variant() {
    let dir = unique_dir("ks_all");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let all = json(&gen, "keyspace.json");
    let mode = &all["enums"].as_array().unwrap()[1]; // enums sort by name: fan, mode
    assert_eq!(mode["name"], "drive::mode");
    assert_eq!(
        mode["values"][2],
        serde_json::json!({"code": 2, "name": "sport", "wire": "sportMode"})
    );
    assert_eq!(
        mode["overrides"],
        serde_json::json!({"sku_b": {"1": {"name": "saver", "wire": "saver"}, "2": {"present": false},
                                     "3": {"name": "turbo", "wire": "turbo"}}})
    );
    let key = &all["namespaces"][0]["classes"][0]["keys"][0];
    assert_eq!(
        key["present"],
        serde_json::json!({"sku_a": true, "sku_b": true})
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Every object property name in `v`, recursively.
/// @version 1
fn property_names(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                out.push(k.clone());
                property_names(x, out);
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| property_names(x, out)),
        _ => {}
    }
}

#[test]
fn no_property_names_a_ui_layout_or_endpoint() {
    let dir = unique_dir("ks_trust");
    let gen = dir.join("gen");
    generate_ok("v1_1", &gen, &[]);
    let schema = json(&gen, "keyspace.schema.json");
    let pattern = schema["$defs"]["trusted"]["propertyNames"]["not"]["pattern"]
        .as_str()
        .unwrap();
    let re = regex::Regex::new(pattern).unwrap();
    for ok in ["required", "uid", "build", "guid", "quiet", "unit"] {
        assert!(!re.is_match(ok), "{ok} must be allowed");
    }
    for bad in [
        "ui_hint", "label", "icon_url", "color", "colour", "widget", "endpoint",
    ] {
        assert!(re.is_match(bad), "{bad} must be refused");
    }
    for name in ["keyspace.json", "keyspace.sku_a.json"] {
        let mut names = Vec::new();
        property_names(&json(&gen, name), &mut names);
        let found: Vec<&String> = names.iter().filter(|n| re.is_match(n)).collect();
        assert!(found.is_empty(), "{name}: {found:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_schema_id_is_the_schema_file_in_the_ingot_repository() {
    let dir = unique_dir("ks_schema_id");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let schema = json(&gen, "keyspace.schema.json");
    assert_eq!(
        schema["$id"],
        "https://raw.githubusercontent.com/brandon-arrendondo/ingot/main/templates/keyspace.schema.json"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn keyspace_files_are_deterministic_and_small() {
    let dir = unique_dir("ks_det");
    let (a, b) = (dir.join("a"), dir.join("b"));
    generate_ok("v2_0", &a, &[]);
    generate_ok("v2_0", &b, &[]);
    for name in [
        "keyspace.json",
        "keyspace.sku_a.json",
        "keyspace.sku_a.json.gz",
        "keyspace.sku_a.head.json",
        "keyspace.schema.json",
    ] {
        assert_eq!(file(&a, name), file(&b, name), "{name}");
    }
    assert!(
        file(&a, "keyspace.sku_a.json.gz").len() < 64 * 1024,
        "per-variant image budget"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Whether the contract.json in `gen` has a key at `path`.
/// @version 1
fn has_key(gen: &Path, path: &str) -> bool {
    any(&contract_json(gen)["keys"], |k| k["path"] == path)
}

#[test]
fn a_key_limited_to_a_variant_exists_only_there() {
    let dir = unique_dir("ks_presence");
    let (a, b) = (dir.join("a"), dir.join("b"));
    generate_ok("v1_0", &a, &[]);
    generate_ok("v1_0", &b, &["--variant", "sku_b"]);
    let jam = "drive.state.error_dock_jam";
    assert!(!has_key(&a, jam) && has_key(&b, jam));
    let all = json(&a, "keyspace.json");
    let drive = all["namespaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "drive")
        .unwrap();
    let keys = drive["classes"][1]["keys"].as_array().unwrap();
    let declared = keys.iter().find(|k| k["name"] == jam).unwrap();
    assert_eq!(
        declared["present"],
        serde_json::json!({"sku_a": false, "sku_b": true})
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn error_groups_resolve_their_wire_base_per_variant() {
    let dir = unique_dir("ks_errors");
    let (a, b) = (dir.join("a"), dir.join("b"));
    generate_ok("v1_0", &a, &[]);
    generate_ok("v1_0", &b, &["--variant", "sku_b"]);
    let dock = |gen: &Path| contract_json(gen)["error_groups"][1].clone();
    assert_eq!(
        (dock(&a)["id"].clone(), dock(&a)["wire_base"].clone()),
        ("dock".into(), 32.into())
    );
    assert_eq!(dock(&b)["wire_base"], 50);
    let all = json(&a, "keyspace.json");
    let groups = all["error_groups"].as_array().unwrap();
    assert_eq!(
        groups[0]["bits"][0],
        serde_json::json!({"bit": 1, "key": "drive.state.error_overcurrent", "present": {"sku_a": true, "sku_b": true}})
    );
    assert_eq!(
        groups[1]["overrides"],
        serde_json::json!({"sku_b": {"wire_base": 50}})
    );
    assert_eq!(groups[0]["reserved_bits"], serde_json::json!([0]));
    let key = contract_json(&a)["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["path"] == "drive.state.error_overcurrent")
        .unwrap()["error"]
        .clone();
    assert_eq!(key, serde_json::json!({"group": "product", "bit": 1}));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rights_list_only_granted_keys_with_their_access() {
    let dir = unique_dir("ks_rights");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let body = json(&gen, "keyspace.sku_a.json");
    let of = |name: &str| {
        body["rights"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["principal"] == name)
            .unwrap()
            .clone()
    };
    let tool = of("tool");
    assert!(
        !any(&tool["keys"], |k| k["key"] == "drive.settings.mode"),
        "no rights, no entry"
    );
    assert!(any(&of("integrator")["keys"], |k| k["key"]
        == "core.command.factory_reset"
        && k["access"] == "r"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_gzip_header_carries_no_time_and_a_run_without_a_variant_is_base() {
    let dir = unique_dir("ks_base");
    let out = dir.join("out");
    let contract =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract/no_unix/contract.toml");
    let args = [
        "--contract",
        contract.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--allow-dirty-ingot",
    ];
    let (ok, stderr) = run_ingot(&args);
    assert!(ok, "{stderr}");
    let gz = file(&out, "keyspace.base.json.gz");
    assert_eq!(&gz[4..8], &[0, 0, 0, 0], "gzip mtime is 0");
    assert_eq!(json(&out, "keyspace.base.head.json")["sku"], "");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_refusal_in_another_variant_refuses_the_run() {
    // mode's sku_b default 2 is "sport", which sku_b removes: valid for sku_a, refused for
    // sku_b, so generating sku_a is refused too
    let dir = unique_dir("ks_other");
    let case = dir.join("model");
    let from = "enum = \"mode\"\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\ndefault = 1";
    let to = "enum = \"mode\"\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\ndefault = 1\ndefaults = { sku_b = 2 }";
    edited_copy("v1_0", &case, &[("ns/drive.toml", from, to)]);
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    let expected = "variant sku_b: drive.settings.mode: default 2 is not a value of the key's enum";
    assert!(!ok && stderr.contains(expected), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_grant_naming_a_key_of_another_variant_grants_nothing_here() {
    let dir = unique_dir("ks_grant_presence");
    let case = dir.join("model");
    let grant = "  { key = \"net.health.integrator_hb\", access = \"rw\" },";
    let both = "  { key = \"net.health.integrator_hb\", access = \"rw\" },\n  { key = \"drive.state.error_dock_jam\", access = \"rw\" },";
    edited_copy("v1_0", &case, &[("contract.toml", grant, both)]);
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    assert!(ok, "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_body_one_mqtt_message_cannot_carry_is_refused() {
    use sha2::{Digest, Sha256};
    // 256 KB of hex (incompressible to under the 96 KB a 128 KB base64 message holds)
    let mut block = Sha256::digest(b"seed").to_vec();
    let mut doc = String::new();
    while doc.len() < 256 * 1024 {
        block = Sha256::digest(&block).to_vec();
        doc.extend(block.iter().map(|b| format!("{b:02x}")));
    }
    let dir = unique_dir("ks_big");
    let case = dir.join("model");
    let with_doc = format!("id = \"label\"\ndoc = \"{doc}\"");
    edited_copy(
        "v1_0",
        &case,
        &[("ns/drive.toml", "id = \"label\"", &with_doc)],
    );
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    assert!(
        !ok && stderr.contains("over one 131072-byte MQTT message as base64"),
        "{stderr}"
    );
    assert!(
        !case.join("out/dmt_tables.c").exists(),
        "nothing is generated"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_body_names_its_sku_and_orders_namespaces_by_id() {
    let dir = unique_dir("ks_order");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &["--variant", "sku_b"]);
    let body = json(&gen, "keyspace.sku_b.json");
    assert_eq!(body["sku"], "sku_b");
    let ids: Vec<u64> = body["namespaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_u64().unwrap())
        .collect();
    assert_eq!(
        ids,
        [1, 2, 3, 4],
        "files sort core, drive, mfg, net; IDs are 1, 3, 2, 4"
    );
    assert_eq!(
        contract_json(&gen)["error_groups"][0]["publish_max_bit"],
        31
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn group_bits_are_listed_by_bit() {
    let dir = unique_dir("ks_bits");
    let case = dir.join("model");
    edited_copy(
        "v1_0",
        &case,
        &[(
            "ns/drive.toml",
            "id = \"enabled\"",
            "id = \"enabled\"\nerror = { group = \"product\", bit = 2 }",
        )],
    );
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    assert!(ok, "{stderr}");
    let bits: Vec<u64> = json(&case.join("out"), "keyspace.json")["error_groups"][0]["bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["bit"].as_u64().unwrap())
        .collect();
    assert_eq!(
        bits,
        [1, 2],
        "enabled (bit 2) sorts before error_overcurrent (bit 1) by key ID"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The map_edit command limited to sku_b (the key grants naming its keys grant nothing on sku_a).
const MAP_EDIT_SKU_B: &[(&str, &str, &str)] = &[(
    "ns/drive.toml",
    "owner = \"app.map\"",
    "owner = \"app.map\"\nvariants = [\"sku_b\"]",
)];

#[test]
fn a_command_limited_to_a_variant_exists_only_there() {
    let dir = unique_dir("ks_cmd_presence");
    let case = dir.join("model");
    edited_copy("v2_0", &case, MAP_EDIT_SKU_B); // 2.0 maps no commands to the wire
    let (a, b) = (case.join("a"), case.join("b"));
    for (out, v) in [(&a, "sku_a"), (&b, "sku_b")] {
        let (ok, stderr) = generate(&case.join("contract.toml"), out, &["--variant", v]);
        assert!(ok, "{v}: {stderr}");
    }
    let cmd = "drive.command.map_edit";
    assert!(!has_key(&a, cmd) && has_key(&b, cmd));
    let _ = fs::remove_dir_all(&dir);
}
