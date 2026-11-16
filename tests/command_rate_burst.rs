//! Command write-rate burst: `rate_burst` on a command type sets the token-bucket burst of its
//! command, result and dedupe keys; a burst without a write rate is refused on a command and on
//! a key, and so is a declared burst of 0 (it would switch the rate limit off), and so is a
//! `[bounds] default_rate_burst` of 0; a minor may
//! change a released command's burst (a rate-table change only).

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// The generic 1.1 model's map_edit burst line, the text the cases replace.
const MAP_EDIT_BURST: &str = "\nrate_burst = 8 ";

/// The three keys of the generic model's map_edit command.
const MAP_EDIT_KEYS: [&str; 3] = [
    "drive.command.map_edit",
    "drive.command.map_edit_result",
    "drive.command.map_edit_dedupe",
];

/// @brief The (rate_per_day, rate_burst) contract.json gives the key at `path` in `gen`.
/// @version 1
fn rate(gen: &Path, path: &str) -> (u64, u64) {
    let k = key_json(gen, path);
    (
        k["rate_per_day"].as_u64().unwrap(),
        k["rate_burst"].as_u64().unwrap(),
    )
}

/// @brief Generate the 1.1 model with `edits` into `dir/gen` for `variant`; (success, stderr).
/// @version 1
fn generate_edited(dir: &Path, edits: &[(&str, &str, &str)], variant: &str) -> (bool, String) {
    edited_copy("v1_1", &dir.join("model"), edits);
    let contract = dir.join("model/contract.toml");
    generate(&contract, &dir.join("gen"), &["--variant", variant])
}

