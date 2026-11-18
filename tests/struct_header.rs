//! Struct value headers: `header = [{ name, type }]` on a struct key lays out a fixed
//! little-endian header before the payload; contract.json carries it, dmt_constants.h gives its
//! constants, native struct and byte-shift codec (round-tripped in C by
//! tests/fixtures/dmt/check_header.c), the SDK binding context lists it, the store tables do not
//! change, a released header is kept within a major, and every malformed header is refused.

mod common;

use common::*;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The generic struct key's declaration line, after which a header is inserted.
const MAP_DATA: &str = "id = \"map_data\"\n";

/// One field of every header type, in the order check_header.c expects.
const EVERY_TYPE: &str =
    "header = [{ name = \"a\", type = \"uint8\" }, { name = \"b\", type = \"int8\" }, \
     { name = \"c\", type = \"uint16\" }, { name = \"d\", type = \"int16\" }, \
     { name = \"e\", type = \"uint32\" }, { name = \"f\", type = \"int32\" }, \
     { name = \"g\", type = \"bool\" }]\n";

/// @brief Copy `version` of the generic model into `dir/model` with `line` (a header and any
/// other key line) added to drive.map.map_data, generate it into `dir/<tag>` with `extra`, and
/// return (success, stderr, output directory).
/// @version 1
fn with_header(
    version: &str,
    dir: &Path,
    tag: &str,
    line: &str,
    extra: &[&str],
) -> (bool, String, std::path::PathBuf) {
    let model_dir = dir.join(format!("model_{tag}"));
    let to = format!("{MAP_DATA}{line}");
    edited_copy(version, &model_dir, &[("ns/drive.toml", MAP_DATA, &to)]);
    let gen = dir.join(tag);
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &gen, extra);
    (ok, stderr, gen)
}

/// @brief dmt_constants.h of `gen`.
/// @version 1
fn constants_h(gen: &Path) -> String {
    fs::read_to_string(gen.join("dmt_constants.h")).unwrap()
}

