//! The shadow tables `dsh_tables.h` / `dsh_tables.c`: compiled and
//! walked in C (tests/fixtures/dsh/check_dsh.c), and their rows checked against the generic
//! model and edited copies of it.

mod common;

use common::*;
use std::fs;
use std::path::{Path, PathBuf};

/// @brief Generate the 1.1 model with `edits` applied for `variant`; returns the output dir.
/// @version 1
fn generated(dir: &Path, tag: &str, variant: &str, edits: &[(&str, &str, &str)]) -> PathBuf {
    let case = dir.join(tag);
    edited_copy("v1_1", &case, edits);
    let out = case.join(variant);
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
    assert!(ok, "{tag} ({variant}): {stderr}");
    out
}

/// @brief The initialiser rows of table `name` in dsh_tables.c.
/// @version 1
fn table(gen: &Path, name: &str) -> Vec<String> {
    let c = fs::read_to_string(gen.join("dsh_tables.c")).unwrap();
    let start = c
        .find(&format!(" {name}[] = {{\n"))
        .unwrap_or_else(|| panic!("{name}"));
    let body = &c[start..];
    let body = &body[body.find('\n').unwrap() + 1..body.find("\n};").unwrap()];
    body.lines()
        .map(|l| l.trim().trim_end_matches(',').to_string())
        .collect()
}

/// @brief The one row of `rows` containing `needle`.
/// @version 1
fn row(rows: &[String], needle: &str) -> String {
    let found: Vec<&String> = rows.iter().filter(|r| r.contains(needle)).collect();
    assert_eq!(found.len(), 1, "one row with {needle} in {rows:#?}");
    found[0].clone()
}

/// @brief The bytes a row's `.value = { off, len }` names in dsh_value_blob.
/// @version 1
fn value(gen: &Path, row: &str) -> Vec<u8> {
    let blob: Vec<u8> = table(gen, "dsh_value_blob")
        .join(", ")
        .split(", ")
        .map(|b| u8::from_str_radix(b.trim_start_matches("0x").trim_end_matches('U'), 16).unwrap())
        .collect();
    let at = &row[row.find(".value = { ").unwrap() + 11..];
    let nums: Vec<usize> = at[..at.find(" }").unwrap()]
        .split(", ")
        .map(|n| n.trim_end_matches('u').parse().unwrap())
        .collect();
    blob[nums[0]..nums[0] + nums[1]].to_vec()
}

/// @brief The value of `#define name` in dsh_tables.h.
/// @version 1
fn define(gen: &Path, name: &str) -> String {
    let h = fs::read_to_string(gen.join("dsh_tables.h")).unwrap();
    let line = h
        .lines()
        .find(|l| l.starts_with(&format!("#define {name} ")))
        .unwrap();
    line.split_whitespace().nth(2).unwrap().to_string()
}

