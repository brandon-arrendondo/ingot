//! Per-principal log streams: the dmt_log_streams table per variant, its independence from the
//! digests and keyspace files, contract.json, the RAM budget, the flash bounds and every
//! refusal (REQ-ING-LOG-001..005).

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// integrator's `[principals.log]` header line in the 1.1 model, the text some cases edit.
const INTEGRATOR_LOG: &str = "[principals.log]\nbacking = \"ring\"";

/// app.cloud's `[principals.log]` header line in the 1.1 model.
const APP_CLOUD_LOG: &str = "[principals.log]\nallowed_backings = [\"ring\"]";

/// One refusal case on the 1.1 model: file, text to replace, replacement, variant, message.
type Case = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

/// Edits of the 1.1 model that each trigger one log refusal.
#[rustfmt::skip]
const CASES: &[Case] = &[
    ("contract.toml", "log_ram_budget_bytes = 12288", "log_ram_budget_bytes = 12287", "sku_a", "log streams: worst-case RAM 12288 (every stream enabled) is above bounds.log_ram_budget_bytes 12287"),
    ("contract.toml", "log_ram_budget_bytes = 12288         # every stream enabled at once: 8192 + 4096\n", "", "sku_a", "log streams need bounds.log_ram_budget_bytes"),
    ("contract.toml", "log_ram_default_budget_bytes = 8192", "log_ram_default_budget_bytes = 8191", "sku_a", "log streams: boot RAM 8192 (streams enabled at boot) is above bounds.log_ram_default_budget_bytes 8191"),
    ("contract.toml", "log_flash_bytes = 65536", "log_flash_bytes = 65535", "sku_a", "log streams: flash 65536 is above bounds.log_flash_bytes 65535"),
    ("contract.toml", "log_flash_bytes_per_day = 262144", "log_flash_bytes_per_day = 262143", "sku_a", "log streams: flash 262144 per day is above bounds.log_flash_bytes_per_day 262143"),
    ("contract.toml", "log_flash_bytes = 65536              # integrator's 4 segments of 16384\n", "", "sku_a", "log streams need bounds.log_flash_bytes"),
    ("contract.toml", "flash_seg_bytes = 16384", "flash_seg_bytes = 0", "sku_a", "log stream integrator: a flash backing needs flash_seg_bytes, flash_seg_count and flash_rate_bytes_per_day above 0"),
    ("contract.toml", "flash_seg_count = 4", "flash_seg_count = 0", "sku_a", "a flash backing needs flash_seg_bytes, flash_seg_count"),
    ("contract.toml", "flash_rate_bytes_per_day = 262144", "flash_rate_bytes_per_day = 0", "sku_a", "a flash backing needs flash_seg_bytes, flash_seg_count"),
    ("contract.toml", APP_CLOUD_LOG, "[principals.log]\nname = \"integrator\"\nallowed_backings = [\"ring\"]", "sku_a", "log stream name integrator is used twice"),
    ("contract.toml", APP_CLOUD_LOG, "[principals.log]\nname = \"App-Cloud\"\nallowed_backings = [\"ring\"]", "sku_a", "log stream name App-Cloud is not a lower-case identifier"),
    ("contract.toml", "default_level_file = [\"error\", \"warning\"]", "default_level_file = [\"error\", \"debug\"]", "sku_a", "log stream integrator: default_level_file is outside default_level_ram"),
    ("contract.toml", "ctl_key = \"core.log.integrator_cfg\"", "ctl_key = \"core.log.nope\"", "sku_a", "log stream integrator: ctl_key core.log.nope is not a key"),
    ("contract.toml", "ctl_key = \"core.log.integrator_cfg\"", "ctl_key = \"core.diag.log_integrator_drops\"", "sku_a", "log stream integrator: no principal may write ctl_key core.diag.log_integrator_drops"),
    ("contract.toml", "drop_counter_key = \"core.diag.log_integrator_drops\"", "drop_counter_key = \"core.diag.keystore_version\"", "sku_a", "log stream integrator: drop_counter_key core.diag.keystore_version is not an unsigned integer key"),
    ("contract.toml", "drop_counter_key = \"core.diag.log_integrator_drops\"", "drop_counter_key = \"core.diag.nope\"", "sku_a", "log stream integrator: drop_counter_key core.diag.nope is not an unsigned integer key"),
    ("contract.toml", INTEGRATOR_LOG, "[principals.log]\nbacking = \"flash_group\"", "sku_a", "log stream integrator: backing flash_group is not an allowed backing on this variant"),
    ("contract.toml", INTEGRATOR_LOG, "[principals.log]\nbacking_by_variant = { sku_b = \"flash_on_trigger\" }\nbacking = \"ring\"", "sku_b", "log stream integrator: backing flash_on_trigger is not an allowed backing on this variant"),
    ("contract.toml", "allowed_backings_by_variant = { sku_b = [\"ring\"] }", "allowed_backings_by_variant = { sku_c = [\"ring\"] }", "sku_a", "log stream integrator: variant sku_c is not declared"),
    ("contract.toml", INTEGRATOR_LOG, "[principals.log]\nbacking_by_variant = { sku_c = \"ring\" }\nbacking = \"ring\"", "sku_a", "log stream integrator: variant sku_c is not declared"),
    ("contract.toml", "ram_cap_bytes = 4096\nmax_line_bytes = 256", "ram_cap_bytes = 4096\nmax_line_bytes = 0", "sku_a", "log stream app_cloud: max_line_bytes 0 must be above 0 and within ram_cap_bytes 4096"),
    ("contract.toml", "ram_cap_bytes = 4096\nmax_line_bytes = 256", "ram_cap_bytes = 4096\nmax_line_bytes = 4097", "sku_a", "log stream app_cloud: max_line_bytes 4097 must be above 0 and within ram_cap_bytes 4096"),
    ("contract.toml", "max_line_bytes = 256\ndefault_level_ram = [\"error\", \"warning\", \"info\"]", "max_line_bytes = 16385\ndefault_level_ram = [\"error\", \"warning\", \"info\"]", "sku_a", "log stream integrator: max_line_bytes 16385 must be within flash_seg_bytes 16384 and flash_rate_burst_bytes 16384"),
    ("contract.toml", "flash_seg_bytes = 16384", "flash_seg_bytes = 255", "sku_a", "log stream integrator: max_line_bytes 256 must be within flash_seg_bytes 255 and flash_rate_burst_bytes 16384"),
    ("contract.toml", "flash_rate_burst_bytes = 16384", "flash_rate_burst_bytes = 255", "sku_a", "log stream integrator: max_line_bytes 256 must be within flash_seg_bytes 16384 and flash_rate_burst_bytes 255"),
    ("contract.toml", "drop_counter_key = \"core.diag.log_app_cloud_drops\"\nintroduced_in = \"1.1\"", "drop_counter_key = \"core.diag.log_app_cloud_drops\"\nintroduced_in = \"1.2\"", "sku_a", "log stream app_cloud: introduced_in is later than the contract"),
    ("contract.toml", "drop_counter_key = \"core.diag.log_app_cloud_drops\"\nintroduced_in = \"1.1\"", "drop_counter_key = \"core.diag.log_app_cloud_drops\"\nintroduced_in = \"1\"", "sku_a", "log stream app_cloud: introduced_in '1' is not a"),
    ("contract.toml", APP_CLOUD_LOG, "[principals.log]\nsize = 1\nallowed_backings = [\"ring\"]", "sku_a", "unknown field `size`"),
    ("contract.toml", APP_CLOUD_LOG, "[principals.log]\nflush = [\"sometimes\"]\nallowed_backings = [\"ring\"]", "sku_a", "unknown variant `sometimes`"),
];

