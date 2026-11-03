//! Integration tests for `--udm-compat`, the UDM-shaped C compatibility API.
//!
//! Over a two-namespace fixture with enums, assert that ingot emits every
//! compat form: uppercase `DATAMODEL_{GET,SET}_<PATH>` aliases, lowercase
//! `DataModel_{Get,Set}<path>` aliases, bare-key `#define <PATH>
//! DM_KEY_<PATH>` constants, per-key `<PATH>_ENUM_T` typedefs whose values
//! equal the schema's, `DATA_MODEL_KEY_TYPE_*` aliases, and the legacy
//! header-name aliases. With the flag off, none of these appear (byte
//! identity of the default output is covered by `codegen_determinism.rs`).
//!
//! Fixtures live in `tests/fixtures/udm_compat/`:
//!
//! - `alpha.toml`: uint8 enum `mode` with a camelCase value `wetAndDry`
//!   (renders `WETANDDRY`) and a snake_case value `night_mode` (keeps its
//!   underscore), proving the emitter neither strips nor inserts separators.
//!   A `devicecfg` class and `versionnumber` key carry a camelCase display
//!   `name`, which drives the lowercase accessor alias. One persistent key
//!   exercises `DataModel_PrintPersistentKeys`.
//! - `beta.toml`: uint16 enum-typed `level` (value 42), proving the
//!   storage-width sentinel (`UINT16_MAX`) and cross-namespace emission.
//! - `binary.toml`: persistent binary and string keys.
//! - `no_bool.toml`: no bool keys and no named enums.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURES: &str = "tests/fixtures/udm_compat";

