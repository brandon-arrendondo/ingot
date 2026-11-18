//! The per-variant bounds dsh_tables.h states for a shadow consumer's buffers (shadow table
//! interface minor 8): rejected-list members, desired path length, published error ids and the
//! largest error id; minor 9: the longest desired value text. tests/fixtures/dsh/check_dsh.c
//! cross-checks them against the tables in C.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// @brief The value of `#define <name> <n>u` in the dsh_tables.h of `gen`.
/// @version 1
fn bound(gen: &Path, name: &str) -> u64 {
    let h = fs::read_to_string(gen.join("dsh_tables.h")).unwrap();
    let rest = h
        .split(&format!("#define {name} "))
        .nth(1)
        .unwrap_or_else(|| panic!("{name} missing from:\n{h}"));
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap()
}

/// @brief The five bounds of `gen`, in header order.
/// v2: the desired value bound.
/// @version 2
fn bounds(gen: &Path) -> [u64; 5] {
    [
        bound(gen, "DSH_REJECTED_MAX_ITEMS"),
        bound(gen, "DSH_DESIRED_PATH_MAX_BYTES"),
        bound(gen, "DSH_ERROR_IDS_PUBLISHED"),
        bound(gen, "DSH_ERROR_ID_MAX"),
        bound(gen, "DSH_DESIRED_VALUE_MAX_BYTES"),
    ]
}

/// @brief Generate the 1.1 model with `edits` for `variant` under `dir/tag`, walk its tables in
/// C (which recomputes the bounds), and read its bounds.
/// v2: five bounds; the C walk.
/// @version 2
fn generated(dir: &Path, tag: &str, variant: &str, edits: &[(&str, &str, &str)]) -> [u64; 5] {
    let case = dir.join(tag);
    edited_copy("v1_1", &case, edits);
    let out = case.join(variant);
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
    assert!(ok, "{tag} ({variant}): {stderr}");
    walk_dsh_tables(&out);
    bounds(&out)
}