#[test]
fn a_command_s_burst_is_on_its_three_keys_and_others_keep_the_default() {
    let dir = unique_dir("burst_parts");
    generate_ok("v1_1", &dir, &[]);
    for path in MAP_EDIT_KEYS {
        assert_eq!(rate(&dir, path), (1440, 8), "{path}");
    }
    assert_eq!(rate(&dir, "core.command.factory_reset"), (1, 4));
    assert_eq!(rate(&dir, "core.command.factory_reset_dedupe"), (1, 4));
    assert_eq!(rate(&dir, "drive.command.drive_result"), (0, 0));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_default_burst_declared_has_the_tables_of_none_declared() {
    let dir = unique_dir("burst_default");
    let default = [("ns/drive.toml", MAP_EDIT_BURST, "\nrate_burst = 4 ")];
    let none = [("ns/drive.toml", MAP_EDIT_BURST, "\n# no burst ")];
    let (ok, stderr) = generate_edited(&dir.join("default"), &default, "sku_a");
    assert!(ok, "{stderr}");
    let (ok, stderr) = generate_edited(&dir.join("none"), &none, "sku_a");
    assert!(ok, "{stderr}");
    assert_same_store_tables(&dir.join("default/gen"), &dir.join("none/gen"));
    assert_eq!(rate(&dir.join("none/gen"), MAP_EDIT_KEYS[2]), (1440, 4));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_burst_without_a_write_rate_is_refused_once_per_declaration() {
    let dir = unique_dir("burst_no_rate");
    let cases = [
        (
            "drive",
            "outcome = \"drive_outcome\"\n",
            "drive.command.drive:",
        ),
        ("docked", "id = \"docked\"\n", "drive.state.docked:"),
    ];
    for (tag, from, path) in cases {
        let to = format!("{from}rate_burst = 3\n");
        let edits = [("ns/drive.toml", from, to.as_str())];
        let (ok, stderr) = generate_edited(&dir.join(tag), &edits, "sku_a");
        let refusal = format!("variant sku_a: {path} rate_burst needs a write_rate");
        assert!(!ok && stderr.contains(&refusal), "{tag}: {stderr}");
        let lines = stderr.lines().filter(|l| l.contains(&refusal)).count();
        assert_eq!(lines, 1, "{tag}: once per variant, not per key:\n{stderr}");
        assert!(!dir.join(tag).join("gen/dmt_tables.c").exists(), "{tag}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_burst_without_a_rate_on_another_variant_refuses_this_one() {
    let dir = unique_dir("burst_variant");
    let from = "id = \"error_dock_jam\"\n";
    let to = format!("{from}rate_burst = 3\n");
    let (ok, stderr) = generate_edited(&dir, &[("ns/drive.toml", from, &to)], "sku_a");
    assert!(!ok, "a sku_b refusal refuses the sku_a run");
    assert!(
        stderr.contains("variant sku_b: drive.state.error_dock_jam: rate_burst needs a write_rate"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The refusal a declared burst of 0 gives, after the declaration's path.
const ZERO_BURST: &str = "rate_burst must be at least 1 (0 switches the write-rate limit off; \
                          omit write_rate for no limit)";

#[test]
fn a_zero_burst_is_refused_once_per_declaration() {
    let dir = unique_dir("burst_zero");
    let velocity = "rate_burst = 20\n\n[[classes.keys]]\nid = \"odometer\"";
    let cases = [
        (
            "command",
            MAP_EDIT_BURST,
            "\nrate_burst = 0 ".to_string(),
            "drive.command.map_edit:",
        ),
        (
            "key",
            velocity,
            velocity.replace("20", "0"),
            "drive.state.velocity:",
        ),
    ];
    for (tag, from, to, path) in cases {
        let edits = [("ns/drive.toml", from, to.as_str())];
        let (ok, stderr) = generate_edited(&dir.join(tag), &edits, "sku_a");
        let refusal = format!("variant sku_a: {path} {ZERO_BURST}");
        assert!(!ok && stderr.contains(&refusal), "{tag}: {stderr}");
        let lines = stderr.lines().filter(|l| l.contains(&refusal)).count();
        assert_eq!(lines, 1, "{tag}: once per variant, not per key:\n{stderr}");
        assert!(!dir.join(tag).join("gen/dmt_tables.c").exists(), "{tag}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_zero_burst_on_another_variant_refuses_this_one() {
    let dir = unique_dir("burst_zero_variant");
    let from = "id = \"error_dock_jam\"\n";
    let to = format!("{from}write_rate = \"1/h\"\nrate_burst = 0\n");
    let (ok, stderr) = generate_edited(&dir, &[("ns/drive.toml", from, &to)], "sku_a");
    assert!(!ok, "a sku_b refusal refuses the sku_a run");
    let refusal = format!("variant sku_b: drive.state.error_dock_jam: {ZERO_BURST}");
    assert!(stderr.contains(&refusal), "{stderr}");
    assert!(!dir.join("gen/dmt_tables.c").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_burst_of_one_is_the_least_accepted() {
    let dir = unique_dir("burst_one");
    let edits = [("ns/drive.toml", MAP_EDIT_BURST, "\nrate_burst = 1 ")];
    let (ok, stderr) = generate_edited(&dir, &edits, "sku_a");
    assert!(ok, "{stderr}");
    for path in MAP_EDIT_KEYS {
        assert_eq!(rate(&dir.join("gen"), path), (1440, 1), "{path}");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The generic model's default burst line, the text the default-burst cases replace.
const DEFAULT_BURST: &str = "default_rate_burst = 4";

#[test]
fn a_default_burst_of_0_is_refused_once() {
    let dir = unique_dir("burst_default_zero");
    let edits = [("contract.toml", DEFAULT_BURST, "default_rate_burst = 0")];
    let (ok, stderr) = generate_edited(&dir, &edits, "sku_a");
    let refusal = "bounds.default_rate_burst must be at least 1 (0 switches the write-rate \
                   limit off on every write_rate without its own rate_burst)";
    assert!(!ok, "{stderr}");
    let lines: Vec<&str> = stderr.lines().filter(|l| l.contains(refusal)).collect();
    assert_eq!(lines.len(), 1, "once per run:\n{stderr}");
    assert!(
        !lines[0].contains("variant "),
        "a model-level refusal: {stderr}"
    );
    assert!(!dir.join("gen/dmt_tables.c").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_default_burst_of_1_is_given_to_every_rate_without_its_own() {
    let dir = unique_dir("burst_default_one");
    let edits = [("contract.toml", DEFAULT_BURST, "default_rate_burst = 1")];
    let (ok, stderr) = generate_edited(&dir, &edits, "sku_a");
    assert!(ok, "{stderr}");
    let gen = dir.join("gen");
    assert_eq!(rate(&gen, "core.command.factory_reset"), (1, 1));
    assert_eq!(rate(&gen, "core.command.factory_reset_dedupe"), (1, 1));
    assert_eq!(
        rate(&gen, MAP_EDIT_KEYS[0]),
        (1440, 8),
        "its own burst wins"
    );
    assert_eq!(rate(&gen, "drive.command.drive_result"), (0, 0));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_minor_may_change_a_released_command_s_rate_and_burst() {
    let dir = unique_dir("burst_minor");
    let released = dir.join("released");
    generate_ok("v1_1", &released, &[]);
    let changed = [(
        "ns/drive.toml",
        "write_rate = \"1/min\"\nrate_burst = 8 ",
        "write_rate = \"2/min\"\nrate_burst = 2 ",
    )];
    edited_copy("v1_1", &dir.join("model"), &changed);
    let previous = released.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    let gen = dir.join("gen");
    let (ok, stderr) = generate(&dir.join("model/contract.toml"), &gen, &args);
    assert!(ok, "a rate change is not frozen: {stderr}");
    assert_eq!(rate(&gen, MAP_EDIT_KEYS[0]), (2880, 2));
    for name in ["integrator", "app", "app.map", "inproc"] {
        assert_eq!(digests(&released, name), digests(&gen, name), "{name}");
    }
    let review = fs::read_to_string(gen.join("release-review.txt")).unwrap();
    assert_eq!(review.trim(), "no sensitive grants to new principals");
    let _ = fs::remove_dir_all(&dir);
}
