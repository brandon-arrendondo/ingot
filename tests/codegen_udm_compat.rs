//! Integration tests for `--udm-compat` — the UDM-shaped C compatibility API.
//!
//! Assert that, over a multi-namespace fixture schema exercising enums, ingot
//! emits every form libBissellIoT compiles against: uppercase
//! `DATAMODEL_{GET,SET}_<PATH>` macro aliases, lowercase `DataModel_{Get,Set}<path>`
//! symbol aliases, bare-key `#define <PATH> DM_KEY_<PATH>` constants, and a
//! `dm_enums.h` of `<PATH>_ENUM_T` typedefs whose constant values equal the
//! schema's. With the flag off, none of these appear (byte-identity is covered
//! by `codegen_determinism.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Create a fresh, uniquely-named temp directory for one test.
fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_udm_compat_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Run the ingot binary, asserting success. CWD is the crate root during
/// `cargo test`, so `templates/` resolves.
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

/// Write the two-namespace fixture (alpha + beta) and return their paths.
///
/// The enum + id spellings mirror how the UDM key catalog is authored: value
/// and identifier names are written in their *UDM source case*, and the
/// emitter reproduces them verbatim (uppercasing for the C surface). This
/// fixture exercises both spelling classes UDM's `dm_enums.h` contains:
///
/// - `alpha`: uint8 enum `mode` with a camelCase value `wetAndDry` (must render
///   concatenated as `WETANDDRY`, matching UDM's `ROBOT_STATE_WET_DRY_MODE_
///   WETANDDRY`) and a snake value `night_mode` (must retain its underscore as
///   `NIGHT_MODE`, matching UDM's `WIFI_THREADHEALTHCHECK_BITS_NETWORK_MONITOR`
///   class of members) — proving the emitter neither strips nor inserts
///   separators. Plus an `awsjob` class + `versionnumber` key that carry the
///   UDM catalog's id/name split (lowercase/concatenated `id` for encoding,
///   camelCase `name` for display), driving the mixed-case lowercase accessor
///   UDM links against.
/// - `beta`: uint16 enum-keyed `level` (value 42) — proves the storage-width
///   sentinel (`UINT16_MAX`) and cross-namespace emission.
fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let alpha = dir.join("alpha.toml");
    fs::write(
        &alpha,
        r#"
[meta]
id = "alpha"
namespace_id = 1
version = "1.0.0"

[enums.mode]
doc = "operating mode"
[enums.mode.values]
off = 0
wetAndDry = 2
night_mode = 3
active = 7

[[classes]]
id = "status"

    [[classes.keys]]
    id = "mode"
    type = "uint8"
    enum = "mode"
    default = 0
    helpers = true

    [[classes.keys]]
    id = "label"
    type = "string"
    max_size = 16
    default = ""
    helpers = true

    [[classes.keys]]
    id = "active"
    type = "bool"
    default = false
    helpers = true

[[classes]]
id = "awsjob"
name = "awsJob"

    [[classes.keys]]
    id = "versionnumber"
    name = "versionNumber"
    type = "int32"
    default = 0
    helpers = true
"#,
    )
    .unwrap();

    let beta = dir.join("beta.toml");
    fs::write(
        &beta,
        r#"
[meta]
id = "beta"
namespace_id = 2
version = "1.0.0"

[enums.level]
doc = "sensor level"
[enums.level.values]
low = 1
high = 42

[[classes]]
id = "sensor"

    [[classes.keys]]
    id = "level"
    type = "uint16"
    enum = "level"
    default = 1
    helpers = true
"#,
    )
    .unwrap();

    (alpha, beta)
}