/// Create a fresh, uniquely-named temp directory for one test.
fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_udm_compat_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Run the ingot binary, asserting success. CWD is the crate root during
/// `cargo test`, so `templates/` and the fixtures resolve.
fn run_ingot(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .args(args)
        .status()
        .expect("spawn ingot");
    assert!(status.success(), "ingot failed for args {args:?}");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Generate `models` (fixture basenames) into `<dir>/gen` with `extra` flags.
fn generate_models(dir: &Path, models: &[&str], extra: &[&str]) -> PathBuf {
    let out = dir.join("gen");
    let paths: Vec<String> = models
        .iter()
        .map(|m| format!("{FIXTURES}/{m}.toml"))
        .collect();
    let mut args: Vec<&str> = Vec::new();
    for path in &paths {
        args.extend(["--model", path.as_str()]);
    }
    args.extend(["--output", out.to_str().unwrap()]);
    args.extend_from_slice(extra);
    run_ingot(&args);
    out
}

/// Generate the alpha + beta fixture with `extra` flags.
fn generate(dir: &Path, extra: &[&str]) -> PathBuf {
    generate_models(dir, &["alpha", "beta"], extra)
}

/// Syntax-check one translation unit against the generated headers.
fn assert_compiles_file(compiler: &str, include: &Path, file: &Path) {
    let output = Command::new(compiler)
        .args(["-fsyntax-only", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(include)
        .arg(file)
        .output()
        .unwrap_or_else(|e| panic!("spawn {compiler}: {e}"));
    assert!(
        output.status.success(),
        "{compiler} rejected {}:\n{}",
        file.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Write `source` beside the generated headers and syntax-check it.
fn assert_compiles(compiler: &str, lang: &str, out: &Path, source: &str) {
    let src = out.join(format!("compat_check.{lang}"));
    fs::write(&src, source).unwrap();
    assert_compiles_file(compiler, out, &src);
}

fn assert_all(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "missing `{needle}`:\n{text}");
    }
}

#[test]
fn udm_compat_emits_uppercase_accessor_aliases() {
    let dir = unique_dir("upper");
    let out = generate(&dir, &["--udm-compat"]);
    let helpers = read(&out.join("dm_helpers.h"));

    // Plain bool/int keys get an object-like macro so a caller can take the
    // accessor's address; strings keep a call-style macro.
    assert_all(
        &helpers,
        &[
            "#define DATAMODEL_GET_ALPHA_STATUS_ACTIVE DataModel_Get_ALPHA_STATUS_ACTIVE",
            "#define DATAMODEL_SET_ALPHA_STATUS_ACTIVE DataModel_Set_ALPHA_STATUS_ACTIVE",
            "#define DATAMODEL_GET_ALPHA_STATUS_LABEL()",
        ],
    );

    // Enum-typed keys get real wrappers: GET returns the per-key typedef and
    // SET takes the raw storage type, since C++ will not convert implicitly.
    for (path, storage) in [
        ("ALPHA_STATUS_MODE", "uint8_t"),
        ("BETA_SENSOR_LEVEL", "uint16_t"),
    ] {
        assert_all(
            &helpers,
            &[
                &format!("static inline {path}_ENUM_T DATAMODEL_GET_{path}(void)"),
                &format!("static inline DM_RETURN_CODE DATAMODEL_SET_{path}({storage} x)"),
            ],
        );
        assert!(
            !helpers.contains(&format!("#define DATAMODEL_GET_{path} ")),
            "enum GET must not be a bare macro alias for {path}:\n{helpers}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_compat_emits_lowercase_aliases_with_display_names() {
    let dir = unique_dir("lower");
    let out = generate(&dir, &["--udm-compat"]);
    let helpers = read(&out.join("dm_helpers.h"));

    assert_all(
        &helpers,
        &[
            "#define DataModel_Getalpha_status_mode DATAMODEL_GET_ALPHA_STATUS_MODE",
            "#define DataModel_Setalpha_status_mode DATAMODEL_SET_ALPHA_STATUS_MODE",
            "#define DataModel_Getbeta_sensor_level DATAMODEL_GET_BETA_SENSOR_LEVEL",
            // The display names spell the lowercase alias; the uppercase
            // surface still derives from the lowercase id.
            "#define DataModel_Getalpha_deviceCfg_versionNumber \
DATAMODEL_GET_ALPHA_DEVICECFG_VERSIONNUMBER",
        ],
    );
    assert!(
        !helpers.contains("DataModel_Getalpha_devicecfg_versionnumber"),
        "id-derived lowercase alias must not be emitted when a name exists:\n{helpers}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_compat_emits_bare_keys_and_enum_typedefs() {
    let dir = unique_dir("keys_enums");
    let out = generate(&dir, &["--udm-compat"]);

    assert_all(
        &read(&out.join("key_definitions.h")),
        &[
            "#define ALPHA_STATUS_MODE DM_KEY_ALPHA_STATUS_MODE",
            "#define ALPHA_STATUS_LABEL DM_KEY_ALPHA_STATUS_LABEL",
            "#define BETA_SENSOR_LEVEL DM_KEY_BETA_SENSOR_LEVEL",
        ],
    );

    // Per-key enum typedefs sit beside the named ones, with schema values.
    let enums = read(&out.join("dm_enums.h"));
    assert_all(
        &enums,
        &[
            "} ALPHA_MODE_T;",
            "} ALPHA_STATUS_MODE_ENUM_T;",
            "ALPHA_STATUS_MODE_OFF = 0,",
            "ALPHA_STATUS_MODE_ACTIVE = 7,",
            "ALPHA_STATUS_MODE_WETANDDRY = 2,",
            "ALPHA_STATUS_MODE_NIGHT_MODE = 3,",
            "ALPHA_STATUS_MODE_MAX_STORAGE_VALUE = UINT8_MAX",
            "} BETA_SENSOR_LEVEL_ENUM_T;",
            "BETA_SENSOR_LEVEL_HIGH = 42,",
            "BETA_SENSOR_LEVEL_MAX_STORAGE_VALUE = UINT16_MAX",
        ],
    );
    assert!(
        !enums.contains("ALPHA_STATUS_MODE_WET_AND_DRY") && !enums.contains("_NIGHTMODE"),
        "enum members must keep their source separators:\n{enums}"
    );
    assert_eq!(
        enums.matches("#ifndef").count(),
        1,
        "no extra guard without --udm-enums-guard:\n{enums}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_compat_emits_header_aliases_and_persistent_dump() {
    let dir = unique_dir("aliases");
    let out = generate(&dir, &["--udm-compat"]);

    for (alias, target) in [
        ("dm_key_definitions.h", "key_definitions.h"),
        ("dm_boolean_storage.h", "boolean_storage.h"),
        ("dm_integer_storage.h", "integer_storage.h"),
        ("dm_string_storage.h", "string_storage.h"),
        ("dm_persistence_storage.h", "persistence_storage.h"),
    ] {
        assert_all(
            &read(&out.join(alias)),
            &[&format!("#include \"{target}\"")],
        );
    }

    assert_all(
        &read(&out.join("persistence_storage.h")),
        &["void DataModel_PrintPersistentKeys(void);"],
    );
    assert_all(
        &read(&out.join("persistence_storage.c")),
        &["\"DM_KEY_ALPHA_DEVICECFG_VERSIONNUMBER\""],
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_compat_surface_compiles_as_c_and_cpp() {
    let dir = unique_dir("compile");
    let out = generate(&dir, &["--udm-compat"]);

    let source = r#"
#include "dm_helpers.h"
#include "dm_key_definitions.h"

typedef bool (*bool_getter)(void);

int compat_check(void)
{
    bool_getter get = &DATAMODEL_GET_ALPHA_STATUS_ACTIVE;
    ALPHA_STATUS_MODE_ENUM_T mode = DATAMODEL_GET_ALPHA_STATUS_MODE();
    (void)DATAMODEL_SET_ALPHA_STATUS_MODE((uint8_t)ALPHA_STATUS_MODE_ACTIVE);
    (void)DataModel_Setbeta_sensor_level((uint16_t)BETA_SENSOR_LEVEL_HIGH);
    (void)DataModel_Getalpha_deviceCfg_versionNumber();
    return (int)mode + (get() ? 1 : 0) + (int)DATA_MODEL_KEY_TYPE_STRING
        + (ALPHA_STATUS_LABEL != 0U);
}
"#;
    assert_compiles("cc", "c", &out, source);
    assert_compiles("c++", "cpp", &out, source);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_enums_guard_wraps_only_the_compat_typedefs() {
    let dir = unique_dir("guard");
    let out = generate(
        &dir,
        &["--udm-compat", "--udm-enums-guard", "LEGACY_DM_ENUMS_H"],
    );

    let enums = read(&out.join("dm_enums.h"));
    let guard = enums
        .find("#ifndef LEGACY_DM_ENUMS_H")
        .unwrap_or_else(|| panic!("missing legacy guard:\n{enums}"));
    let named = enums.find("} ALPHA_MODE_T;").unwrap();
    let compat = enums.find("} ALPHA_STATUS_MODE_ENUM_T;").unwrap();
    assert!(
        named < guard && guard < compat,
        "guard must cover the compat typedefs only:\n{enums}"
    );
    assert!(enums.contains("#endif /* LEGACY_DM_ENUMS_H */"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_enums_guard_requires_udm_compat() {
    let status = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .args([
            "--model",
            "examples/battery.toml",
            "--udm-enums-guard",
            "X_H",
        ])
        .output()
        .expect("spawn ingot");
    assert!(!status.status.success());
}

#[test]
fn udm_compat_with_tinyfsm_emits_send_dm_key_event() {
    let dir = unique_dir("events");
    let out = generate(&dir, &["--udm-compat", "--emit-tinyfsm"]);

    assert_all(
        &read(&out.join("dm_key_events.h")),
        &["void send_dm_key_event(uint32_t key_id);"],
    );
    assert_all(
        &read(&out.join("dm_key_events.c")),
        &["send_tinyfsm_event_by_key(key_id);"],
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tinyfsm_emits_has_event_and_namespace_predicates() {
    let dir = unique_dir("has_event");
    let out = generate(&dir, &["--emit-tinyfsm"]);

    let hpp = read(&out.join("dm_key_has_event.hpp"));
    assert_all(
        &hpp,
        &[
            "case DM_KEY_ALPHA_STATUS_MODE:",
            "#define DM_KEY_NS_SHIFT 22U",
            "#define DM_KEY_NS_MASK 0x3FFU",
            "static inline uint32_t dm_key_namespace(uint32_t key)",
        ],
    );
    assert!(
        !hpp.contains("case DM_KEY_ALPHA_STATUS_ACTIVE:"),
        "non-event key must not be listed:\n{hpp}"
    );
    // The legacy event alias is compat-only.
    assert!(!out.join("dm_key_events.h").exists());

    let source = r#"
#include "dm_key_has_event.hpp"
int check(void)
{
    return dm_key_has_event(DM_KEY_ALPHA_STATUS_MODE)
        && dm_key_namespace(DM_KEY_ALPHA_STATUS_MODE) == 1U
        && DM_KEY_NS_LOCAL == DM_KEY_NS_LOCAL;
}
"#;
    assert_compiles("cc", "c", &out, source);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_flag_no_udm_compat_surface_is_emitted() {
    let dir = unique_dir("off");
    let out = generate(&dir, &[]);

    let helpers = read(&out.join("dm_helpers.h"));
    for needle in ["DATAMODEL_GET_", "DATAMODEL_SET_", "DataModel_Getalpha_"] {
        assert!(
            !helpers.contains(needle),
            "`{needle}` must not appear without --udm-compat"
        );
    }

    let keys = read(&out.join("key_definitions.h"));
    assert!(!keys.contains("#define ALPHA_STATUS_MODE DM_KEY_ALPHA_STATUS_MODE"));

    let enums = read(&out.join("dm_enums.h"));
    assert!(!enums.contains("_ENUM_T"), "no per-key typedefs:\n{enums}");
    assert!(
        !enums.contains("stdint.h"),
        "default dm_enums.h unchanged:\n{enums}"
    );

    assert!(!read(&out.join("dm_key.h")).contains("DATA_MODEL_KEY_TYPE"));
    assert!(!read(&out.join("persistence_storage.h")).contains("DataModel_PrintPersistentKeys"));

    for alias in [
        "dm_key_definitions.h",
        "dm_boolean_storage.h",
        "dm_integer_storage.h",
        "dm_string_storage.h",
        "dm_persistence_storage.h",
    ] {
        assert!(
            !out.join(alias).exists(),
            "{alias} must not be emitted without --udm-compat"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Storage aliases follow their canonical header: no bool keys, no alias.
#[test]
fn udm_compat_skips_boolean_storage_alias_when_no_bool_keys() {
    let dir = unique_dir("no_bool");
    let out = generate_models(&dir, &["no_bool"], &["--udm-compat"]);

    assert!(!out.join("boolean_storage.h").exists());
    assert!(!out.join("dm_boolean_storage.h").exists());
    // No named enums, so no dm_enums.h, and dm_helpers.h must not include it.
    assert!(!out.join("dm_enums.h").exists());
    assert!(!read(&out.join("dm_helpers.h")).contains("dm_enums.h"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn udm_compat_emits_data_model_key_type_aliases_in_dm_key_h() {
    let dir = unique_dir("keytype_on");
    let out = generate(&dir, &["--udm-compat"]);

    let dm_key = read(&out.join("dm_key.h"));
    for (alias, target) in [
        ("DATA_MODEL_KEY_TYPE_BOOLEAN", "DM_KEY_TYPE_BOOL"),
        ("DATA_MODEL_KEY_TYPE_UINT8", "DM_KEY_TYPE_UINT8"),
        ("DATA_MODEL_KEY_TYPE_UINT16", "DM_KEY_TYPE_UINT16"),
        ("DATA_MODEL_KEY_TYPE_UINT32", "DM_KEY_TYPE_UINT32"),
        ("DATA_MODEL_KEY_TYPE_INT8", "DM_KEY_TYPE_INT8"),
        ("DATA_MODEL_KEY_TYPE_INT16", "DM_KEY_TYPE_INT16"),
        ("DATA_MODEL_KEY_TYPE_INT32", "DM_KEY_TYPE_INT32"),
        ("DATA_MODEL_KEY_TYPE_STRING", "DM_KEY_TYPE_STRING"),
        ("DATA_MODEL_KEY_TYPE_BYTEARRAY", "DM_KEY_TYPE_BINARY"),
    ] {
        let found = dm_key.lines().any(|line| {
            line.starts_with(&format!("#define {alias}"))
                && line.split_whitespace().nth(2) == Some(target)
        });
        assert!(found, "missing alias {alias} -> {target}:\n{dm_key}");
    }

    let _ = fs::remove_dir_all(&dir);
}

/// The persistent-key dump must not read past a buffer: strings print with a
/// bound, and binary keys (which need not be NUL-terminated) print as hex.
#[test]
fn print_persistent_keys_bounds_strings_and_hex_dumps_binary() {
    let dir = unique_dir("print_binary");
    let out = generate_models(&dir, &["binary"], &["--udm-compat"]);

    assert_all(
        &read(&out.join("persistence_storage.c")),
        &[
            "printf(\" %02x\", (unsigned) persistence_data.cfg_blob[i]);",
            "(int) sizeof(persistence_data.cfg_name), persistence_data.cfg_name);",
        ],
    );
    assert_compiles_file("cc", &out, &out.join("persistence_storage.c"));

    let _ = fs::remove_dir_all(&dir);
}