#[test]
fn the_tables_compile_and_walk_on_every_variant() {
    let dir = unique_dir("dsh_walk");
    for variant in ["sku_a", "sku_b"] {
        let gen = generated(&dir, "plain", variant, &[]);
        walk_dsh_tables(&gen);
        for (name, count) in [
            ("dsh_desired", "DSH_NUM_DESIRED"),
            ("dtp_topics", "DTP_NUM_TOPICS"),
        ] {
            assert_eq!(define(&gen, count), format!("{}u", table(&gen, name).len()));
        }
        assert_eq!(
            define(&gen, "DSH_NUM_ALLOWED"),
            "0u",
            "an empty table counts 0"
        );
        assert_eq!(
            table(&gen, "dsh_allowed"),
            vec!["0"],
            "and holds one zero row"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_header_carries_the_body_hash_version_and_sizes() {
    let dir = unique_dir("dsh_header");
    let gen = generated(&dir, "plain", "sku_b", &[]);
    let read = |f: &str| -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(gen.join(f)).unwrap()).unwrap()
    };
    let hash = read("keyspace.sku_b.head.json")["hash"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(define(&gen, "DSH_CONTRACT_HASH"), format!("\"{hash}\""));
    assert_eq!(define(&gen, "DSH_CONTRACT_VERSION"), "\"1.1.0\"");
    assert_eq!(define(&gen, "DSH_VARIANT"), "\"sku_b\"");
    let reported = table(&gen, "dsh_reported");
    assert!(
        row(&reported, "\"config.cvHash\"").contains(&format!(".constant = \"\\\"{hash}\\\"\""))
    );
    assert!(row(&reported, "\"config.cv\"").contains(".constant = \"\\\"1.1.0\\\"\""));
    assert!(row(&reported, "\"sv\"").contains(".constant = \"\\\"1.0-gen\\\"\""));
    let shadow = &read("keyspace.sku_b.json")["shadow"];
    assert_eq!(
        define(&gen, "DSH_FULL_MAX_BYTES"),
        format!("{}u", shadow["full_max_bytes"])
    );
    assert_eq!(
        define(&gen, "DSH_DELTA_FORCED_BYTES"),
        format!("{}u", shadow["delta_forced_bytes"])
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn enums_carry_outbound_codes_and_inbound_strings_per_variant() {
    let dir = unique_dir("dsh_enums");
    let a = generated(&dir, "plain", "sku_a", &[]);
    let b = generated(&dir, "plain", "sku_b", &[]);
    assert!(table(&a, "dsh_outbound").contains(&"{ .code = 0, .wire = \"quiet\" }".to_string()));
    assert!(table(&b, "dsh_outbound").contains(&"{ .code = 0, .wire = \"whisper\" }".to_string()));
    let inbound = table(&b, "dsh_inbound");
    for (text, code) in [("quiet", 0), ("whisper", 0), ("max", 2), ("high", 2)] {
        assert!(
            inbound.contains(&format!("{{ .text = \"{text}\", .code = {code} }}")),
            "{text}"
        );
    }
    let fan = row(&table(&b, "dsh_enums"), "\"drive::fan\"");
    assert!(
        fan.contains(".out_count = 3u") && fan.contains(".in_count = 5u"),
        "{fan}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The fan key's inbound rule in the 1.1 model.
const FAN_IN: &str = "inbound_unknown = \"quiet\"";

#[test]
fn desired_rows_carry_unknown_rules_ranges_groups_and_rejects() {
    let dir = unique_dir("dsh_desired");
    let gen = generated(&dir, "plain", "sku_a", &[]);
    let desired = table(&gen, "dsh_desired");
    assert!(
        desired[0].contains("\"settings.fanSpeed\"")
            && desired[1].contains("\"settings.speedLimit\"")
    );
    let fan = &desired[0];
    for part in [
        ".inbound_unknown = DSH_IN_CODE",
        ".inbound_unknown_code = 0,",
        ".form = DSH_FORM_ENUM",
        ".apply_group = 0u",
        ".on_reject = DSH_REJ_ECHO",
        ".implies_count = 1u",
        ".flags = 0u",
    ] {
        assert!(fan.contains(part), "{part}: {fan}");
    }
    let limit = &desired[1];
    for part in [
        ".min = 0, .max = 2000, .step = 10",
        ".flags = DSH_DF_RANGE",
        ".on_reject = DSH_REJ_CLEAR_DESIRED",
        ".inbound_unknown = DSH_IN_NONE",
        ".enum_index = DSH_NONE_U16",
        ".type = 2u",
        ".max_size = 2u",
    ] {
        assert!(limit.contains(part), "{part}: {limit}");
    }
    let implies = row(&table(&gen, "dsh_implies"), ".key");
    assert_eq!(value(&gen, &implies), 500u16.to_le_bytes().to_vec());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn each_unknown_inbound_rule_reaches_the_binder() {
    let dir = unique_dir("dsh_inbound_rules");
    for (rule, expect) in [
        ("max", "DSH_IN_CODE"),
        ("ignore", "DSH_IN_IGNORE"),
        ("reject", "DSH_IN_REJECT"),
    ] {
        let to = format!("inbound_unknown = \"{rule}\"");
        let g = generated(&dir, rule, "sku_a", &[("ns/drive.toml", FAN_IN, &to)]);
        let fan = &table(&g, "dsh_desired")[0];
        assert!(
            fan.contains(&format!(".inbound_unknown = {expect}")),
            "{rule}: {fan}"
        );
        let code = if rule == "max" { 2 } else { 0 };
        assert!(
            fan.contains(&format!(".inbound_unknown_code = {code},")),
            "{rule}: {fan}"
        );
    }
    let two = "shadow = [\"settings.fanSpeed\", { path = \"settings.fanLevel\", as = \"integer\" }]\napply_group";
    let anchor = "shadow = \"settings.fanSpeed\"\napply_group";
    let g = generated(&dir, "two", "sku_a", &[("ns/drive.toml", anchor, two)]);
    let level = row(&table(&g, "dsh_desired"), "\"settings.fanLevel\"");
    assert!(
        level.contains(".inbound_unknown = DSH_IN_NONE"),
        "an integer path has no unknown string: {level}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_discrete_set_a_sentinel_and_an_integer_form_reach_the_binder() {
    let dir = unique_dir("dsh_allowed");
    let twin = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"max_accel_applied\"\nkey_index = 14\ntype = \"uint16\"\nintroduced_in = \"1.1\"\nshadow = \"state.maxAccel\"\napplies = \"drive.settings.max_accel\"";
    let accel = "allowed = [250, 500, 1000]\nshadow = \"settings.maxAccel\"\nunset = 0";
    let fan = "shadow = { path = \"settings.fanSpeed\", as = \"integer\" }\napply_group";
    let edits = [
        ("ns/drive.toml", "telem = \"dock\"", twin),
        ("ns/drive.toml", "allowed = [250, 500, 1000]", accel),
        ("ns/drive.toml", "default = 500", "default = 0"),
        ("ns/drive.toml", "shadow = \"settings.fanSpeed\"\napply_group", fan),
        ("ns/drive.toml", "inbound_unknown = \"quiet\"            # an unmatched string is quiet (code 0), as today\n", ""),
    ];
    let gen = generated(&dir, "set", "sku_a", &edits);
    let desired = table(&gen, "dsh_desired");
    let accel = row(&desired, "\"settings.maxAccel\"");
    for part in [
        ".allowed_first = 0u, .allowed_count = 3u",
        ".unset = 0",
        ".flags = DSH_DF_UNSET",
    ] {
        assert!(accel.contains(part), "{part}: {accel}");
    }
    assert_eq!(table(&gen, "dsh_allowed"), vec!["250", "500", "1000"]);
    let fan = row(&desired, "\"settings.fanSpeed\"");
    assert!(
        fan.contains(".form = DSH_FORM_INTEGER") && !fan.contains(".enum_index = DSH_NONE_U16"),
        "{fan}"
    );
    assert!(fan.contains(".inbound_unknown = DSH_IN_NONE"), "{fan}");
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn echo_rows_are_sorted_with_the_key_commands_apply() {
    let dir = unique_dir("dsh_echo");
    let gen = generated(&dir, "plain", "sku_a", &[]);
    let echo = table(&gen, "dsh_echo");
    assert_eq!(
        echo[0],
        "{ .path = \"settings.planExists\", .command_key = 0x00000000U, .max_size = 0u, .type = DSH_ECHO_BOOL, .command_key_type = 0u }"
    );
    assert!(echo[1].contains("\"settings.trim\"") && echo[1].contains(".type = DSH_ECHO_FLOAT"));
    let gain = key_json(&gen, "drive.settings.gain")["id"]
        .as_u64()
        .unwrap();
    assert!(
        echo[1].contains(&format!(".command_key = {gain:#010X}U, .max_size = 0u")),
        "{}",
        echo[1]
    );
    assert!(echo[1].contains(".command_key_type = 7u"));
    let later = "type = \"float\"\ncommand_key = \"drive.settings.gain\"  # the delta path echoes it; command bodies apply it\n\n[[shadow_echo]]\npath = \"settings.alpha\"\ntype = \"string\"\nmax_size = 4";
    let anchor = "type = \"float\"\ncommand_key = \"drive.settings.gain\"  # the delta path echoes it; command bodies apply it";
    let gen = generated(&dir, "later", "sku_a", &[("contract.toml", anchor, later)]);
    let echo = table(&gen, "dsh_echo");
    assert!(
        echo[0].contains("\"settings.alpha\"")
            && echo[0].contains(".max_size = 4u, .type = DSH_ECHO_STRING"),
        "sorted by path: {echo:?}"
    );
    let reported = table(&gen, "dsh_reported");
    assert!(
        row(&reported, "\"settings.trim\"").contains(".ref = 2u"),
        "{reported:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The path of every row of dsh_reported, in order.
/// @version 1
fn reported_paths(gen: &Path) -> Vec<String> {
    table(gen, "dsh_reported")
        .iter()
        .map(|r| r.split('"').nth(1).unwrap().to_string())
        .collect()
}

#[test]
fn reported_rows_are_in_emission_order_with_their_kinds() {
    let dir = unique_dir("dsh_reported");
    let gen = generated(&dir, "plain", "sku_a", &[]);
    let order = [
        "state.connected",
        "state.errors",
        "state.fanSpeed",
        "state.tones",
        "state.docked",
        "sv",
        "config.cv",
        "config.cvHash",
        "settings.planExists",
        "settings.trim",
        "settings.fanSpeed",
        "settings.speedLimit",
    ];
    assert_eq!(reported_paths(&gen), order);
    let rows = table(&gen, "dsh_reported");
    let at = |p: &str| row(&rows, &format!(".path = \"{p}\""));
    assert!(
        at("state.connected").contains(".kind = DSH_RK_CONSTANT")
            && at("state.connected").contains(".constant = \"true\"")
    );
    assert!(at("state.connected")
        .contains(".scope = DSH_SC_FULL | DSH_SC_DELTA, .flags = DSH_RF_FORCED"));
    assert!(at("sv").contains(".scope = DSH_SC_FULL, .flags = 0u"));
    assert!(at("state.errors").contains(".kind = DSH_RK_ERRORS"));
    assert!(
        at("settings.trim").contains(".kind = DSH_RK_ECHO")
            && at("settings.trim").contains(".ref = 1u")
    );
    assert!(
        at("state.docked").contains(".flags = DSH_RF_FORCED")
            && at("state.docked").contains(".telem = \"dock\"")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn key_rows_carry_their_form_unknown_rule_and_twin() {
    let dir = unique_dir("dsh_reported_keys");
    let gen = generated(&dir, "plain", "sku_a", &[]);
    let rows = table(&gen, "dsh_reported");
    let at = |p: &str| row(&rows, &format!(".path = \"{p}\""));
    let tones = at("state.tones");
    for part in [
        ".form = DSH_FORM_CSV_INDEX_LIST",
        ".max_items = 4u",
        ".type = 8u",
        ".enum_index = 2u",
    ] {
        assert!(tones.contains(part), "{part}: {tones}");
    }
    let fan = at("state.fanSpeed");
    for part in [
        ".outbound_unknown = DSH_OUT_WIRE",
        ".outbound_unknown_wire = \"normal\"",
        ".form = DSH_FORM_ENUM",
        ".kind = DSH_RK_KEY",
    ] {
        assert!(fan.contains(part), "{part}: {fan}");
    }
    let applies = key_json(&gen, "drive.settings.fan")["id"].as_u64().unwrap();
    assert!(
        fan.contains(&format!(".applies = {applies:#010X}U")),
        "{fan}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_raw_unknown_an_integer_path_and_a_scope_reach_the_reporter() {
    let dir = unique_dir("dsh_reported_forms");
    let fan = "shadow = [\"state.fanSpeed\", { path = \"state.fanLevel\", as = \"integer\", scope = \"delta\" }, \"settings.fanSpeed\"]";
    let edits = [
        (
            "ns/drive.toml",
            "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]",
            fan,
        ),
        (
            "ns/drive.toml",
            "outbound_unknown = \"normal\"",
            "outbound_unknown = \"raw\"",
        ),
        (
            "ns/drive.toml",
            "shadow = \"state.tones\"",
            "shadow = { path = \"state.tones\", scope = \"full\" }",
        ),
        (
            "ns/drive.toml",
            "shadow = \"state.docked\"\nforced",
            "shadow = [\"state.docked\", { path = \"state.dockedFull\", scope = \"full\" }]\nforced",
        ),
    ];
    let gen = generated(&dir, "forms", "sku_a", &edits);
    let full_only = row(&table(&gen, "dsh_reported"), "\"state.dockedFull\"");
    assert!(
        full_only.contains(".scope = DSH_SC_FULL, .flags = 0u"),
        "a forced key's full-only path is not forced: {full_only}"
    );
    let rows = table(&gen, "dsh_reported");
    let at = |p: &str| row(&rows, &format!(".path = \"{p}\""));
    assert!(at("state.fanSpeed").contains(".outbound_unknown = DSH_OUT_RAW"));
    assert!(at("state.fanSpeed").contains(".outbound_unknown_wire = NULL"));
    let level = at("state.fanLevel");
    for part in [
        ".form = DSH_FORM_INTEGER",
        ".outbound_unknown = DSH_OUT_NONE",
        ".scope = DSH_SC_DELTA, .flags = 0u",
    ] {
        assert!(level.contains(part), "{part}: {level}");
    }
    assert!(
        at("state.tones").contains(".scope = DSH_SC_FULL, .flags = 0u"),
        "a full-only path"
    );
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_bool_path_reports_0_1_in_one_document_and_true_false_in_the_other() {
    let dir = unique_dir("dsh_bool_integer");
    let docked = "shadow = [{ path = \"state.docked\", as = \"integer\", scope = \"full\" }, { path = \"state.docked\", scope = \"delta\" }]\nforced";
    let edits = [("ns/drive.toml", "shadow = \"state.docked\"\nforced", docked)];
    let gen = generated(&dir, "bool_integer", "sku_a", &edits);
    let rows = table(&gen, "dsh_reported");
    let docked_rows: Vec<&String> = rows
        .iter()
        .filter(|r| r.contains(".path = \"state.docked\""))
        .collect();
    assert_eq!(docked_rows.len(), 2, "{rows:#?}");
    let full = row(&rows, ".form = DSH_FORM_BOOL_INTEGER");
    let delta = row(&rows, ".scope = DSH_SC_DELTA, .flags = DSH_RF_FORCED");
    assert!(
        full.contains(".path = \"state.docked\"")
            && full.contains(".scope = DSH_SC_FULL, .flags = 0u"),
        "{full}"
    );
    assert!(
        delta.contains(".path = \"state.docked\"") && delta.contains(".form = DSH_FORM_PLAIN"),
        "{delta}"
    );
    assert!(full.contains(".enum_index = DSH_NONE_U16"), "{full}");
    let docked = key_json(&gen, "drive.state.docked");
    assert_eq!(docked["shadow"], serde_json::json!(["state.docked"]));
    assert_eq!(docked["shadow_forms"][0]["as"], "integer");
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_rejected_settings_list_exists_only_on_its_variants() {
    let dir = unique_dir("dsh_rejected_variants");
    let at = "[[shadow_constant]]\npath = \"state.connected\"";
    let declared =
        format!("[shadow_rejected]\npath = \"state.rejected\"\nvariants = [\"sku_b\"]\n\n{at}");
    let edits = [("contract.toml", at, declared.as_str())];
    let listed = generated(&dir, "listed", "sku_b", &edits);
    row(&table(&listed, "dsh_reported"), ".kind = DSH_RK_REJECTED");
    let shadow = &contract_json(&listed)["shadow"]["rejected"];
    assert_eq!(shadow["variants"], serde_json::json!(["sku_b"]));
    let other = generated(&dir, "other", "sku_a", &edits);
    assert!(contract_json(&other)["shadow"].get("rejected").is_none());
    let c = fs::read_to_string(other.join("dsh_tables.c")).unwrap();
    assert!(
        !c.contains("DSH_RK_REJECTED"),
        "no row on a variant not listed"
    );
    walk_dsh_tables(&listed);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_rejected_settings_list_is_a_delta_row_at_its_declared_path() {
    let dir = unique_dir("dsh_rejected");
    let at = "[[shadow_constant]]\npath = \"state.connected\"";
    let declared = format!("[shadow_rejected]\npath = \"state.rejected\"\n\n{at}");
    let gen = generated(
        &dir,
        "rejected",
        "sku_a",
        &[("contract.toml", at, &declared)],
    );
    let rejected = row(&table(&gen, "dsh_reported"), ".kind = DSH_RK_REJECTED");
    for part in [
        ".path = \"state.rejected\"",
        ".constant = NULL",
        ".key = 0x00000000U",
        ".scope = DSH_SC_DELTA, .flags = 0u",
    ] {
        assert!(rejected.contains(part), "{part}: {rejected}");
    }
    let shadow = &contract_json(&gen)["shadow"];
    assert_eq!(shadow["rejected"]["path"], "state.rejected");
    let plain = generated(&dir, "plain", "sku_a", &[]);
    assert!(contract_json(&plain)["shadow"].get("rejected").is_none());
    assert!(!fs::read_to_string(plain.join("dsh_tables.c"))
        .unwrap()
        .contains("DSH_RK_REJECTED"));
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn error_groups_carry_their_variant_base_reserved_bits_and_sorted_bits() {
    let dir = unique_dir("dsh_errors");
    let a = generated(&dir, "plain", "sku_a", &[]);
    let b = generated(&dir, "plain", "sku_b", &[]);
    let product = row(&table(&a, "dsh_error_groups"), "\"product\"");
    for part in [
        ".reserved = 0x1ULL",
        ".wire_base = 0u",
        ".bits_first = 0u, .bits_count = 1u",
        ".width = 32u, .publish_max_bit = 31u",
        ".shadow = \"state.errors\"",
    ] {
        assert!(product.contains(part), "{part}: {product}");
    }
    assert!(row(&table(&a, "dsh_error_groups"), "\"dock\"")
        .contains(".wire_base = 32u, .bits_first = 1u, .bits_count = 0u"));
    assert!(row(&table(&b, "dsh_error_groups"), "\"dock\"")
        .contains(".wire_base = 50u, .bits_first = 1u, .bits_count = 1u"));
    let over = key_json(&b, "drive.state.error_overcurrent")["id"]
        .as_u64()
        .unwrap();
    let jam = key_json(&b, "drive.state.error_dock_jam")["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        table(&b, "dsh_error_bits"),
        vec![
            format!("{{ .key = {over:#010X}U, .bit = 1u }}"),
            format!("{{ .key = {jam:#010X}U, .bit = 0u }}")
        ]
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief A `.name = <n>u` field of a row, as a number.
/// @version 1
fn field(row: &str, name: &str) -> usize {
    let at = &row[row
        .find(&format!(".{name} = "))
        .unwrap_or_else(|| panic!("{name}: {row}"))
        + name.len()
        + 4..];
    at[..at.find('u').unwrap()].parse().unwrap()
}

/// @brief The steps of a dsh_commands row, each with its argument rows.
/// @version 1
fn steps(gen: &Path, command: &str) -> Vec<(String, Vec<String>)> {
    let (all, args) = (table(gen, "dsh_steps"), table(gen, "dsh_args"));
    let first = field(command, "steps_first");
    all[first..first + field(command, "steps_count")]
        .iter()
        .map(|s| {
            let a = field(s, "args_first");
            (s.clone(), args[a..a + field(s, "args_count")].to_vec())
        })
        .collect()
}

#[test]
fn command_rows_carry_selectors_bodies_steps_and_arguments() {
    let dir = unique_dir("dsh_commands");
    let gen = generated(&dir, "plain", "sku_b", &[]);
    let rows = table(&gen, "dsh_commands");
    let reset = row(&rows, "\"net.factoryReset\"");
    assert!(reset.contains(".kind = DSH_SEL_TRUE") && reset.contains(".action = DSH_DO_STEPS"));
    let reset_id = key_json(&gen, "core.command.factory_reset")["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        steps(&gen, &reset)[0].0,
        format!("{{ .command = {reset_id:#010X}U, .args_first = 0u, .args_count = 0u, .command_type = 9u }}")
    );
    let object = row(&rows, "DSH_SEL_OBJECT");
    for part in [
        ".body = DSH_BODY_SETTINGS",
        ".action = DSH_DO_SETTINGS",
        ".steps_count = 0u",
        ".topic = 2u",
    ] {
        assert!(object.contains(part), "{part}: {object}");
    }
    let mapcmd = row(&rows, "\"mapcmd\"");
    assert!(mapcmd.contains(".body_selector = { .field = \"cmd\", .wire = \"saveMap\", .kind = DSH_SEL_TEXT }, .body_object = \"map\", .body = DSH_BODY_OBJECT"), "{mapcmd}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn command_array_and_field_arguments_carry_their_presence_rules() {
    let dir = unique_dir("dsh_command_args");
    let gen = generated(&dir, "plain", "sku_b", &[]);
    let rows = table(&gen, "dsh_commands");
    let split = row(&rows, "\"splitArea\"");
    assert!(
        split.contains(".kind = DSH_SEL_ELEMENT")
            && split.contains(".protocol = DSH_PROTO_COMMAND_ARRAY")
            && split.contains(".topic = 4u")
    );
    let args = &steps(&gen, &split)[0].1;
    assert!(args[0].contains(".field = \"mapid\"") && args[0].contains(".flags = DSH_AF_ABSENT"));
    assert_eq!(value(&gen, &args[0]), vec![0, 0]);
    assert!(
        args[1].contains(".flags = DSH_AF_REQUIRED, .ec = 5u"),
        "{}",
        args[1]
    );
    let save = steps(
        &gen,
        &rows
            .iter()
            .find(|r| r.contains("\"saveMap\", .kind = DSH_SEL_ELEMENT"))
            .unwrap()
            .clone(),
    );
    let data = &save[0].1[1];
    assert!(
        data.contains(".value = { 0u, 0u }")
            && data.contains(".flags = DSH_AF_OMIT")
            && data.contains(".encoding = DSH_ENC_BASE64"),
        "{data}"
    );
    let plan = &steps(&gen, &row(&rows, "\"approve\""))[0].1[0];
    assert!(plan.contains(".type = 3u") && plan.contains(".max_size = 4u"));
    assert_eq!(value(&gen, plan), vec![0, 0, 0, 0]);
    let _ = fs::remove_dir_all(&dir);
}

/// Keys added to the drive command class for the argument forms.
const ARG_KEYS: &str = "doc = \"Base64 text of a map request, passed through as text.\"\n\n[[classes.keys]]\nid = \"drive_mode\"\nkey_index = 10\ntype = \"uint8\"\nenum = \"mode\"\nintroduced_in = \"1.1\"\n\n[[classes.keys]]\nid = \"plan_blob\"\nkey_index = 11\ntype = \"binary\"\nmax_size = 96\nintroduced_in = \"1.1\"\n\n[[classes.keys]]\nid = \"path_from\"\nkey_index = 12\ntype = \"int16\"\nintroduced_in = \"1.1\"";

/// The go and stop rows of the 1.1 model.
const GO_ROW: &str =
    "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"";
const STOP_ROW: &str =
    "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"";

#[test]
fn argument_forms_resolve_names_elements_and_encodings() {
    let dir = unique_dir("dsh_args");
    let go = format!("{GO_ROW}\nvariants = [\"sku_a\"]\ngrounded_in = \"the sku_a build\"\nargs = [ {{ key = \"drive.command.drive_mode\", value = \"eco\" }} ]");
    let stop = format!("{STOP_ROW}\nvariants = [\"sku_a\"]\ngrounded_in = \"the sku_a build\"\nargs = [\n  {{ field = \"mode\", key = \"drive.command.drive_mode\", absent = \"sport\" }},\n  {{ field = \"plan\", key = \"drive.command.plan_blob\", omit_when_absent = true, encoding = \"base64\" }},\n  {{ field = \"pathRange\", element = 1, key = \"drive.command.path_from\", absent = -1, encoding = \"decimal_text\" }},\n]");
    let anchor = "doc = \"Base64 text of a map request, passed through as text.\"";
    let edits = [
        ("ns/drive.toml", anchor, ARG_KEYS),
        ("ns/drive.toml", GO_ROW, go.as_str()),
        ("ns/drive.toml", STOP_ROW, stop.as_str()),
    ];
    let gen = generated(&dir, "args", "sku_a", &edits);
    let rows = table(&gen, "dsh_commands");
    let constant = &steps(&gen, &row(&rows, "\"go\""))[0].1[0];
    assert!(
        constant.contains(".field = NULL") && constant.contains(".flags = DSH_AF_CONSTANT"),
        "{constant}"
    );
    assert!(
        !constant.contains(".enum_index = DSH_NONE_U16"),
        "an enum key's argument names its enum"
    );
    assert_eq!(value(&gen, constant), vec![1], "eco is 1");
    let stop = &steps(&gen, &row(&rows, "\"stop\""))[0].1;
    assert_eq!(value(&gen, &stop[0]), vec![2], "sport is 2");
    assert!(stop[1].contains(".type = 9u") && stop[1].contains(".encoding = DSH_ENC_BASE64"));
    assert!(
        stop[2].contains(".element = 1u") && stop[2].contains(".encoding = DSH_ENC_DECIMAL_TEXT")
    );
    assert!(stop[0].contains(".element = DSH_NONE_U16"));
    assert_eq!(value(&gen, &stop[2]), vec![0xff, 0xff]);
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_internal_row_carries_its_declared_id() {
    let dir = unique_dir("dsh_internal_id");
    let rows = format!("{GO_ROW}\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"getlog\" }}\ninternal = \"the library uploads its own log\"\ninternal_id = 7");
    let gen = generated(&dir, "id", "sku_a", &[("ns/drive.toml", GO_ROW, &rows)]);
    let all = table(&gen, "dsh_commands");
    let getlog = row(&all, "\"getlog\"");
    assert!(getlog.contains(".internal_id = 7u"), "{getlog}");
    let go = row(&all, ".wire = \"go\"");
    assert!(go.contains(".internal_id = 0u"), "{go}");
    let json = &contract_json(&gen)["shadow"]["commands"];
    let declared = json
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["internal_id"] == 7);
    assert!(declared.is_some(), "{json}");
    assert!(json[0].get("internal_id").is_none());
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rows_without_a_command_answer_stay_internal_or_write_state() {
    let dir = unique_dir("dsh_commandless");
    let rows = format!("{GO_ROW}\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"ping\", wire = false }}\nanswer = 3\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"getlog\" }}\ninternal = \"the library uploads its own log\"\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"mobile\" }}\nargs = [ {{ field = \"label\", key = \"drive.state.label\", omit_when_absent = true }} ]");
    let gen = generated(
        &dir,
        "commandless",
        "sku_a",
        &[("ns/drive.toml", GO_ROW, &rows)],
    );
    let all = table(&gen, "dsh_commands");
    let ping = row(
        &all,
        ".field = \"ping\", .wire = NULL, .kind = DSH_SEL_FALSE",
    );
    assert!(
        ping.contains(".action = DSH_DO_ANSWER, .answer = 3u")
            && ping.contains(".steps_count = 0u"),
        "{ping}"
    );
    let getlog = row(&all, "\"getlog\"");
    assert!(
        getlog.contains(".internal = \"the library uploads its own log\"")
            && getlog.contains(".action = DSH_DO_INTERNAL")
    );
    assert!(getlog.contains(".steps_count = 0u"));
    let mobile = steps(&gen, &row(&all, "\"mobile\""));
    assert!(
        mobile[0].0.starts_with("{ .command = 0x00000000U,"),
        "{}",
        mobile[0].0
    );
    assert!(mobile[0].1[0].contains(".field = \"label\"") && mobile[0].1[0].contains(".type = 8u"));
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_pub_topic_declares_the_vars_its_publisher_fills() {
    let dir = unique_dir("dtp_vars");
    let telemetry = "template = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"";
    let with_vars = "template = \"{stage}/{duid}/telemetry/{session}/{event}/{session}\"\ndirection = \"pub\"\nvars = [{ name = \"session\", max_len = 16 }, { name = \"event\", max_len = 24 }]";
    let gen = generated(
        &dir,
        "vars",
        "sku_a",
        &[("contract.toml", telemetry, with_vars)],
    );
    assert_eq!(
        table(&gen, "dtp_vars"),
        vec![
            "{ .name = \"session\", .max_len = 16u }",
            "{ .name = \"event\", .max_len = 24u }"
        ]
    );
    let topics = table(&gen, "dtp_topics");
    let t = row(&topics, "\"telemetry\"");
    assert!(t.contains(".vars_first = 0u, .vars_count = 2u"), "{t}");
    let other = row(&topics, "\"contract_resp\"");
    assert!(
        other.contains(".vars_first = 2u, .vars_count = 0u"),
        "{other}"
    );
    let h = fs::read_to_string(gen.join("dsh_tables.h")).unwrap();
    assert!(h.contains("#define DTP_NUM_VARS 2u"), "{h}");
    let json = &contract_json(&gen)["shadow"]["topics"];
    let declared = json
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "telemetry")
        .unwrap();
    assert_eq!(declared["vars"][1]["name"], "event");
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Whether the tables and contract.json in `gen` both have topic `id`.
/// @version 1
fn has_topic(gen: &Path, id: &str) -> bool {
    let in_table = table(gen, "dtp_topics")
        .iter()
        .any(|r| r.contains(&format!(".id = \"{id}\"")));
    let json = contract_json(gen)["shadow"]["topics"].clone();
    let in_json = json.as_array().unwrap().iter().any(|t| t["id"] == id);
    assert_eq!(in_table, in_json, "{id}: tables and contract.json agree");
    in_table
}

#[test]
fn a_topic_exists_only_on_its_variants() {
    let dir = unique_dir("dtp_topic_variants");
    let telemetry = "template = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"";
    let limited = format!("{telemetry}\nvariants = [\"sku_b\"]");
    let edits = [("contract.toml", telemetry, limited.as_str())];
    let listed = generated(&dir, "listed", "sku_b", &edits);
    assert!(has_topic(&listed, "telemetry"));
    let other = generated(&dir, "other", "sku_a", &edits);
    assert!(!has_topic(&other, "telemetry"));
    walk_dsh_tables(&listed);
    walk_dsh_tables(&other);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_sub_topic_declares_the_vars_its_consumer_extracts() {
    let dir = unique_dir("dtp_sub_vars");
    let cmd = "template = \"{stage}/{duid}/cmd/req\"\ndirection = \"sub\"";
    let per_job = "template = \"{stage}/{duid}/cmd/{job}/req\"\ndirection = \"sub\"\nvars = [{ name = \"job\", max_len = 64 }]";
    let gen = generated(&dir, "sub", "sku_a", &[("contract.toml", cmd, per_job)]);
    assert_eq!(
        table(&gen, "dtp_vars"),
        vec!["{ .name = \"job\", .max_len = 64u }"]
    );
    let t = row(&table(&gen, "dtp_topics"), "\"command_topic\"");
    assert!(
        t.contains(".direction = DTP_SUB") && t.contains(".vars_count = 1u"),
        "{t}"
    );
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_declared_placeholder_is_emitted_for_the_consumer_to_configure() {
    let dir = unique_dir("dtp_placeholders");
    let topic = "[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{duid}/telemetry\"";
    let declared = "[[placeholders]]\nname = \"sku\"\nmax_len = 32\n\n[[placeholders]]\nname = \"env\"\nmax_len = 8\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"rules/x_{env}/{sku}/{duid}/telemetry\"";
    let gen = generated(
        &dir,
        "declared",
        "sku_a",
        &[("contract.toml", topic, declared)],
    );
    assert_eq!(
        table(&gen, "dtp_placeholders"),
        vec![
            "{ .name = \"sku\", .max_len = 32u }",
            "{ .name = \"env\", .max_len = 8u }"
        ]
    );
    let json = &contract_json(&gen)["shadow"]["placeholders"];
    assert_eq!(json[1]["name"], "env");
    walk_dsh_tables(&gen);
    let plain = generated(&dir, "plain", "sku_a", &[]);
    let h = fs::read_to_string(plain.join("dsh_tables.h")).unwrap();
    assert!(h.contains("#define DTP_NUM_PLACEHOLDERS 0u"), "{h}");
    assert!(contract_json(&plain)["shadow"]
        .get("placeholders")
        .is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_without_vars_has_none_and_names_its_interface() {
    let dir = unique_dir("dtp_no_vars");
    let plain = generated(&dir, "plain", "sku_a", &[]);
    let h = fs::read_to_string(plain.join("dsh_tables.h")).unwrap();
    let types = fs::read_to_string(plain.join("dsh_types.h")).unwrap();
    let minor = types.split("#define DSH_IF_MINOR ").nth(1).unwrap();
    let minor = minor.split_whitespace().next().unwrap();
    assert!(h.contains("#define DTP_NUM_VARS 0u"), "{h}");
    assert!(
        h.contains(&format!("shadow table\n * interface 1.{minor}.")),
        "{h}"
    );
    assert!(contract_json(&plain)["shadow"]["topics"][0]
        .get("vars")
        .is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn topics_carry_envelopes_acks_and_the_contract_request() {
    let dir = unique_dir("dsh_topics");
    let gen = generated(&dir, "plain", "sku_a", &[]);
    let topics = table(&gen, "dtp_topics");
    let cmd = row(&topics, "\"command_topic\"");
    for part in [
        ".pattern = \"{stage}/{duid}/cmd/req\"",
        ".id_field = \"id\"",
        ".id_absent = NULL",
        ".source_field = \"source\"",
        ".source_absent = 0,",
        ".source_enum = 0u",
        ".subscribe_order = 2u",
        ".direction = DTP_SUB",
        ".ack = 3u",
        ".flags = DTP_F_ENVELOPE | DTP_F_ID_REQUIRED | DTP_F_SETTINGS_IN_EVERY_BODY",
        ".local_wrapper = \"command_topic\"",
    ] {
        assert!(cmd.contains(part), "{part}: {cmd}");
    }
    let contract = row(&topics, "\"contract_req\"");
    assert!(
        contract.contains(".subscribe_order = 4u")
            && contract.contains(".ack = 9u")
            && contract.contains(".source_enum = DSH_NONE_U16"),
        "{contract}"
    );
    assert!(row(&topics, "\"contract_resp\"")
        .contains(".direction = DTP_PUB, .ack = DSH_NONE_U8, .flags = 0u"));
    assert!(row(&topics, "\"telemetry\"").contains(".connection = \"telemetry\""));
    let optional = "envelope = { id = { field = \"id\", absent = 0 }, source = { enum = \"core::origin\", absent = \"cloud\" } }";
    let required = "envelope = { id = { field = \"id\", required = true }, source = { enum = \"core::origin\", absent = \"unspecified\" } }";
    let edited = generated(
        &dir,
        "optional",
        "sku_a",
        &[("contract.toml", required, optional)],
    );
    let map = row(&table(&edited, "dtp_topics"), "\"mapjob_topic\"");
    assert!(
        map.contains(".id_absent = \"0\"")
            && map.contains(".source_absent = 3,")
            && map.contains(".flags = DTP_F_ENVELOPE,"),
        "{map}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_kept_literal_shadow_name_is_flagged() {
    let dir = unique_dir("dsh_literal_name");
    let delta = "template = \"$aws/things/{duid}/shadow/name/{shadow_name}/update/delta\"";
    let literal = "template = \"$aws/things/{duid}/shadow/name/device/update/delta\"\nliteral_shadow_name = true";
    let gen = generated(
        &dir,
        "literal",
        "sku_a",
        &[("contract.toml", delta, literal)],
    );
    let topic = row(&table(&gen, "dtp_topics"), "\"shadow_delta\"");
    assert!(
        topic.contains(".flags = DTP_F_LITERAL_SHADOW_NAME,"),
        "{topic}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_composer_buffer_is_warned_about_only_when_exceeded() {
    let dir = unique_dir("dsh_buffer");
    let contract = model("v1_1").join("contract.toml");
    let full: usize = define(
        &generated(&dir, "plain", "sku_a", &[]),
        "DSH_FULL_MAX_BYTES",
    )
    .trim_end_matches('u')
    .parse()
    .unwrap();
    for (buffer, warned) in [(full - 1, true), (full, false)] {
        let out = dir.join(format!("b{buffer}"));
        let (ok, stderr) = generate(&contract, &out, &["--shadow-buffer", &buffer.to_string()]);
        assert!(ok, "{stderr}");
        let text = format!("up to {full} bytes, over the {buffer}-byte composer buffer");
        assert_eq!(stderr.contains(&text), warned, "{buffer}: {stderr}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_without_a_mapping_has_no_shadow_tables_but_the_fixed_header() {
    let dir = unique_dir("dsh_none");
    let gen = dir.join("gen");
    let contract =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract/no_unix/contract.toml");
    let (ok, stderr) = generate(&contract, &gen, &[]);
    assert!(ok, "{stderr}");
    assert!(!gen.join("dsh_tables.c").exists() && !gen.join("dsh_tables.h").exists());
    let types = dir.join("types");
    let (ok, stderr) = run_ingot(&["--table-types", "--output", types.to_str().unwrap()]);
    assert!(ok, "{stderr}");
    let fixed = fs::read_to_string(types.join("dsh_types.h")).unwrap();
    assert_eq!(fixed, fs::read_to_string(gen.join("dsh_types.h")).unwrap());
    assert!(fixed.contains("#define DSH_IF_MAJOR 1") && fixed.contains("#define DSH_IF_MINOR 10"));
    assert!(fixed.contains("DSH_FORM_BOOL_INTEGER = 4"));
    assert!(fixed.contains("DSH_RK_REJECTED = 4"));
    assert!(
        fixed.contains("#define DTP_VARS_MAX 8u") && fixed.contains("#define DTP_VAR_LEN_MAX 128u")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn error_bits_are_sorted_by_bit_not_by_key() {
    let dir = unique_dir("dsh_bits");
    let second = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"error_stall\"\nkey_index = 14\ntype = \"bool\"\nintroduced_in = \"1.1\"\nerror = { group = \"product\", bit = 2 }";
    let edits = [
        ("ns/drive.toml", "telem = \"dock\"", second),
        (
            "ns/drive.toml",
            "error = { group = \"product\", bit = 1 }",
            "error = { group = \"product\", bit = 3 }",
        ),
    ];
    let gen = generated(&dir, "bits", "sku_a", &edits);
    let bits = table(&gen, "dsh_error_bits");
    assert!(
        bits[0].ends_with(".bit = 2u }") && bits[1].ends_with(".bit = 3u }"),
        "{bits:?}"
    );
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_verb_without_its_argument_answers_success_and_does_nothing() {
    let dir = unique_dir("dsh_required_success");
    let rows = format!("{GO_ROW}\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = {{ field = \"state\", wire = \"getlog\" }}\nargs = [ {{ field = \"url\", key = \"drive.state.label\", required = true, ec = 0 }} ]");
    let gen = generated(&dir, "getlog", "sku_a", &[("ns/drive.toml", GO_ROW, &rows)]);
    let getlog = steps(&gen, &row(&table(&gen, "dsh_commands"), "\"getlog\""));
    assert!(
        getlog[0].0.starts_with("{ .command = 0x00000000U,"),
        "STATE writes only"
    );
    let url = &getlog[0].1[0];
    assert!(url.contains(".flags = DSH_AF_REQUIRED, .ec = 0u"), "{url}");
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The 1.1 model whose factory reset row also enables the trace and edits the map,
/// generated for sku_a into `dir`.
/// @version 1
fn also_model(dir: &Path) -> PathBuf {
    let internal =
        "internal = \"a signed envelope a diagnostic tool delivers, not a shadow request\"";
    let reset = "command = \"core.command.factory_reset\"";
    let also = "command = \"core.command.factory_reset\"\nalso = [ { command = \"core.command.trace_enable\" }, { command = \"drive.command.map_edit\", args = [ { key = \"drive.command.map_id\", value = 7 } ] } ]";
    let edits = [
        ("ns/core.toml", internal, ""),
        ("ns/core.toml", reset, also),
    ];
    generated(dir, "also", "sku_a", &edits)
}

#[test]
fn a_row_submits_further_commands_as_steps() {
    let dir = unique_dir("dsh_also");
    let gen = also_model(&dir);
    let steps = steps(
        &gen,
        &row(&table(&gen, "dsh_commands"), "\"net.factoryReset\""),
    );
    let id = |k: &str| format!("{:#010X}U", key_json(&gen, k)["id"].as_u64().unwrap());
    let commands: Vec<&str> = steps
        .iter()
        .map(|(s, _)| s.split(", ").next().unwrap())
        .collect();
    let expected = [
        "core.command.factory_reset",
        "core.command.trace_enable",
        "drive.command.map_edit",
    ];
    assert_eq!(
        commands,
        expected.map(|k| format!("{{ .command = {}", id(k)))
    );
    assert!(steps[0].1.is_empty() && steps[1].1.is_empty());
    assert_eq!(value(&gen, &steps[2].1[0]), vec![7, 0]);
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_further_command_lists_the_row_that_submits_it() {
    let dir = unique_dir("dsh_also_keyspace");
    let gen = also_model(&dir);
    let text = fs::read_to_string(gen.join("keyspace.sku_a.json")).unwrap();
    let keyspace: serde_json::Value = serde_json::from_str(&text).unwrap();
    let commands = keyspace["commands"].as_array().unwrap();
    let trace = commands
        .iter()
        .find(|c| c["name"] == "core.command.trace_enable")
        .unwrap();
    assert_eq!(trace["wire"][0]["selector"]["field"], "net.factoryReset");
    assert_eq!(trace["wire"][0]["also"][1]["args"][0]["value"], 7);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn base64_text_decodes_into_a_struct_argument_and_steps_name_their_command_type() {
    let dir = unique_dir("dsh_struct_arg");
    let doc = "doc = \"Base64 text of a map request, passed through as text.\"";
    let blob = "doc = \"Base64 text of a map request, passed through as text.\"\n\n[[classes.keys]]\nid = \"plan_data\"\nkey_index = 10\ntype = \"struct\"\nmax_size = 98304\nintroduced_in = \"1.1\"";
    let arg = "{ field = \"data\", key = \"drive.command.map_payload\", omit_when_absent = true, encoding = \"base64\" },";
    let to = "{ field = \"data\", key = \"drive.command.plan_data\", omit_when_absent = true, encoding = \"base64\" },";
    let gen = generated(
        &dir,
        "blob",
        "sku_a",
        &[("ns/drive.toml", doc, blob), ("ns/drive.toml", arg, to)],
    );
    let data = row(&table(&gen, "dsh_args"), "\"data\"");
    assert!(
        data.contains(".type = 10u") && data.contains(".encoding = DSH_ENC_BASE64"),
        "{data}"
    );
    assert!(data.contains(".max_size = 98304u"), "{data}");
    let save = row(
        &table(&gen, "dsh_commands"),
        "\"saveMap\", .kind = DSH_SEL_ELEMENT",
    );
    let step = &steps(&gen, &save)[0].0;
    assert!(
        step.ends_with(".command_type = 10u }"),
        "a struct command: {step}"
    );
    let go = row(&table(&gen, "dsh_commands"), "\"go\"");
    assert!(
        steps(&gen, &go)[0].0.ends_with(".command_type = 9u }"),
        "a binary command"
    );
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_config_key_is_reported_and_never_desired() {
    let dir = unique_dir("dsh_config_path");
    let serial = "id = \"serial\"";
    let mapped = "id = \"serial\"\nshadow = { path = \"config.serial\", scope = \"full\" }";
    let gen = generated(&dir, "serial", "sku_a", &[("ns/mfg.toml", serial, mapped)]);
    let reported = row(&table(&gen, "dsh_reported"), "\"config.serial\"");
    assert!(
        reported.contains(".kind = DSH_RK_KEY") && reported.contains(".scope = DSH_SC_FULL,"),
        "{reported}"
    );
    assert!(
        !table(&gen, "dsh_desired")
            .iter()
            .any(|r| r.contains("config.serial")),
        "never desired"
    );
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

/// A config topic whose payload is a constant, a CONFIG key, a nested SETTINGS enum key and an
/// integer form, appended to the 1.1 model's topics.
const CONFIG_TOPIC: &str = "\n[[topics]]\nid = \"config\"\ntemplate = \"{stage}/{duid}/config\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"config\"\npath = \"sv\"\nvalue = \"1.0\"\n\n[[topic_payload]]\ntopic = \"config\"\npath = \"sku\"\nkey = \"mfg.identity.serial\"\n\n[[topic_payload]]\ntopic = \"config\"\npath = \"sw.fan\"\nkey = \"drive.settings.fan\"\noutbound_unknown = \"normal\"\n\n[[topic_payload]]\ntopic = \"config\"\npath = \"sw.fanCode\"\nkey = \"drive.settings.fan\"\nas = \"integer\"\n";

#[test]
fn a_topic_payload_is_built_from_keys_and_constants() {
    let dir = unique_dir("dsh_payload");
    let end = "connection = \"telemetry\"";
    let edits = [("contract.toml", end, &*format!("{end}\n{CONFIG_TOPIC}"))];
    let gen = generated(&dir, "config", "sku_a", &edits);
    let topics = table(&gen, "dtp_topics");
    let config = row(&topics, "\"config\"");
    assert!(
        config.contains(".payload_first = 0u, .payload_count = 4u"),
        "{config}"
    );
    assert!(row(&topics, "\"command_topic\"").contains(".payload_count = 0u"));
    let payload = table(&gen, "dtp_payload");
    let at = |p: &str| row(&payload, &format!(".path = \"{p}\""));
    assert!(
        at("sv").contains(".constant = \"\\\"1.0\\\"\"")
            && at("sv").contains(".kind = DSH_RK_CONSTANT")
    );
    assert!(at("sku").contains(".kind = DSH_RK_KEY") && at("sku").contains(".type = 8u"));
    let fan = at("sw.fan");
    assert!(
        fan.contains(".form = DSH_FORM_ENUM")
            && fan.contains(".outbound_unknown_wire = \"normal\""),
        "{fan}"
    );
    assert!(at("sw.fanCode").contains(".form = DSH_FORM_INTEGER"));
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Remove the text of `file` from `start` up to (not including) the next `end`, or to the
/// end of the file; the removed text must occur once.
/// @version 1
fn cut(file: &Path, start: &str, end: Option<&str>) {
    let text = fs::read_to_string(file).unwrap();
    let from = text.find(start).unwrap_or_else(|| panic!("{start}"));
    let len = end.map_or(text.len() - from, |e| {
        text[from..].find(e).unwrap_or_else(|| panic!("{e}"))
    });
    edit(file, &text[from..from + len], "");
}

/// @brief A copy of the 1.1 model under `case` without desired, echo, coupled-write or command
/// rows: no echo paths, no settings key at a desired path, no shadow commands.
/// @version 1
fn bare_binder_copy(case: &Path) {
    edited_copy("v1_1", case, &[]);
    let drive = case.join("ns/drive.toml");
    let echo = ("[[shadow_echo]]", Some("[[shadow_constant]]"));
    cut(&case.join("contract.toml"), echo.0, echo.1);
    cut(
        &drive,
        "shadow = \"settings.speedLimit\"\napply_group",
        Some("\n\n"),
    );
    cut(&drive, "shadow = \"settings.fanSpeed\"", Some("\n\n"));
    cut(&drive, "# -- shadow wire mapping: how requests", None);
    cut(&case.join("ns/core.toml"), "[[shadow_commands]]", None);
}

#[test]
fn a_variant_without_binder_or_command_rows_builds_strictly_and_bounds_no_desired_text() {
    let dir = unique_dir("dsh_bare");
    let case = dir.join("bare");
    bare_binder_copy(&case);
    let gen = case.join("sku_a");
    let (ok, stderr) = generate(&case.join("contract.toml"), &gen, &[]);
    assert!(ok, "{stderr}");
    for count in [
        "DSH_NUM_DESIRED",
        "DSH_NUM_ECHO",
        "DSH_NUM_IMPLIES",
        "DSH_NUM_COMMANDS",
        "DSH_DESIRED_PATH_MAX_BYTES",
        "DSH_DESIRED_VALUE_MAX_BYTES",
    ] {
        assert_eq!(define(&gen, count), "0u", "{count}");
    }
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}