#[test]
fn udm_compat_emits_all_four_forms_across_namespaces() {
    let dir = unique_dir("on");
    let (alpha, beta) = write_fixture(&dir);
    let out = dir.join("gen");
    run_ingot(&[
        "--model",
        alpha.to_str().unwrap(),
        "--model",
        beta.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--udm-compat",
    ]);

    // --- Form 1: uppercase DATAMODEL_{GET,SET}_<PATH> aliases ---
    // Plain bool/int keys (no enum) get an OBJECT-like macro (no parameter
    // list): the original UDM generator emitted DATAMODEL_GET_<PATH> as a
    // real, address-of-able function for these types (durableInterface.c's
    // DCI dispatch table takes its address without invoking it) -- a
    // function-like macro is left unexpanded (undefined) when not
    // immediately followed by "(", which a call-style macro cannot support.
    // String keys keep the call-style macro (no address-of caller for those;
    // matches UDM's own shape for strings). ALPHA_STATUS_MODE and
    // BETA_SENSOR_LEVEL are enum-typed, so their GET side is asserted
    // separately below (Form 1b) as a real cast-wrapper function, not a
    // macro alias, per P2-20260707-013.
    let helpers = read(&out.join("dm_helpers.h"));
    assert!(
        helpers.contains("#define DATAMODEL_SET_ALPHA_STATUS_MODE DataModel_Set_ALPHA_STATUS_MODE"),
        "missing uppercase SET object-macro for enum key (setter keeps raw storage type):\n{helpers}"
    );
    // String key keeps the call-style macro form.
    assert!(
        helpers.contains("#define DATAMODEL_GET_ALPHA_STATUS_LABEL()"),
        "missing uppercase GET macro for string key:\n{helpers}"
    );

    // --- Form 1b: enum-typed GET is a real static-inline cast wrapper, not a
    // macro alias (P2-20260707-013) --- original UDM's DATAMODEL_GET_<PATH>
    // for enum keys casts the native getter's raw storage-typed return to
    // the enum type; a bare macro alias would leave callers assigning a raw
    // uint8_t/uint16_t to an enum-typed variable, which fails to compile
    // under C++ strict typing (see shadowfsm.cpp's
    // `ROBOT_STATE_STATE_ENUM_T state = DATAMODEL_GET_ROBOT_STATE_STATE();`).
    assert!(
        helpers.contains(
            "static inline ALPHA_STATUS_MODE_ENUM_T DATAMODEL_GET_ALPHA_STATUS_MODE(void)"
        ),
        "missing enum-getter-cast wrapper signature for ALPHA_STATUS_MODE:\n{helpers}"
    );
    assert!(
        helpers.contains(
            "return (ALPHA_STATUS_MODE_ENUM_T)DataModel_Get_ALPHA_STATUS_MODE();"
        ),
        "enum-getter-cast wrapper must cast the native getter's return to the enum type:\n{helpers}"
    );
    assert!(
        !helpers
            .contains("#define DATAMODEL_GET_ALPHA_STATUS_MODE DataModel_Get_ALPHA_STATUS_MODE"),
        "enum-typed GET must not regress to the raw-storage-typed macro alias:\n{helpers}"
    );
    // Cross-namespace: beta's enum key gets the same treatment.
    assert!(
        helpers.contains(
            "static inline BETA_SENSOR_LEVEL_ENUM_T DATAMODEL_GET_BETA_SENSOR_LEVEL(void)"
        ),
        "missing cross-namespace enum-getter-cast wrapper signature for BETA_SENSOR_LEVEL:\n{helpers}"
    );
    assert!(
        helpers.contains("return (BETA_SENSOR_LEVEL_ENUM_T)DataModel_Get_BETA_SENSOR_LEVEL();"),
        "cross-namespace enum-getter-cast wrapper must cast to the enum type:\n{helpers}"
    );

    // --- Form 2: lowercase DataModel_{Get,Set}<path> symbol aliases ---
    assert!(
        helpers.contains("#define DataModel_Getalpha_status_mode DataModel_Get_ALPHA_STATUS_MODE"),
        "missing lowercase GET alias:\n{helpers}"
    );
    assert!(
        helpers.contains("#define DataModel_Setalpha_status_mode DataModel_Set_ALPHA_STATUS_MODE"),
        "missing lowercase SET alias:\n{helpers}"
    );
    assert!(
        helpers.contains("#define DataModel_Getbeta_sensor_level DataModel_Get_BETA_SENSOR_LEVEL"),
        "missing cross-namespace lowercase GET alias:\n{helpers}"
    );

    // --- Form 3: bare-key `#define <PATH> DM_KEY_<PATH>` constants ---
    let keys = read(&out.join("key_definitions.h"));
    assert!(
        keys.contains("#define ALPHA_STATUS_MODE DM_KEY_ALPHA_STATUS_MODE"),
        "missing bare-key alias:\n{keys}"
    );
    assert!(
        keys.contains("#define ALPHA_STATUS_LABEL DM_KEY_ALPHA_STATUS_LABEL"),
        "missing bare-key alias for string key:\n{keys}"
    );
    assert!(
        keys.contains("#define BETA_SENSOR_LEVEL DM_KEY_BETA_SENSOR_LEVEL"),
        "missing cross-namespace bare-key alias:\n{keys}"
    );

    // --- Form 4: <PATH>_ENUM_T typedefs + named constants (values == schema) ---
    let enums = read(&out.join("dm_enums.h"));
    assert!(
        enums.contains("} ALPHA_STATUS_MODE_ENUM_T;"),
        "missing ALPHA enum typedef:\n{enums}"
    );
    assert!(
        enums.contains("ALPHA_STATUS_MODE_OFF = 0,"),
        "missing/incorrect enum constant OFF:\n{enums}"
    );
    // Value must equal the schema's declared value (non-sequential 7).
    assert!(
        enums.contains("ALPHA_STATUS_MODE_ACTIVE = 7,"),
        "enum constant ACTIVE must carry the schema value 7:\n{enums}"
    );
    assert!(
        enums.contains("ALPHA_STATUS_MODE_MAX_STORAGE_VALUE = UINT8_MAX"),
        "missing uint8 storage sentinel:\n{enums}"
    );
    // beta: uint16 sentinel + schema value 42.
    assert!(
        enums.contains("} BETA_SENSOR_LEVEL_ENUM_T;"),
        "missing BETA enum typedef:\n{enums}"
    );
    assert!(
        enums.contains("BETA_SENSOR_LEVEL_HIGH = 42,"),
        "enum constant HIGH must carry the schema value 42:\n{enums}"
    );
    assert!(
        enums.contains("BETA_SENSOR_LEVEL_MAX_STORAGE_VALUE = UINT16_MAX"),
        "uint16 enum key must use the UINT16_MAX sentinel:\n{enums}"
    );

    // --- Form 5: dm_key_definitions.h alias for the dropped gen/udm filename ---
    let key_defs_alias = read(&out.join("dm_key_definitions.h"));
    assert!(
        key_defs_alias.contains("#include \"key_definitions.h\""),
        "dm_key_definitions.h must alias the canonical key_definitions.h:\n{key_defs_alias}"
    );

    // --- Form 6: dm_boolean_storage.h alias for the dropped gen/udm filename ---
    // (P2-20260707-014 Gap A) libBissellIoT's telemetry.c/endpoint_handler.c/
    // local_comm.c/udm_comm.c #include "dm_boolean_storage.h" verbatim for the
    // BooleanStorage_SetKey/GetKey declarations; alias to the canonical
    // boolean_storage.h rather than hand-maintaining a duplicate.
    let bool_storage_alias = read(&out.join("dm_boolean_storage.h"));
    assert!(
        bool_storage_alias.contains("#include \"boolean_storage.h\""),
        "dm_boolean_storage.h must alias the canonical boolean_storage.h:\n{bool_storage_alias}"
    );
    // The alias must not redeclare the API itself as a function prototype — the
    // fixture has a bool key, so the canonical header carries the real
    // declarations already; the alias is purely an #include forward.
    assert!(
        !bool_storage_alias.contains("bool BooleanStorage_SetKey("),
        "dm_boolean_storage.h must forward, not redeclare, BooleanStorage_SetKey:\n{bool_storage_alias}"
    );
    let bool_storage = read(&out.join("boolean_storage.h"));
    assert!(
        bool_storage.contains("bool BooleanStorage_SetKey(uint32_t key, bool value);"),
        "canonical boolean_storage.h must declare BooleanStorage_SetKey:\n{bool_storage}"
    );
    assert!(
        bool_storage.contains("bool BooleanStorage_GetKey(uint32_t key, bool *result);"),
        "canonical boolean_storage.h must declare BooleanStorage_GetKey:\n{bool_storage}"
    );

    // --- Fidelity 1: enum members reproduce the UDM spelling verbatim ---
    // camelCase source value collapses to a concatenated member (UDM's
    // ROBOT_STATE_WET_DRY_MODE_WETANDDRY class), while a snake_case source
    // value retains its underscore (UDM's WIFI_THREADHEALTHCHECK_BITS_
    // NETWORK_MONITOR class). The emitter must neither strip nor insert
    // separators — UDM's own naming is inconsistent, so only verbatim
    // uppercasing of UDM-cased source names reproduces every member.
    assert!(
        enums.contains("ALPHA_STATUS_MODE_WETANDDRY = 2,"),
        "camelCase enum value must concatenate to the UDM spelling:\n{enums}"
    );
    assert!(
        !enums.contains("ALPHA_STATUS_MODE_WET_AND_DRY"),
        "camelCase value must not gain inserted underscores:\n{enums}"
    );
    assert!(
        enums.contains("ALPHA_STATUS_MODE_NIGHT_MODE = 3,"),
        "snake_case enum value must retain its underscore (UDM keeps these):\n{enums}"
    );
    assert!(
        !enums.contains("ALPHA_STATUS_MODE_NIGHTMODE"),
        "snake_case value must not be stripped to a concatenated spelling:\n{enums}"
    );

    // --- Fidelity 2: lowercase accessor uses the camelCase display name ---
    // The class/key carry the UDM id/name split (id "awsjob"/"versionnumber",
    // name "awsJob"/"versionNumber"). The lowercase alias must use the camelCase
    // NAME segments to match UDM symbols like
    // DataModel_Getlocal_config_awsJob_versionNumber, while the uppercase C
    // surface derives independently from the lowercase/concatenated id.
    assert!(
        helpers.contains(
            "#define DataModel_Getalpha_awsJob_versionNumber \
DataModel_Get_ALPHA_AWSJOB_VERSIONNUMBER"
        ),
        "lowercase GET alias must use the camelCase display names:\n{helpers}"
    );
    assert!(
        helpers.contains(
            "#define DataModel_Setalpha_awsJob_versionNumber \
DataModel_Set_ALPHA_AWSJOB_VERSIONNUMBER"
        ),
        "lowercase SET alias must use the camelCase display names:\n{helpers}"
    );
    // Regression guard: the id-derived all-lowercase alias (the pre-fix bug that
    // dropped the camelCase, breaking lib call-sites) must NOT be emitted.
    assert!(
        !helpers.contains("DataModel_Getalpha_awsjob_versionnumber"),
        "id-derived lowercased accessor must not be emitted when a name exists:\n{helpers}"
    );
    // The uppercase macro/by-key surface derives from id.to_uppercase() (int32
    // non-string key -> object-macro form, per the address-of-able rule above).
    assert!(
        helpers.contains(
            "#define DATAMODEL_GET_ALPHA_AWSJOB_VERSIONNUMBER \
DataModel_Get_ALPHA_AWSJOB_VERSIONNUMBER"
        ),
        "uppercase surface must derive from the uppercased id:\n{helpers}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_flag_no_udm_compat_surface_is_emitted() {
    let dir = unique_dir("off");
    let (alpha, beta) = write_fixture(&dir);
    let out = dir.join("gen");
    run_ingot(&[
        "--model",
        alpha.to_str().unwrap(),
        "--model",
        beta.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);

    assert!(
        !out.join("dm_enums.h").exists(),
        "dm_enums.h must not be emitted without --udm-compat"
    );

    let helpers = read(&out.join("dm_helpers.h"));
    assert!(
        !helpers.contains("DATAMODEL_GET_ALPHA_STATUS_MODE"),
        "uppercase macro aliases must not appear without --udm-compat"
    );
    assert!(
        !helpers.contains("DataModel_Getalpha_status_mode"),
        "lowercase symbol aliases must not appear without --udm-compat"
    );
    assert!(
        !helpers.contains("DataModel_Getalpha_awsJob_versionNumber"),
        "camelCase lowercase aliases must not appear without --udm-compat"
    );

    let keys = read(&out.join("key_definitions.h"));
    assert!(
        !keys.contains("#define ALPHA_STATUS_MODE DM_KEY_ALPHA_STATUS_MODE"),
        "bare-key aliases must not appear without --udm-compat"
    );

    assert!(
        !out.join("dm_key_definitions.h").exists(),
        "dm_key_definitions.h must not be emitted without --udm-compat"
    );

    assert!(
        !out.join("dm_boolean_storage.h").exists(),
        "dm_boolean_storage.h must not be emitted without --udm-compat"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `dm_boolean_storage.h` is gated on the model actually carrying a boolean
/// key (mirrors `boolean_storage.h` itself): with `--udm-compat` but no bool
/// storage, neither file should appear.
#[test]
fn udm_compat_skips_boolean_storage_alias_when_no_bool_keys() {
    let dir = unique_dir("no_bool");
    let beta = dir.join("beta.toml");
    fs::write(
        &beta,
        r#"
[meta]
id = "beta"
namespace_id = 2
version = "1.0.0"

[enums.level]
doc = "sensor level"
[enums.level.values]
low = 1
high = 42

[[classes]]
id = "sensor"

    [[classes.keys]]
    id = "level"
    type = "uint16"
    enum = "level"
    default = 1
    helpers = true
"#,
    )
    .unwrap();
    let out = dir.join("gen");
    run_ingot(&[
        "--model",
        beta.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--udm-compat",
    ]);

    assert!(
        !out.join("boolean_storage.h").exists(),
        "boolean_storage.h must not be emitted for a model with no bool keys"
    );
    assert!(
        !out.join("dm_boolean_storage.h").exists(),
        "dm_boolean_storage.h alias must not be emitted without boolean storage"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// P2-20260707-014 Gap B: `--udm-compat` must alias the original UDM's
/// `DATA_MODEL_KEY_TYPE_*` enum names onto ingot's `DM_KEY_TYPE_*`
/// enumerators in `dm_key.h`, as compile-time-constant `#define`s (not a
/// separate runtime-valued enum) — libBissellIoT's telem_broker_handler.c
/// uses them as array indices in a static initializer, which requires a
/// real integer constant expression, and the recovered original
/// gen/udm/dm_key.h declared these values identically (BOOLEAN=0, UINT8=1,
/// UINT16=2, UINT32=3, INT8=4, INT16=5, INT32=6, STRING=8).
#[test]
fn udm_compat_emits_data_model_key_type_aliases_in_dm_key_h() {
    let dir = unique_dir("keytype_on");
    let (alpha, beta) = write_fixture(&dir);
    let out = dir.join("gen");
    run_ingot(&[
        "--model",
        alpha.to_str().unwrap(),
        "--model",
        beta.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--udm-compat",
    ]);

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
    ] {
        let found = dm_key.lines().any(|line| {
            line.starts_with(&format!("#define {alias}"))
                && line.split_whitespace().nth(2) == Some(target)
        });
        assert!(
            found,
            "missing DATA_MODEL_KEY_TYPE alias {alias} -> {target}:\n{dm_key}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Without `--udm-compat`, none of the `DATA_MODEL_KEY_TYPE_*` aliases
/// should appear in `dm_key.h` — the flag gates every UDM-compat surface,
/// not just the ones exercised elsewhere in this suite.
#[test]
fn without_flag_no_data_model_key_type_aliases_are_emitted() {
    let dir = unique_dir("keytype_off");
    let (alpha, beta) = write_fixture(&dir);
    let out = dir.join("gen");
    run_ingot(&[
        "--model",
        alpha.to_str().unwrap(),
        "--model",
        beta.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);

    let dm_key = read(&out.join("dm_key.h"));
    assert!(
        !dm_key.contains("DATA_MODEL_KEY_TYPE"),
        "DATA_MODEL_KEY_TYPE aliases must not appear without --udm-compat:\n{dm_key}"
    );

    let _ = fs::remove_dir_all(&dir);
}
