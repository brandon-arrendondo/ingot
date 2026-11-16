//! Integration tests for contract mode (`--contract`): the generic test model at 1.0, 1.1 and
//! 2.0, the generated tables compiled and walked in C, determinism, variants, rights, bounds,
//! digests and every contract.json field. Refusals are in contract_refusals.rs.

mod common;

use common::*;
use std::fs;
use std::path::Path;
use std::process::Command;

/// @brief Compile the generated tables with the C walker and the value checks, run both, and
/// cross-check the C digests, model hash and ingot provenance against contract.json.
/// v3: the ingot version and commit too.
/// v4: each principal's digests in minor order; the checks moved into helpers.
/// @version 4
fn compile_and_walk(gen: &Path) {
    for program in ["check_tables", "check_values"] {
        run_c_check(gen, program);
    }
    let json = contract_json(gen);
    check_provenance(
        &fs::read_to_string(gen.join("dmt_tables.h")).unwrap(),
        &json,
    );
    check_digest_rows(
        &fs::read_to_string(gen.join("dmt_tables.c")).unwrap(),
        &json,
    );
}

/// @brief dmt_tables.h carries contract.json's model hash, ingot version and ingot commit.
/// @version 1
fn check_provenance(h: &str, json: &serde_json::Value) {
    assert!(
        h.contains(&format!("\"{}\"", json["model_hash"].as_str().unwrap())),
        "model hash"
    );
    let commit = json["ingot"]["commit"].as_str().unwrap();
    assert!(!commit.is_empty(), "contract.json names the ingot commit");
    assert!(h.contains(&format!("DMT_INGOT_COMMIT          \"{commit}\"")));
    let version = json["ingot"]["version"].as_str().unwrap();
    assert!(h.contains(&format!("DMT_INGOT_VERSION         \"{version}\"")));
}

/// @brief Each principal's dmt_digest row in dmt_tables.c is its contract.json digests, in
/// minor order.
/// @version 1
fn check_digest_rows(c: &str, json: &serde_json::Value) {
    for p in json["principals"].as_array().unwrap() {
        let rows: Vec<String> = p["digests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| c_bytes(d.as_str().unwrap()))
            .collect();
        assert!(
            c.contains(&format!("{{ {} }}", rows.join(",\n      "))),
            "{}: dmt_digest row is not its digests in minor order",
            p["name"]
        );
    }
}

/// @brief The C initialiser the tables use for the bytes of a lower-case hex string.
/// @version 1
fn c_bytes(hex: &str) -> String {
    let bytes: Vec<String> = (0..hex.len())
        .step_by(2)
        .map(|i| format!("0x{}U", &hex[i..i + 2]))
        .collect();
    format!("{{ {} }}", bytes.join(", "))
}

/// @brief Build `tests/fixtures/dmt/<program>.c` with the generated tables in `gen`, and run it.
/// @version 1
fn run_c_check(gen: &Path, program: &str) {
    let check = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dmt")
        .join(format!("{program}.c"));
    let exe = gen.join(program);
    let include = format!("-I{}", gen.display());
    let tables = gen.join("dmt_tables.c");
    let out = Command::new("cc")
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-pedantic",
            "-Werror",
            &include,
        ])
        .args([check.as_path(), tables.as_path()])
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("spawn cc");
    assert!(
        out.status.success(),
        "cc failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        Command::new(&exe).status().unwrap().success(),
        "{program} failed"
    );
}