#[test]
fn contract_json_lays_the_header_out_packed_in_order() {
    let dir = unique_dir("header_json");
    let line = "header = [{ name = \"session_id\", type = \"uint32\" }, \
                { name = \"map_id\", type = \"int32\" }, { name = \"global\", type = \"bool\" }]\n";
    let (ok, stderr, gen) = with_header("v1_1", &dir, "gen", line, &[]);
    assert!(ok, "{stderr}");
    let key = key_json(&gen, "drive.map.map_data");
    assert_eq!(
        key["header"],
        serde_json::json!([
            {"name": "session_id", "type": "uint32", "offset": 0, "size": 4},
            {"name": "map_id", "type": "int32", "offset": 4, "size": 4},
            {"name": "global", "type": "bool", "offset": 8, "size": 1}
        ])
    );
    assert_eq!(key["header_bytes"], 9);
    let other = key_json(&gen, "drive.settings.mode");
    assert!(other.get("header").is_none() && other.get("header_bytes").is_none());
    let plain = dir.join("plain");
    generate_ok("v1_1", &plain, &[]);
    assert_same_store_tables(&gen, &plain);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_a_header_dmt_constants_h_has_no_header_code() {
    let dir = unique_dir("header_none");
    generate_ok("v1_1", &dir, &[]);
    let h = constants_h(&dir);
    assert!(
        !h.contains("#include") && !h.contains("_HEADER_BYTES"),
        "{h}"
    );
    assert!(!h.contains("Struct value headers"), "{h}");
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Build check_header.c against dmt_constants.h in `gen` as strict C11 (with
/// -Wconversion) and run it, asserting every check passes; and compile the header as C++.
/// @version 1
fn round_trip_in_c(gen: &Path) {
    let check = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dmt/check_header.c");
    let exe = gen.join("check_header");
    let include = format!("-I{}", gen.display());
    let strict = [
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Wconversion",
        "-pedantic",
        "-Werror",
    ];
    let files = [check.to_str().unwrap(), "-o", exe.to_str().unwrap()];
    compile(
        "cc",
        &[&strict[..], &[include.as_str()], &files[..]].concat(),
    );
    let status = Command::new(&exe).status().unwrap();
    assert_eq!(status.code(), Some(0), "failed checks in check_header");
    let header = gen.join("dmt_constants.h");
    let cxx = [
        "-fsyntax-only",
        "-Wall",
        "-Wextra",
        "-pedantic",
        "-Werror",
        "-x",
        "c++",
    ];
    compile("c++", &[&cxx[..], &[header.to_str().unwrap()]].concat());
}

#[test]
fn the_generated_codec_round_trips_every_type_in_c() {
    let dir = unique_dir("header_c");
    let (ok, stderr, gen) = with_header("v1_1", &dir, "gen", EVERY_TYPE, &[]);
    assert!(ok, "{stderr}");
    round_trip_in_c(&gen);
    let h = constants_h(&gen);
    assert!(
        h.contains("#define DMT_DRIVE_MAP_MAP_DATA_HEADER_BYTES 15u"),
        "{h}"
    );
    assert!(
        !h.contains("__attribute__") && !h.contains("#pragma"),
        "no packed struct"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_header_on_a_key_of_one_variant_is_generated_on_that_variant_only() {
    let dir = unique_dir("header_variant");
    let line = format!("variants = [\"sku_b\"]\n{EVERY_TYPE}");
    let (ok, stderr, on_b) = with_header("v1_1", &dir, "sku_b", &line, &["--variant", "sku_b"]);
    assert!(ok, "{stderr}");
    round_trip_in_c(&on_b);
    let (ok, stderr, on_a) = with_header("v1_1", &dir, "sku_a", &line, &["--variant", "sku_a"]);
    assert!(ok, "{stderr}");
    assert!(
        !constants_h(&on_a).contains("HEADER"),
        "sku_a has no such key"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_binding_context_lists_the_headers_a_principal_has_a_right_on() {
    let dir = unique_dir("header_sdk");
    let binding = dir.join("binding");
    fs::create_dir_all(&binding).unwrap();
    let body = "{% for h in value_headers %}{{ h.path }} {{ h.ctype }} {{ h.encode }} \
                {{ h.decode }} {{ h.prefix }} {{ h.bytes }} {{ h.payload_max }}\
                {% for f in h.fields %} {{ f.name }}:{{ f.ctype }}@{{ f.offset }}{% endfor %}\
                {% endfor %}";
    fs::write(binding.join("PRINCIPAL.txt"), body).unwrap();
    let line =
        "header = [{ name = \"map_id\", type = \"int32\" }, { name = \"ok\", type = \"bool\" }]\n";
    let args = ["--sdk-binding", binding.to_str().unwrap()];
    let (ok, stderr, gen) = with_header("v1_1", &dir, "gen", line, &args);
    assert!(ok, "{stderr}");
    let read = |p: &str| fs::read_to_string(gen.join(format!("sdk/{p}/{p}.txt"))).unwrap();
    let want =
        "drive.map.map_data dmt_drive_map_map_data_header_t dmt_drive_map_map_data_header_encode \
                dmt_drive_map_map_data_header_decode DMT_DRIVE_MAP_MAP_DATA 5 65531 \
                map_id:int32_t@0 ok:bool@4\n";
    assert_eq!(read("app_map"), want, "a writer");
    assert_eq!(read("integrator"), want, "a reader");
    assert_eq!(read("app_cloud"), "\n", "no right, no header");
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Copy the 1.1 model into `dir/model` with `edits` (from, to) to ns/drive.toml and the
/// namespace files `files` (name, text) added, and generate it (sku_a) into `dir/gen`; returns
/// (success, stderr, output directory).
/// @version 1
fn run_edited(
    dir: &Path,
    edits: &[(&str, &str)],
    files: &[(&str, &str)],
) -> (bool, String, std::path::PathBuf) {
    let model_dir = dir.join("model");
    let all: Vec<(&str, &str, &str)> = edits
        .iter()
        .map(|(f, t)| ("ns/drive.toml", *f, *t))
        .collect();
    edited_copy("v1_1", &model_dir, &all);
    for (name, text) in files {
        fs::write(model_dir.join("ns").join(name), text).unwrap();
    }
    let gen = dir.join("gen");
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &gen, &[]);
    (ok, stderr, gen)
}

/// @brief Assert the 1.1 model (sku_a) with `edits` to ns/drive.toml and `files` added is
/// refused with `expected` in the log, and nothing is generated.
/// v2: and namespace files added, through run_edited.
/// @version 2
fn refused_with(tag: &str, edits: &[(&str, &str)], files: &[(&str, &str)], expected: &str) {
    let dir = unique_dir(&format!("header_refused_{tag}"));
    let (ok, stderr, gen) = run_edited(&dir, edits, files);
    assert!(!ok, "{tag} must be refused");
    assert!(
        stderr.contains(expected),
        "{tag}: '{expected}' in:\n{stderr}"
    );
    assert!(
        !gen.join("dmt_constants.h").exists(),
        "{tag}: nothing generated"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Assert the 1.1 model with `edits` to ns/drive.toml is refused with `expected`.
/// @version 1
fn refused_edits(tag: &str, edits: &[(&str, &str)], expected: &str) {
    refused_with(tag, edits, &[], expected);
}

/// @brief Assert the 1.1 model with `line` on drive.map.map_data is refused with `expected`.
/// @version 1
fn refused(tag: &str, line: &str, expected: &str) {
    refused_edits(tag, &[(MAP_DATA, &format!("{MAP_DATA}{line}"))], expected);
}

/// @brief Assert the 1.1 model with `edits` to ns/drive.toml generates.
/// @version 1
fn accepted(tag: &str, edits: &[(&str, &str)]) {
    let dir = unique_dir(&format!("header_accepted_{tag}"));
    let (ok, stderr, _) = run_edited(&dir, edits, &[]);
    assert!(ok, "{tag}: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

/// The line the generic model's command class starts with, before which a key is added.
const COMMAND_CLASS: &str = "[[classes]]\nid = \"command\"\n";

/// @brief The edit adding a struct key `id` (index 1, max_size 64, one uint8 field) to the class
/// drive.map, before the command class.
/// @version 1
fn added_key(id: &str) -> (&'static str, String) {
    let key = format!(
        "[[classes.keys]]\nid = \"{id}\"\nkey_index = 1\ntype = \"struct\"\nmax_size = 64\n\
         introduced_in = \"1.0\"\n{}\n{COMMAND_CLASS}",
        field("x", "uint8")
    );
    (COMMAND_CLASS, key)
}

/// @brief A one-field header line.
/// @version 1
fn field(name: &str, ty: &str) -> String {
    format!("header = [{{ name = \"{name}\", type = \"{ty}\" }}]\n")
}

#[test]
fn malformed_header_fields_are_refused() {
    refused(
        "empty",
        "header = []\n",
        "drive.map.map_data: header has no field",
    );
    refused(
        "float",
        &field("x", "float32"),
        "header field x: type float32 is not one of",
    );
    refused("string", &field("x", "string"), "type string is not one of");
    refused("unknown", &field("x", "uint64"), "unknown variant `uint64`");
    refused(
        "ident",
        &field("1x", "uint8"),
        "header field 1x is not a C identifier",
    );
    let twice =
        "header = [{ name = \"x\", type = \"uint8\" }, { name = \"x\", type = \"bool\" }]\n";
    refused("twice", twice, "header field x is declared twice");
}

#[test]
fn a_header_on_a_scalar_key_or_filling_max_size_is_refused() {
    let odometer = "id = \"odometer\"\n";
    let on_scalar = format!("{odometer}{}", field("x", "uint8"));
    refused_edits(
        "scalar",
        &[(odometer, &on_scalar)],
        "drive.state.odometer: header on a uint32 key: only a struct key has a header",
    );
    let small = [
        (MAP_DATA, format!("{MAP_DATA}{}", field("x", "uint32"))),
        ("max_size = 65536\n", "max_size = 4\n".to_string()),
    ];
    let small: Vec<(&str, &str)> = small.iter().map(|(f, t)| (*f, t.as_str())).collect();
    refused_edits(
        "full",
        &small,
        "header of 4 bytes leaves no payload in max_size 4",
    );
    let wider = [small[0], ("max_size = 65536\n", "max_size = 2\n")];
    refused_edits(
        "wider",
        &wider,
        "header of 4 bytes leaves no payload in max_size 2",
    );
    accepted(
        "one_payload_byte",
        &[small[0], ("max_size = 65536\n", "max_size = 5\n")],
    );
}

#[test]
fn a_generated_name_over_63_characters_is_refused() {
    // DMT_DRIVE_MAP_MAP_DATA_<field>_OFFSET: 63 characters with a 33-character field
    accepted(
        "field_63",
        &[(
            MAP_DATA,
            &format!("{MAP_DATA}{}", field(&"n".repeat(33), "uint8")),
        )],
    );
    refused(
        "field_64",
        &field(&"n".repeat(34), "uint8"),
        "is longer than 63 characters",
    );
    // dmt_drive_map_<key>_header_decode (and _encode): 63 characters with a 35-character key
    let (at, key) = added_key(&"k".repeat(35));
    accepted("key_63", &[(at, &key)]);
    let (at, key) = added_key(&"k".repeat(36));
    let want = "_header_decode is longer than 63 characters";
    refused_edits("key_64", &[(at, &key)], want);
}

/// A namespace `e` whose enum constants (keys e.m.s, e.m.payload and e.m.header, values
/// x_offset, max and bytes) include DMT_E_E_M_S_X_OFFSET, DMT_E_E_M_PAYLOAD_MAX and
/// DMT_E_E_M_HEADER_BYTES: the offset constant of field s_x of the header of key e.e.m and that
/// header's payload maximum and byte count.
const NAMESPACE_E: &str = "[meta]\nid = \"e\"\nnamespace_id = 9\n\n\
    [enums.s.values]\nx_offset = 0\nmax = 1\nbytes = 2\n\n\
    [[classes]]\nid = \"m\"\nclass_index = 0\nrole = \"state\"\n\n\
    [[classes.keys]]\nid = \"s\"\nkey_index = 0\ntype = \"uint8\"\nenum = \"s\"\nintroduced_in = \"1.0\"\n\n\
    [[classes.keys]]\nid = \"payload\"\nkey_index = 1\ntype = \"uint8\"\nenum = \"s\"\nintroduced_in = \"1.0\"\n\n\
    [[classes.keys]]\nid = \"header\"\nkey_index = 2\ntype = \"uint8\"\nenum = \"s\"\nintroduced_in = \"1.0\"\n\n\
    [[classes]]\nid = \"e\"\nclass_index = 1\nrole = \"state\"\n\n\
    [[classes.keys]]\nid = \"m\"\nkey_index = 0\ntype = \"struct\"\nmax_size = 16\nintroduced_in = \"1.0\"\n\
    header = [{ name = \"s_x\", type = \"uint8\" }]\n";

#[test]
fn a_header_name_that_is_not_an_identifier_or_an_enum_constant_s_is_refused() {
    let (at, key) = added_key("a-b");
    refused_edits(
        "dash",
        &[(at, &key)],
        "header name DMT_DRIVE_MAP_A-B_HEADER_BYTES is not a C identifier",
    );
    for (tag, name, key) in [
        ("offset", "S_X_OFFSET", "s"),
        ("payload", "PAYLOAD_MAX", "payload"),
        ("bytes", "HEADER_BYTES", "header"),
    ] {
        let want = format!("e.e.m: header name DMT_E_E_M_{name} is also e.m.{key}'s");
        refused_with(tag, &[], &[("e.toml", NAMESPACE_E)], &want);
    }
}

#[test]
fn two_fields_naming_one_constant_are_refused() {
    let clash =
        "header = [{ name = \"x\", type = \"uint8\" }, { name = \"X\", type = \"bool\" }]\n";
    refused(
        "clash",
        clash,
        "header name DMT_DRIVE_MAP_MAP_DATA_X_OFFSET is also drive.map.map_data's",
    );
}

/// The released header of the previous-contract tests.
const RELEASED: &str = "header = [{ name = \"session_id\", type = \"uint32\" }, \
     { name = \"map_id\", type = \"int32\" }]\n";

/// @brief Generate 1.0 with `prev_line` on drive.map.map_data as the previous release, then
/// `version` with `line` against it; returns (success, stderr).
/// @version 1
fn against_released(dir: &Path, prev_line: &str, version: &str, line: &str) -> (bool, String) {
    let (ok, stderr, prev) = with_header("v1_0", dir, "prev", prev_line, &[]);
    assert!(ok, "{stderr}");
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    let (ok, stderr, _) = with_header(version, dir, "cur", line, &args);
    (ok, stderr)
}

#[test]
fn a_minor_keeps_a_released_header_exactly() {
    let dir = unique_dir("header_minor");
    let (ok, stderr) = against_released(&dir, RELEASED, "v1_1", RELEASED);
    assert!(ok, "the same header: {stderr}");
    let retyped = RELEASED.replace("\"int32\"", "\"uint32\"");
    let reordered = "header = [{ name = \"map_id\", type = \"int32\" }, \
                     { name = \"session_id\", type = \"uint32\" }]\n";
    let cases = [
        ("added", "", RELEASED),
        ("removed", RELEASED, ""),
        ("retyped", RELEASED, retyped.as_str()),
        ("reordered", RELEASED, reordered),
    ];
    for (tag, before, after) in cases {
        let (ok, stderr) = against_released(&dir.join(tag), before, "v1_1", after);
        assert!(!ok, "{tag} must be refused in a minor");
        let want = "drive.map.map_data: its header was added, removed or changed in a minor";
        assert!(stderr.contains(want), "{tag}: {stderr}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_major_may_change_a_released_header() {
    let dir = unique_dir("header_major");
    let changed = "header = [{ name = \"map_id\", type = \"int16\" }]\n";
    for (tag, after) in [("changed", changed), ("removed", "")] {
        let (ok, stderr) = against_released(&dir.join(tag), RELEASED, "v2_0", after);
        assert!(ok, "{tag} at 2.0: {stderr}");
    }
    let _ = fs::remove_dir_all(&dir);
}