/// @brief Generate the 1.1 model with `edits` applied into `dir`/gen for `variant`; returns
/// (success, stderr).
/// @version 1
fn generate_edited(dir: &Path, edits: &[(&str, &str, &str)], variant: &str) -> (bool, String) {
    let model_dir = dir.join("model");
    edited_copy("v1_1", &model_dir, edits);
    generate(
        &model_dir.join("contract.toml"),
        &dir.join("gen"),
        &["--variant", variant],
    )
}

#[test]
fn every_log_refusal_has_a_triggering_model() {
    let dir = unique_dir("log_refusals");
    for (i, (file, from, to, variant, expected)) in CASES.iter().enumerate() {
        let (ok, stderr) =
            generate_edited(&dir.join(format!("case{i}")), &[(file, from, to)], variant);
        assert!(!ok, "case {i} ({expected}) must be refused");
        assert!(
            stderr.contains(expected),
            "case {i}: expected '{expected}' in:\n{stderr}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The key ID of `path` in a generation's contract.json, as the tables write it.
/// @version 1
fn id_text(gen: &Path, path: &str) -> String {
    format!("{:#010X}U", key_json(gen, path)["id"].as_u64().unwrap())
}

/// @brief integrator's row on a variant whose allowed-backing mask is `allowed`.
/// @version 1
fn integrator_row(gen: &Path, allowed: &str) -> String {
    let (ctl, drop) = (
        id_text(gen, "core.log.integrator_cfg"),
        id_text(gen, "core.diag.log_integrator_drops"),
    );
    format!(
        "{{ 8192u, 256u, 16384u, 4u, 262144u, 16384u, {ctl}, {drop}, 0u, 0x0017u, 1u, 1u, {allowed}u, 0x07u, 0x03u, 0u }}"
    )
}

/// @brief app.cloud's row (principal index 2, off at boot, ring allowed, no flash).
/// @version 1
fn app_cloud_row(gen: &Path) -> String {
    let (ctl, drop) = (
        id_text(gen, "core.log.app_cloud_cfg"),
        id_text(gen, "core.diag.log_app_cloud_drops"),
    );
    format!(
        "{{ 4096u, 256u, 0u, 0u, 0u, 0u, {ctl}, {drop}, 2u, 0x0000u, 1u, 0u, 0x03u, 0x0fu, 0x00u, 0u }}"
    )
}

#[test]
fn log_rows_follow_the_variant() {
    let dir = unique_dir("log_rows");
    for (variant, allowed) in [("sku_a", "0x07"), ("sku_b", "0x03")] {
        let gen = dir.join(variant);
        generate_ok("v1_1", &gen, &["--variant", variant]);
        let c = fs::read_to_string(gen.join("dmt_tables.c")).unwrap();
        let rows = format!(
            "dmt_log_streams[] = {{\n    {},\n    {},\n}};",
            integrator_row(&gen, allowed),
            app_cloud_row(&gen)
        );
        assert!(c.contains(&rows), "{variant}: {rows} in\n{c}");
        assert!(
            c.contains("    \"integrator\",\n    \"app_cloud\",\n};"),
            "{c}"
        );
        let h = fs::read_to_string(gen.join("dmt_tables.h")).unwrap();
        for define in [
            "#define DMT_NUM_LOG_STREAMS       2\n",
            "#define DMT_LOG_RAM_BUDGET_BYTES  12288u\n",
            "#define DMT_LOG_RAM_DEFAULT_BUDGET_BYTES 8192u\n",
            "#define DMT_LOG_FLASH_BYTES       65536u\n",
            "#define DMT_LOG_FLASH_BYTES_PER_DAY 262144u\n",
        ] {
            assert!(h.contains(define), "{variant}: {define} in\n{h}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_without_log_streams_has_an_empty_table() {
    let dir = unique_dir("log_none");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let h = fs::read_to_string(gen.join("dmt_tables.h")).unwrap();
    assert!(h.contains("#define DMT_NUM_LOG_STREAMS       0\n"), "{h}");
    assert!(h.contains("#define DMT_LOG_RAM_BUDGET_BYTES  0u\n"), "{h}");
    assert!(contract_json(&gen).get("log_streams").is_none());
    let _ = fs::remove_dir_all(&dir);
}

/// The 1.1 model's log declarations, each removed by one edit (the switch and counter keys stay).
const WITHOUT_LOG: &[(&str, &str, &str)] = &[
    ("contract.toml", "[principals.log]\nbacking = \"ring\"\nallowed_backings = [\"ring\", \"flash_on_trigger\"]\nallowed_backings_by_variant = { sku_b = [\"ring\"] }\nram_cap_bytes = 8192\nmax_line_bytes = 256\ndefault_level_ram = [\"error\", \"warning\", \"info\"]\ndefault_level_file = [\"error\", \"warning\"]\nflash_seg_bytes = 16384\nflash_seg_count = 4\nflash_rate_bytes_per_day = 262144\nflash_rate_burst_bytes = 16384\nflush = [\"fault\", \"watchdog\", \"shutdown\", \"error_line\"]\nctl_key = \"core.log.integrator_cfg\"\ndrop_counter_key = \"core.diag.log_integrator_drops\"\nintroduced_in = \"1.1\"\n", ""),
    ("contract.toml", "[principals.log]\nallowed_backings = [\"ring\"]\nram_cap_bytes = 4096\nmax_line_bytes = 256\ndefault_level_ram = [\"error\", \"warning\", \"info\", \"debug\"]\nctl_key = \"core.log.app_cloud_cfg\"\ndrop_counter_key = \"core.diag.log_app_cloud_drops\"\nintroduced_in = \"1.1\"\n", ""),
];

/// @brief Assert file `name` is byte-identical in generations `a` and `b`.
/// @version 1
fn same_file(a: &Path, b: &Path, name: &str) {
    let read = |d: &Path| fs::read(d.join(name)).unwrap();
    assert!(read(a) == read(b), "{name} differs");
}

#[test]
fn log_streams_enter_no_digest_and_no_keyspace_file() {
    let dir = unique_dir("log_digest");
    let with = dir.join("with");
    generate_ok("v1_1", &with, &[]);
    let (ok, stderr) = generate_edited(&dir.join("without"), WITHOUT_LOG, "sku_a");
    assert!(ok, "{stderr}");
    let without = dir.join("without/gen");
    for p in contract_json(&with)["principals"].as_array().unwrap() {
        let name = p["name"].as_str().unwrap();
        assert_eq!(digests(&with, name), digests(&without, name), "{name}");
    }
    for name in [
        "keyspace.json",
        "keyspace.sku_a.json",
        "keyspace.sku_a.head.json",
        "dsh_tables.h",
        "dmt_types.h",
    ] {
        same_file(&with, &without, name);
    }
    let h = fs::read_to_string(without.join("dmt_tables.h")).unwrap();
    assert!(h.contains("#define DMT_NUM_LOG_STREAMS       0\n"), "{h}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_lists_the_streams_as_the_variant_resolves_them() {
    let dir = unique_dir("log_json");
    let gen = dir.join("gen");
    generate_ok("v1_1", &gen, &["--variant", "sku_b"]);
    let streams = contract_json(&gen)["log_streams"].clone();
    assert_eq!(
        streams[0],
        serde_json::json!({
            "name": "integrator", "principal": "integrator", "backing": "ring",
            "allowed_backings": ["off", "ring"], "ram_cap_bytes": 8192, "max_line_bytes": 256,
            "default_level_ram": ["error", "warning", "info"],
            "default_level_file": ["error", "warning"],
            "flash_seg_bytes": 16384, "flash_seg_count": 4, "flash_rate_bytes_per_day": 262144,
            "flash_rate_burst_bytes": 16384,
            "flush": ["fault", "watchdog", "shutdown", "error_line"],
            "ctl_key": "core.log.integrator_cfg",
            "drop_counter_key": "core.diag.log_integrator_drops", "introduced_in": "1.1"
        })
    );
    assert_eq!(streams[1]["name"], "app_cloud");
    assert_eq!(streams[1]["principal"], "app.cloud");
    assert_eq!(streams[1]["backing"], "off");
    assert_eq!(streams.as_array().unwrap().len(), 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_contract_json_without_log_streams_is_a_valid_previous_contract() {
    let dir = unique_dir("log_previous");
    let prev = dir.join("prev");
    generate_ok("v1_1", &prev, &[]);
    let path = prev.join("contract.json");
    let mut json = contract_json(&prev);
    json.as_object_mut().unwrap().remove("log_streams");
    fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    let args = ["--previous-contract", path.to_str().unwrap()];
    let (ok, stderr) = generate(
        &model("v1_1").join("contract.toml"),
        &dir.join("out"),
        &args,
    );
    assert!(ok, "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn budgets_equal_to_the_sums_are_accepted_and_off_only_counts_nothing() {
    // sku_b allows integrator no flash, so its flash figures count nowhere: bounds of 0 pass
    let dir = unique_dir("log_edges");
    let edits = [
        (
            "contract.toml",
            "log_flash_bytes = 65536",
            "log_flash_bytes = 0",
        ),
        (
            "contract.toml",
            "log_flash_bytes_per_day = 262144",
            "log_flash_bytes_per_day = 0",
        ),
    ];
    let (ok, stderr) = generate_edited(&dir.join("b"), &edits, "sku_b");
    assert!(ok, "{stderr}");
    // app.cloud allowed only off: its 4096 bytes no longer count, so a budget of 8192 passes
    let off_only = [
        (
            "contract.toml",
            APP_CLOUD_LOG,
            "[principals.log]\nallowed_backings = []",
        ),
        (
            "contract.toml",
            "log_ram_budget_bytes = 12288",
            "log_ram_budget_bytes = 8192",
        ),
    ];
    let (ok, stderr) = generate_edited(&dir.join("off"), &off_only, "sku_a");
    assert!(ok, "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn allowed_backings_default_to_the_backing_and_off() {
    // app.cloud declares no allowed_backings and boots with a ring: ring and off are allowed
    let dir = unique_dir("log_default_allowed");
    let edits = [
        (
            "contract.toml",
            APP_CLOUD_LOG,
            "[principals.log]\nbacking = \"ring\"",
        ),
        (
            "contract.toml",
            "log_ram_default_budget_bytes = 8192",
            "log_ram_default_budget_bytes = 12288",
        ),
    ];
    let (ok, stderr) = generate_edited(&dir, &edits, "sku_a");
    assert!(ok, "{stderr}");
    let stream = contract_json(&dir.join("gen"))["log_streams"][1].clone();
    assert_eq!(stream["backing"], "ring");
    assert_eq!(
        stream["allowed_backings"],
        serde_json::json!(["off", "ring"])
    );
    let _ = fs::remove_dir_all(&dir);
}