#[test]
fn the_generic_model_states_its_bounds_per_variant() {
    let dir = unique_dir("dsh_bounds");
    // no rejected list; "settings.speedLimit" and "settings.planExists" are 19 bytes;
    // product publishes bits 1..31 (bit 0 reserved), dock bits 0..31; the float echo
    // "settings.trim" has the longest value text, 47, over "normal" (2 + 6 * 6) and a u16 (5).
    assert_eq!(generated(&dir, "a", "sku_a", &[]), [0, 20, 63, 63, 48]);
    // sku_b numbers its dock errors from 50: the largest id is e81.
    assert_eq!(generated(&dir, "b", "sku_b", &[]), [0, 20, 63, 81, 48]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_accept_alias_on_one_variant_sets_that_variants_value_bound() {
    let dir = unique_dir("dsh_bounds_alias");
    let from = "accept = [\"quiet\"] }";
    let to = "accept = [\"quiet\", \"whisper_quietly\"] }";
    let edits = [("ns/drive.toml", from, to)];
    // sku_b accepts a 15-byte alias: 2 + 6 * 15 + 1; sku_a keeps the float echo's 47 + 1.
    assert_eq!(generated(&dir, "b", "sku_b", &edits)[4], 93);
    assert_eq!(generated(&dir, "a", "sku_a", &edits)[4], 48);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_string_echo_counts_six_bytes_per_byte() {
    let dir = unique_dir("dsh_bounds_string");
    let from = "type = \"bool\"\n\n[[shadow_echo]]";
    let to = "type = \"string\"\nmax_size = 10\n\n[[shadow_echo]]";
    let edits = [("contract.toml", from, to)];
    assert_eq!(generated(&dir, "s", "sku_a", &edits)[4], 2 + 60 + 1);
    let _ = fs::remove_dir_all(&dir);
}

/// The float echo "settings.trim" (47) as an int echo (11), so that a shorter row can lead.
const INT_TRIM: (&str, &str, &str) = (
    "contract.toml",
    "type = \"float\"\ncommand_key",
    "type = \"int\"\ncommand_key",
);

#[test]
fn an_enum_path_counts_its_longest_inbound_string_escaped() {
    let dir = unique_dir("dsh_bounds_enum");
    // "settings.fanSpeed" leads: "normal" as 2 + 6 * 6.
    assert_eq!(generated(&dir, "e", "sku_a", &[INT_TRIM])[4], 39);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_enum_integer_path_counts_its_key_type() {
    let dir = unique_dir("dsh_bounds_integer");
    let anchor = "shadow = \"settings.fanSpeed\"\napply_group";
    let integer = "shadow = { path = \"settings.fanSpeed\", as = \"integer\" }\napply_group";
    let rule = "inbound_unknown = \"quiet\"            # an unmatched string is quiet (code 0), as today\n";
    let edits = [
        INT_TRIM,
        ("ns/drive.toml", anchor, integer),
        ("ns/drive.toml", rule, ""),
    ];
    // "settings.fanSpeed" carries its uint8 code (3); the int echo leads.
    assert_eq!(generated(&dir, "i", "sku_a", &edits)[4], 12);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_string_desired_path_counts_six_bytes_per_byte_of_its_size() {
    let dir = unique_dir("dsh_bounds_desired_string");
    let anchor = "[[classes.keys]]\nid = \"mode\"";
    let key = format!(
        "[[classes.keys]]\nid = \"nickname\"\nkey_index = 20\ntype = \"string\"\nmax_size = 20\n\
         introduced_in = \"1.1\"\nwrite_rate = \"1/min\"\ndefault = \"\"\n\
         shadow = \"settings.nickname\"\n\n{anchor}"
    );
    let twin = "telem = \"dock\"\n\n[[classes.keys]]\nid = \"nickname_applied\"\nkey_index = 14\n\
                type = \"string\"\nmax_size = 20\nintroduced_in = \"1.1\"\n\
                shadow = \"settings.nickname\"\napplies = \"drive.settings.nickname\"";
    let edits = [
        ("ns/drive.toml", anchor, key.as_str()),
        ("ns/drive.toml", "telem = \"dock\"", twin),
    ];
    assert_eq!(generated(&dir, "d", "sku_a", &edits)[4], 2 + 120 + 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_rejected_list_holds_one_member_per_desired_and_echo_path() {
    let dir = unique_dir("dsh_bounds_rejected");
    let at = "[[shadow_constant]]\npath = \"state.connected\"";
    let declared = format!("[shadow_rejected]\npath = \"state.rejected\"\n\n{at}");
    let edits = [("contract.toml", at, declared.as_str())];
    assert_eq!(generated(&dir, "r", "sku_a", &edits)[0], 4);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_longest_desired_or_echo_path_sets_the_path_bound() {
    let dir = unique_dir("dsh_bounds_path");
    let from = "path = \"settings.planExists\"";
    let to = "path = \"settings.planExistsLonger\"";
    let edits = [("contract.toml", from, to)];
    assert_eq!(generated(&dir, "p", "sku_a", &edits)[1], 26);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reserved_bits_and_publish_max_bit_bound_the_published_ids() {
    let dir = unique_dir("dsh_bounds_errors");
    let product = "reserved_bits = [0]                  # bit 0 is never an error";
    let dock = "wire_bases = { sku_b = 50 }";
    let edits = [
        (
            "contract.toml",
            product,
            "reserved_bits = [0, 2]\npublish_max_bit = 3",
        ),
        (
            "contract.toml",
            dock,
            "wire_bases = { sku_b = 50 }\npublish_max_bit = 7",
        ),
    ];
    // product bits 1 and 3, dock bits 0..7 from 32: ten ids, the largest e39.
    assert_eq!(generated(&dir, "e", "sku_a", &edits)[2..4], [10, 39]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_declared_bit_above_the_published_range_still_bounds_the_largest_id() {
    let dir = unique_dir("dsh_bounds_declared");
    let product = "reserved_bits = [0]                  # bit 0 is never an error";
    let dock = "wire_bases = { sku_b = 50 }";
    let edits = [
        (
            "contract.toml",
            product,
            "reserved_bits = [0]\npublish_max_bit = 0",
        ),
        (
            "contract.toml",
            dock,
            "wire_bases = { sku_b = 50 }\nreserved_bits = [0]\npublish_max_bit = 0",
        ),
    ];
    // nothing is published; product's key on bit 1 can still report e1.
    assert_eq!(generated(&dir, "d", "sku_a", &edits)[2..4], [0, 1]);
    let _ = fs::remove_dir_all(&dir);
}
