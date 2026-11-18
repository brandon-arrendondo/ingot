//! The per-model constants header `dmt_constants.h`: one `DMT_E_<KEY PATH>_<VALUE>` define per
//! value of every enum key and one `DMT_E_<COMMAND PATH>_<VALUE>` per outcome of every command
//! that names its outcomes, as the variant resolves its enum, compiled as C.

mod common;

use common::*;
use std::fs;
use std::path::Path;
use std::process::Command;

/// @brief The `#define` lines of dmt_constants.h in `gen`, without the guard.
/// @version 1
fn defines(gen: &Path) -> Vec<String> {
    let h = fs::read_to_string(gen.join("dmt_constants.h")).unwrap();
    h.lines()
        .filter(|l| l.starts_with("#define DMT_E_"))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

/// @brief Compile a file that includes dmt_constants.h in `gen` and uses a key constant and a
/// command outcome constant, as strict C11 and as C++.
/// v2: and an outcome constant.
/// @version 2
fn compiles(gen: &Path) {
    let header = gen.join("use_constants.c");
    let body =
        "#include \"dmt_constants.h\"\nint main(void) { return DMT_E_DRIVE_SETTINGS_MODE_OFF \
                + DMT_E_DRIVE_COMMAND_DRIVE_ACCEPTED; }\n";
    fs::write(&header, body).unwrap();
    for (cc, lang) in [("cc", "c"), ("c++", "c++")] {
        let out = Command::new(cc)
            .args([
                "-fsyntax-only",
                "-Wall",
                "-Wextra",
                "-pedantic",
                "-Werror",
                "-x",
                lang,
            ])
            .arg(&header)
            .output()
            .expect("spawn compiler");
        assert!(
            out.status.success(),
            "{cc}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn every_enum_key_value_is_a_named_constant_of_its_variant() {
    let dir = unique_dir("enum_constants");
    let (a, b) = (dir.join("sku_a"), dir.join("sku_b"));
    generate_ok("v1_1", &a, &["--variant", "sku_a"]);
    generate_ok("v1_1", &b, &["--variant", "sku_b"]);
    let on_a = defines(&a);
    for d in [
        "#define DMT_E_DRIVE_SETTINGS_MODE_OFF 0",
        "#define DMT_E_DRIVE_SETTINGS_MODE_ECO 1",
        "#define DMT_E_DRIVE_SETTINGS_MODE_SPORT 2",
        "#define DMT_E_DRIVE_SETTINGS_FAN_BOOST 2",
        "#define DMT_E_DRIVE_STATE_FAN_APPLIED_BOOST 2",
    ] {
        assert!(on_a.contains(&d.to_string()), "{d} in {on_a:#?}");
    }
    let on_b = defines(&b);
    assert!(on_b.contains(&"#define DMT_E_DRIVE_SETTINGS_MODE_SAVER 1".to_string()));
    assert!(on_b.contains(&"#define DMT_E_DRIVE_SETTINGS_MODE_TURBO 3".to_string()));
    assert!(!on_b.iter().any(|d| d.contains("MODE_SPORT")), "{on_b:#?}");
    let mut sorted = on_a.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), on_a.len(), "each constant once");
    compiles(&a);
    compiles(&b);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_key_s_constants_are_in_code_order() {
    let dir = unique_dir("enum_constants_order");
    generate_ok("v1_1", &dir, &["--variant", "sku_a"]);
    let mode: Vec<String> = defines(&dir)
        .into_iter()
        .filter(|d| d.contains("DMT_E_DRIVE_SETTINGS_MODE_"))
        .collect();
    assert_eq!(
        mode,
        vec![
            "#define DMT_E_DRIVE_SETTINGS_MODE_OFF 0",
            "#define DMT_E_DRIVE_SETTINGS_MODE_ECO 1",
            "#define DMT_E_DRIVE_SETTINGS_MODE_SPORT 2"
        ]
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The line of the generic drive command that names its outcome enum.
const OUTCOME: &str = "outcome = \"drive_outcome\"\n";

/// @brief The defines of `gen` whose name starts with `prefix`.
/// @version 1
fn defines_of(gen: &Path, prefix: &str) -> Vec<String> {
    let prefix = format!("#define {prefix}");
    defines(gen)
        .into_iter()
        .filter(|d| d.starts_with(&prefix))
        .collect()
}

#[test]
fn a_command_s_outcomes_are_named_constants_of_its_variant() {
    let dir = unique_dir("outcome_constants");
    let (a, b) = (dir.join("sku_a"), dir.join("sku_b"));
    generate_ok("v1_1", &a, &["--variant", "sku_a"]);
    generate_ok("v1_1", &b, &["--variant", "sku_b"]);
    let base = [
        "#define DMT_E_DRIVE_COMMAND_DRIVE_ACCEPTED 0",
        "#define DMT_E_DRIVE_COMMAND_DRIVE_REFUSED 1",
        "#define DMT_E_DRIVE_COMMAND_DRIVE_BUSY 2",
    ];
    assert_eq!(defines_of(&a, "DMT_E_DRIVE_COMMAND_DRIVE_"), base);
    let mut on_b = base.to_vec();
    on_b.push("#define DMT_E_DRIVE_COMMAND_DRIVE_STALE_OFFER 3");
    assert_eq!(defines_of(&b, "DMT_E_DRIVE_COMMAND_DRIVE_"), on_b);
    assert!(
        defines_of(&a, "DMT_E_DRIVE_COMMAND_MAP_EDIT_").is_empty(),
        "a command without outcomes has no constants"
    );
    compiles(&a);
    compiles(&b);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_command_s_outcomes_sit_at_its_command_key() {
    // map_id (key index 7) takes the outcome enum too: its constants follow the command's (3)
    let dir = unique_dir("outcome_order");
    let model_dir = dir.join("model");
    let map_id = "id = \"map_id\"\nkey_index = 7\n";
    let with_enum = "id = \"map_id\"\nkey_index = 7\nenum = \"drive_outcome\"\n";
    edited_copy("v1_1", &model_dir, &[("ns/drive.toml", map_id, with_enum)]);
    let gen = dir.join("gen");
    let contract = model_dir.join("contract.toml");
    let (ok, stderr) = generate(&contract, &gen, &["--variant", "sku_a"]);
    assert!(ok, "{stderr}");
    let all = defines(&gen);
    let at = |name: &str| all.iter().position(|d| d.contains(name)).unwrap();
    assert!(at("DMT_E_DRIVE_STATE_FAN_APPLIED_BOOST") < at("DMT_E_DRIVE_COMMAND_DRIVE_ACCEPTED"));
    assert_eq!(
        at("DMT_E_DRIVE_COMMAND_DRIVE_BUSY") + 1,
        at("DMT_E_DRIVE_COMMAND_MAP_ID_ACCEPTED"),
        "{all:#?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_carries_a_command_s_outcomes() {
    let dir = unique_dir("outcome_json");
    generate_ok("v1_1", &dir, &["--variant", "sku_b"]);
    let commands = contract_json(&dir)["commands"].as_array().unwrap().clone();
    let of = |path: &str| {
        let c = commands.iter().find(|c| c["path"] == path).unwrap();
        c.get("outcome").cloned()
    };
    assert_eq!(
        of("drive.command.drive"),
        Some(serde_json::json!({"accepted": 0, "refused": 1, "busy": 2, "stale_offer": 3}))
    );
    assert_eq!(
        of("drive.command.map_edit"),
        None,
        "absent without outcomes"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn store_tables_and_digests_do_not_depend_on_outcomes() {
    let dir = unique_dir("outcome_neutral");
    let (with, model_dir, without) = (dir.join("with"), dir.join("model"), dir.join("without"));
    generate_ok("v1_1", &with, &["--variant", "sku_a"]);
    edited_copy("v1_1", &model_dir, &[("ns/drive.toml", OUTCOME, "")]);
    let contract = model_dir.join("contract.toml");
    let (ok, stderr) = generate(&contract, &without, &["--variant", "sku_a"]);
    assert!(ok, "{stderr}");
    assert_same_store_tables(&with, &without);
    assert!(defines_of(&without, "DMT_E_DRIVE_COMMAND_DRIVE_").is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_minor_may_add_outcomes_and_name_new_ones() {
    // against 1.0: drive gains an outcome and map_edit names outcomes for the first time
    let dir = unique_dir("outcome_minor");
    let (prev, model_dir) = (dir.join("prev"), dir.join("model"));
    generate_ok("v1_0", &prev, &[]);
    let map_edit = "dedupe_ring = 8\n";
    edited_copy(
        "v1_1",
        &model_dir,
        &[
            ("ns/drive.toml", "busy = 2\n", "busy = 2\nlow_battery = 4\n"),
            (
                "ns/drive.toml",
                map_edit,
                "dedupe_ring = 8\noutcome = \"drive_outcome\"\n",
            ),
        ],
    );
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &dir.join("gen"), &args);
    assert!(ok, "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}