#[test]
fn generic_model_versions_and_previous_checks() {
    let dir = unique_dir("versions");
    let (g10, g11, g20) = (dir.join("1.0"), dir.join("1.1"), dir.join("2.0"));
    generate_ok("v1_0", &g10, &[]);
    assert!(!g10.join("release-review.txt").exists());
    let prev10 = g10.join("contract.json");
    generate_ok(
        "v1_1",
        &g11,
        &["--previous-contract", prev10.to_str().unwrap()],
    );
    let review = fs::read_to_string(g11.join("release-review.txt")).unwrap();
    assert_eq!(
        review.trim(),
        "new principal app.ota: r on existing net.credentials.cloud_token"
    );
    for name in ["integrator", "app", "mcu", "inproc"] {
        assert_eq!(
            digests(&g10, name)[0],
            digests(&g11, name)[0],
            "{name}: 1.0 digest must reappear"
        );
        assert_ne!(
            digests(&g11, name)[0],
            digests(&g11, name)[1],
            "{name}: 1.1 adds keys"
        );
    }
    let prev11 = g11.join("contract.json");
    generate_ok(
        "v2_0",
        &g20,
        &["--previous-contract", prev11.to_str().unwrap()],
    );
    for gen in [&g10, &g11, &g20] {
        compile_and_walk(gen);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_major_change_labelled_as_a_minor_is_refused() {
    let dir = unique_dir("minor12");
    let (m12, g11) = (dir.join("m12"), dir.join("g11"));
    generate_ok("v1_1", &g11, &[]);
    copy_dir(&model("v2_0"), &m12);
    edit(
        &m12.join("contract.toml"),
        "major = 2\nminor = 0",
        "major = 1\nminor = 2",
    );
    edit(
        &m12.join("ns/drive.toml"),
        "introduced_in = \"2.0\"",
        "introduced_in = \"1.2\"",
    );
    let prev = g11.join("contract.json");
    let (ok, stderr) = generate(
        &m12.join("contract.toml"),
        &dir.join("out"),
        &["--previous-contract", prev.to_str().unwrap()],
    );
    assert!(!ok);
    assert!(
        stderr.contains("drive.state.label was removed in a minor"),
        "{stderr}"
    );
    assert!(
        !dir.join("out/dmt_tables.c").exists(),
        "a refusal generates nothing"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn output_is_deterministic_and_variant_resolved() {
    let dir = unique_dir("determinism");
    let (a, b, c) = (dir.join("a"), dir.join("b"), dir.join("c"));
    generate_ok("v1_0", &a, &[]);
    generate_ok("v1_0", &b, &[]);
    for file in [
        "dmt_types.h",
        "dmt_tables.h",
        "dmt_tables.c",
        "contract.json",
    ] {
        assert_eq!(
            fs::read(a.join(file)).unwrap(),
            fs::read(b.join(file)).unwrap(),
            "{file}"
        );
    }
    generate_ok("v1_0", &c, &["--variant", "sku_b"]);
    let blob = |d: &Path| {
        let text = fs::read_to_string(d.join("dmt_tables.c")).unwrap();
        text.lines()
            .find(|l| l.starts_with("const uint8_t dmt_default_blob"))
            .unwrap()
            .to_string()
    };
    assert_ne!(blob(&a), blob(&c), "sku_b changes per-variant defaults");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_mode_refuses_a_template_directory() {
    let dir = unique_dir("templates");
    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let (ok, stderr) = generate(
        &model("v1_0").join("contract.toml"),
        &dir,
        &["--templates", templates.to_str().unwrap()],
    );
    assert!(!ok && stderr.contains("built-in templates"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rights_are_resolved() {
    let dir = unique_dir("rights");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    // read on a secret comes from its reader list only; write from grants
    assert_eq!(right(&gen, "app.net", "net.credentials.wifi_psk"), "rw");
    assert_eq!(right(&gen, "integrator", "net.credentials.wifi_psk"), "");
    assert_eq!(
        right(&gen, "integrator", "net.credentials.cloud_token"),
        "r"
    );
    // a key grant replaces the class grant; class grants combine
    assert_eq!(right(&gen, "mcu", "drive.state.speed"), "rw");
    assert_eq!(right(&gen, "mcu", "drive.state.odometer"), "");
    assert_eq!(right(&gen, "integrator", "net.health.app_hb"), "r");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_last_key_grant_wins_and_none_removes() {
    let dir = unique_dir("override");
    let model_dir = dir.join("model");
    copy_dir(&model("v1_0"), &model_dir);
    edit(
        &model_dir.join("contract.toml"),
        "  { key = \"net.health.app_hb\", access = \"rw\" },\n]",
        "  { key = \"net.health.app_hb\", access = \"rw\" },\n  { key = \"drive.state.odometer\", access = \"rw\" },\n  { key = \"drive.state.odometer\", access = \"none\" },\n  { key = \"drive.state.enabled\", access = \"none\" },\n  { key = \"drive.state.enabled\", access = \"rw\" },\n]",
    );
    let gen = dir.join("gen");
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &gen, &[]);
    assert!(ok, "{stderr}");
    assert_eq!(right(&gen, "app", "drive.state.odometer"), "");
    assert_eq!(right(&gen, "app", "drive.state.enabled"), "rw");
    assert_eq!(right(&gen, "app", "drive.state.label"), "r");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn queue_bound_is_computed_from_rates_and_latency() {
    // ceil(200 * 1_216_830 writes/day * 50_000 ms / (100 * 86_400_000)) = 1409
    let dir = unique_dir("bound");
    let model_dir = dir.join("model");
    copy_dir(&model("v1_0"), &model_dir);
    edit(
        &model_dir.join("contract.toml"),
        "latency_budget_ms = 50\ncaps = { inflight_fd = 4",
        "latency_budget_ms = 50000\ncaps = { inflight_fd = 4",
    );
    let out = dir.join("bound");
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &out, &[]);
    assert!(ok, "{stderr}");
    assert_eq!(principal(&out, "integrator")["queue_bound"], 1409);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_carries_every_header_and_key_field() {
    let dir = unique_dir("json");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let json = contract_json(&gen);
    assert_eq!(json["tiv"], "1.0");
    assert_eq!(
        json["contract"],
        serde_json::json!({"name": "generic", "major": 1, "minor": 0, "patch": 0})
    );
    assert_eq!(json["variant"], "sku_a");
    assert_eq!(json["ingot"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["reserved"], serde_json::json!([0, 0x00C2_0480]));
    assert_eq!(
        key_json(&gen, "drive.state.odometer"),
        serde_json::json!({
        "path": "drive.state.odometer", "id": 0x00C2_0098, "type": "uint32", "introduced_in": "1.0",
        "persist": "deferred", "deferred_ms": 5000, "rate_per_day": 86400, "rate_burst": 4,
        "max_size": 4, "secret": false, "survives_reset": false, "mfg": false, "alias_of": null,
        "enum": null, "enum_wire": null, "command": null, "wk": null, "doc": null, "unit": null,
        "range": null, "allowed": null, "unset": null, "scale": 1.0, "error": null})
    );
    let mode = key_json(&gen, "drive.settings.mode");
    assert_eq!(
        mode["enum"],
        serde_json::json!({"eco": 1, "off": 0, "sport": 2})
    );
    assert_eq!(
        key_json(&gen, "drive.state.error_overcurrent")["doc"],
        "Error key: set and cleared by the integrator."
    );
    // the state class's default interval applies to its deferred key only
    assert_eq!(key_json(&gen, "drive.state.enabled")["deferred_ms"], 0);
    let serial = key_json(&gen, "mfg.identity.serial");
    assert!(serial["mfg"] == true && serial["survives_reset"] == true);
    let reset = key_json(&gen, "core.command.factory_reset_dedupe");
    assert_eq!(reset["command"], "core.command.factory_reset");
    assert_eq!(
        key_json(&gen, "core.diag.corruption")["wk"],
        "DIAG_CORRUPTION"
    );
    assert_eq!(key_json(&gen, "net.credentials.wifi_psk")["secret"], true);
    assert_eq!(
        key_json(&gen, "core.command.trace_enable")["doc"],
        "Signed trace-enable envelope; the core verifies it."
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_carries_every_principal_command_and_rights_field() {
    let dir = unique_dir("json_principals");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let json = contract_json(&gen);
    let integrator = principal(&gen, "integrator");
    assert_eq!(
        (integrator["uid"].clone(), integrator["token"].clone()),
        (2001.into(), 0.into())
    );
    assert_eq!(
        (integrator["kind"].clone(), integrator["flags"].clone()),
        ("unix".into(), 1.into())
    );
    assert_eq!(integrator["introduced_in"], "1.0");
    assert_eq!(
        (
            integrator["queue_bound"].clone(),
            integrator["latency_budget_ms"].clone()
        ),
        (16.into(), 50.into())
    );
    assert_eq!(principal(&gen, "mcu")["kind"], "uart");
    assert_eq!(principal(&gen, "inproc")["kind"], "inproc");
    assert_eq!(
        json["commands"][0],
        serde_json::json!({
        "path": "core.command.factory_reset", "persist": "durable", "owner": "core"})
    );
    assert_eq!(json["commands"][2]["persist"], "none");
    assert_eq!(right(&gen, "app", "drive.settings.mode"), "rw");
    assert_eq!(right(&gen, "app.net", "net.credentials.cloud_token"), "rw");
    assert_eq!(right(&gen, "app.cloud", "core.command.trace_enable"), "rw");
    assert_eq!(right(&gen, "app.net", "net.credentials.wifi_psk"), "rw");
    assert_eq!(right(&gen, "mcu", "drive.settings.mode"), "r");
    assert_eq!(
        right(&gen, "app.map", "drive.command.map_edit_result"),
        "rw"
    );
    assert_eq!(right(&gen, "integrator", "core.command.factory_reset"), "r");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn digests_match_the_independent_oracle() {
    // KSET v1 computed independently (canonical bytes built with printf, hashed with coreutils
    // sha256sum) over tool's six core.diag keys as of minor 0, then the same set plus a
    // write-only grant on drive.settings.mode.
    let dir = unique_dir("oracle");
    let (gen, model_dir, wgen) = (dir.join("gen"), dir.join("model"), dir.join("wgen"));
    generate_ok("v1_0", &gen, &[]);
    assert_eq!(
        digests(&gen, "tool"),
        ["d8372d65196f5a652b4f5b832d5df47a874210de6d3f0d11170580c0f1f9f0a6"]
    );
    copy_dir(&model("v1_0"), &model_dir);
    edit(
        &model_dir.join("contract.toml"),
        "  { namespace = \"core\", class = \"diag\", access = \"r\" },\n]\n\n# -- a peer MCU",
        "  { namespace = \"core\", class = \"diag\", access = \"r\" },\n  { key = \"drive.settings.mode\", access = \"w\" },\n]\n\n# -- a peer MCU",
    );
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &wgen, &[]);
    assert!(ok, "{stderr}");
    assert_eq!(
        digests(&wgen, "tool"),
        ["49f5c6c57c3c0f9c73a642e2ba7244ba41074f6658ebaf4e3ac2715e61a9f2c7"]
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief One model-hash input entry: `path 0x00 u64le(len) bytes` of `dir/rel`.
/// @version 1
fn hash_entry(dir: &Path, rel: &str) -> Vec<u8> {
    let bytes = fs::read(dir.join(rel)).unwrap();
    let mut entry = rel.as_bytes().to_vec();
    entry.push(0);
    entry.extend((bytes.len() as u64).to_le_bytes());
    entry.extend(bytes);
    entry
}

/// @brief Independent model hash: SHA-256 over `path 0x00 u64le(len) bytes` for each input in
/// byte order of its path relative to `dir`, then `0x00 variant`.
/// @version 1
fn oracle_model_hash(dir: &Path, rel_paths: &[&str], variant: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted = rel_paths.to_vec();
    sorted.sort_unstable();
    let mut input: Vec<u8> = sorted.iter().flat_map(|rel| hash_entry(dir, rel)).collect();
    input.push(0);
    input.extend(variant.as_bytes());
    Sha256::digest(&input)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn the_model_hash_covers_every_input_in_path_order() {
    // The namespace directory sorts before contract.toml, so path order is not read order; a
    // non-.toml file in it is not an input.
    let dir = unique_dir("model_hash");
    let model_dir = dir.join("model");
    copy_dir(&model("v1_0"), &model_dir);
    fs::rename(model_dir.join("ns"), model_dir.join("a_ns")).unwrap();
    edit(
        &model_dir.join("contract.toml"),
        "namespace_dir = \"ns\"",
        "namespace_dir = \"a_ns\"",
    );
    fs::write(model_dir.join("a_ns/README.md"), "not an input").unwrap();
    let gen = dir.join("gen");
    let (ok, stderr) = generate(
        &model_dir.join("contract.toml"),
        &gen,
        &["--variant", "sku_b"],
    );
    assert!(ok, "{stderr}");
    let inputs = [
        "contract.toml",
        "a_ns/core.toml",
        "a_ns/drive.toml",
        "a_ns/mfg.toml",
        "a_ns/net.toml",
        "units.toml",
    ];
    assert_eq!(
        contract_json(&gen)["model_hash"],
        oracle_model_hash(&model_dir, &inputs, "sku_b")
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The generic 1.0 model's factory-reset command, uid-2001 caps and in-process principal, each
/// moved elsewhere in its file by `declaration_order_does_not_change_the_tables`.
const FACTORY_RESET: &str = "[[classes.commands]]\nid = \"factory_reset\"\ncmd_index = 0\nresult_index = 1\ndedupe_index = 2\nintroduced_in = \"1.0\"\npersistence = \"durable\"\nowner = \"core\"\ndedupe_ring = 4\nwrite_rate = \"1/day\"\nwk = \"RESET_REQUEST\"\n\n";
const CAPS_2001: &str =
    "[[uid_caps]]\nuid = 2001\ninflight_fd = 4\ninflight_bytes = 1048576\nopen_fds = 16\n\n";
const INPROC: &str = "# -- an in-process consumer (compile-time role 10) ---------------------------------------------\n[[principals]]\nname = \"inproc\"\nuid = 10\nkind = \"inproc\"\nlatency_budget_ms = 50\nresync_k = 3\nresync_t_ms = 60000\nstaging_bound_bytes = 8192\nintroduced_in = \"1.0\"\ngrants = [\n  { namespace = \"drive\", class = \"settings\", access = \"r\" },\n  { namespace = \"drive\", class = \"state\", access = \"r\" },\n  { key = \"drive.settings.mode\", access = \"rw\" },   # an in-process role writes a key\n  { key = \"drive.command.drive\", access = \"w\" },    # and submits a command\n]\n\n";

/// @brief Remove `block` from `file` and append it at the end.
/// @version 1
fn move_to_end(file: &Path, block: &str) {
    edit(file, block, "");
    fs::write(file, fs::read_to_string(file).unwrap() + "\n" + block).unwrap();
}

#[test]
fn declaration_order_does_not_change_the_tables() {
    let dir = unique_dir("order");
    let (base, model_dir, gen) = (dir.join("base"), dir.join("model"), dir.join("gen"));
    generate_ok("v1_0", &base, &[]);
    copy_dir(&model("v1_0"), &model_dir);
    let (contract, core) = (
        model_dir.join("contract.toml"),
        model_dir.join("ns/core.toml"),
    );
    move_to_end(&core, FACTORY_RESET);
    move_to_end(&contract, CAPS_2001);
    edit(&contract, INPROC, "");
    edit(
        &contract,
        "# -- integrator",
        &format!("{INPROC}# -- integrator"),
    );
    let (ok, stderr) = generate(&contract, &gen, &[]);
    assert!(ok, "{stderr}");
    assert_eq!(
        fs::read_to_string(base.join("dmt_tables.c")).unwrap(),
        fs::read_to_string(gen.join("dmt_tables.c")).unwrap(),
        "principals, commands and uid caps are sorted, not taken in declaration order"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_an_in_process_principal_the_in_process_budgets_are_zero() {
    let dir = unique_dir("no_inproc");
    let (model_dir, gen) = (dir.join("model"), dir.join("gen"));
    copy_dir(&model("v1_0"), &model_dir);
    edit(&model_dir.join("contract.toml"), INPROC, "");
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &gen, &[]);
    assert!(ok, "{stderr}");
    let h = fs::read_to_string(gen.join("dmt_tables.h")).unwrap();
    assert!(h.contains("#define DMT_INPROC_QUEUE_BYTES    0u"), "{h}");
    assert!(h.contains("#define DMT_INPROC_SNAPSHOT_BYTES 0u"), "{h}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_without_uid_caps_or_defaults_still_compiles() {
    let dir = unique_dir("no_unix");
    let gen = dir.join("gen");
    let contract =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract/no_unix/contract.toml");
    let (ok, stderr) = generate(&contract, &gen, &[]);
    assert!(ok, "{stderr}");
    let h = fs::read_to_string(gen.join("dmt_tables.h")).unwrap();
    assert!(h.contains("#define DMT_NUM_UID_CAPS          0"), "{h}");
    assert!(h.contains("#define DMT_DEFAULT_BLOB_LEN      0"), "{h}");
    run_c_check(&gen, "check_tables");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn flags_and_scope_grants_combine_and_secrets_keep_their_readers() {
    let dir = unique_dir("combine");
    let (model_dir, gen) = (dir.join("model"), dir.join("gen"));
    copy_dir(&model("v1_0"), &model_dir);
    let contract = model_dir.join("contract.toml");
    edit(
        &contract,
        "flags = [\"token_issuer\"]",
        "flags = [\"token_issuer\", \"trace_reader\"]",
    );
    edit(
        &contract,
        "  { namespace = \"core\", class = \"diag\", access = \"r\" },\n]\n\n# -- a peer MCU",
        "  { namespace = \"core\", class = \"diag\", access = \"r\" },\n  { namespace = \"drive\", access = \"r\" },\n  { namespace = \"drive\", class = \"settings\", access = \"w\" },\n  { namespace = \"net\", class = \"credentials\", access = \"r\" },\n]\n\n# -- a peer MCU",
    );
    let (ok, stderr) = generate(&contract, &gen, &[]);
    assert!(ok, "{stderr}");
    assert_eq!(principal(&gen, "app")["flags"], 0x04 | 0x02);
    assert_eq!(right(&gen, "tool", "drive.settings.mode"), "rw");
    assert_eq!(right(&gen, "tool", "drive.state.odometer"), "r");
    assert_eq!(
        right(&gen, "tool", "net.credentials.wifi_psk"),
        "",
        "not a reader"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn write_only_keys_count_in_neither_the_queue_bound_nor_the_snapshot() {
    // in-process principal with read on drive.settings and drive.state except speed (write
    // only): 5 x 1440 + 2 x 86400 writes/day readable (trip_count and run_time included);
    // ceil(200 * 180000 * 50000 / 8.64e9) = 209.
    let dir = unique_dir("write_only");
    let (model_dir, gen) = (dir.join("model"), dir.join("gen"));
    copy_dir(&model("v1_0"), &model_dir);
    let contract = model_dir.join("contract.toml");
    edit(
        &contract,
        "latency_budget_ms = 50\nresync_k = 3\nresync_t_ms = 60000\nstaging_bound_bytes = 8192",
        "latency_budget_ms = 50000\nresync_k = 3\nresync_t_ms = 60000\nstaging_bound_bytes = 8192",
    );
    edit(
        &contract,
        "# and submits a command\n]",
        "# and submits a command\n  { key = \"drive.state.speed\", access = \"w\" },\n]",
    );
    let (ok, stderr) = generate(&contract, &gen, &[]);
    assert!(ok, "{stderr}");
    assert_eq!(principal(&gen, "inproc")["queue_bound"], 209);
    // the snapshot floor drops by speed's 2 bytes, from 68 to 66
    edit(
        &contract,
        "inproc_snapshot_bytes = 4096",
        "inproc_snapshot_bytes = 65",
    );
    let (ok, stderr) = generate(&contract, &dir.join("refused"), &[]);
    assert!(
        !ok && stderr.contains("inproc_snapshot_bytes 65 is below its lower bound 66"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn c_strings_are_escaped() {
    let dir = unique_dir("escape");
    let (model_dir, gen) = (dir.join("model"), dir.join("gen"));
    copy_dir(&model("v1_0"), &model_dir);
    edit(
        &model_dir.join("contract.toml"),
        "name = \"generic\"",
        "name = \"ge\\\"ne\\\\ric\"",
    );
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &gen, &[]);
    assert!(ok, "{stderr}");
    let h = fs::read_to_string(gen.join("dmt_tables.h")).unwrap();
    assert!(h.contains("\"ge\\\"ne\\\\ric\""), "{h}");
    run_c_check(&gen, "check_tables");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn enums_resolve_per_variant_with_wire_strings() {
    let dir = unique_dir("enum_variants");
    let (a, b) = (dir.join("a"), dir.join("b"));
    generate_ok("v1_0", &a, &[]);
    generate_ok("v1_0", &b, &["--variant", "sku_b"]);
    let mode = |gen: &Path| key_json(gen, "drive.settings.mode");
    assert_eq!(
        mode(&a)["enum"],
        serde_json::json!({"off": 0, "eco": 1, "sport": 2})
    );
    assert_eq!(
        mode(&a)["enum_wire"],
        serde_json::json!({"0": "off", "1": "eco", "2": "sportMode"})
    );
    // sku_b renames 1, adds 3 and removes sport
    assert_eq!(
        mode(&b)["enum"],
        serde_json::json!({"off": 0, "saver": 1, "turbo": 3})
    );
    assert_eq!(
        mode(&b)["enum_wire"],
        serde_json::json!({"0": "off", "1": "saver", "3": "turbo"})
    );
    assert_eq!(
        contract_json(&a)["variants"],
        serde_json::json!(["sku_a", "sku_b"])
    );
    let _ = fs::remove_dir_all(&dir);
}

/// sku_b's enum changes in the generic 1.0 model.
const SKU_B_MODE: &str = "[enums.mode.variants.sku_b]              # sku_b renames 1, adds 3, has no sport\nremove = [\"sport\"]\n\n[enums.mode.variants.sku_b.values]\nsaver = 1\nturbo = 3\n";

#[test]
fn store_tables_and_digests_do_not_depend_on_enum_maps() {
    // the same model with and without sku_b's enum changes; only the model hash differs
    let dir = unique_dir("enum_neutral");
    let (with, model_dir, without) = (dir.join("with"), dir.join("model"), dir.join("without"));
    generate_ok("v1_0", &with, &["--variant", "sku_b"]);
    edited_copy("v1_0", &model_dir, &[("ns/drive.toml", SKU_B_MODE, "")]);
    let contract = model_dir.join("contract.toml");
    let (ok, stderr) = generate(&contract, &without, &["--variant", "sku_b"]);
    assert!(ok, "{stderr}");
    assert_same_store_tables(&with, &without);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn version_prints_the_commit_contract_mode_stamps() {
    let dir = unique_dir("version");
    let gen = dir.join("gen");
    generate_ok("v1_0", &gen, &[]);
    let out = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .arg("--version")
        .output()
        .expect("spawn ingot");
    let printed = String::from_utf8_lossy(&out.stdout).into_owned();
    let commit = contract_json(&gen)["ingot"]["commit"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        printed.trim(),
        format!("ingot {} ({commit})", env!("CARGO_PKG_VERSION"))
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contract_json_carries_units_ranges_sets_and_scale() {
    let dir = unique_dir("json_limits");
    let (g10, g20) = (dir.join("g10"), dir.join("g20"));
    generate_ok("v1_0", &g10, &[]);
    generate_ok("v2_0", &g20, &[]);
    let limit = key_json(&g10, "drive.settings.speed_limit");
    assert_eq!(limit["unit"], "mm/s");
    assert_eq!(
        limit["range"],
        serde_json::json!({"min": 0, "max": 2000, "step": 10})
    );
    assert_eq!(contract_json(&g10)["units"]["mm/s"]["quantity"], "speed");
    assert_eq!(
        key_json(&g20, "drive.settings.max_accel")["allowed"],
        serde_json::json!([250, 500, 1000])
    );
    assert_eq!(key_json(&g20, "drive.settings.gain_pct")["scale"], 0.5);
    let _ = fs::remove_dir_all(&dir);
}
