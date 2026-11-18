//! The internal row constants of `dsh_tables.h` (shadow table interface minor 10): one
//! `DSH_INTERNAL_<VERB>` define per internal shadow_commands row of the variant that declares an
//! internal_id, named by the text that selects the row, and walked in C against dsh_commands.

mod common;

use common::*;
use std::fs;
use std::path::{Path, PathBuf};

/// The stop row of the 1.1 model, the text the internal rows are appended after.
const STOP_ROW: &str =
    "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"";

/// @brief One internal `[[shadow_commands]]` row on `topic` selected by `selector` (TOML inline
/// table), with `extra` lines (body, variants) and id `id`.
/// @version 1
fn internal_row(topic: &str, selector: &str, extra: &str, id: u16) -> String {
    format!(
        "\n\n[[shadow_commands]]\ntopic = \"{topic}\"\n{extra}selector = {selector}\n\
         internal = \"the library handles it\"\ninternal_id = {id}"
    )
}

/// @brief Generate the 1.1 model with `rows` appended after its stop row, for `variant`.
/// @version 1
fn generated_with(dir: &Path, variant: &str, rows: &str) -> PathBuf {
    let case = dir.join("model");
    let to = format!("{STOP_ROW}{rows}");
    edited_copy("v1_1", &case, &[("ns/drive.toml", STOP_ROW, to.as_str())]);
    let out = dir.join(variant);
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", variant]);
    assert!(ok, "{variant}: {stderr}");
    out
}

/// @brief The internal defines of `gen` as `NAME=value` texts.
/// @version 1
fn named(gen: &Path) -> Vec<String> {
    internal_defines(gen)
        .into_iter()
        .map(|(n, v)| format!("{n}={v}"))
        .collect()
}

/// `variants = ["<v>"]` with the build the row is grounded in.
const ON_SKU_A: &str = "variants = [\"sku_a\"]\ngrounded_in = \"the sku_a build\"\n";
const ON_SKU_B: &str = "variants = [\"sku_b\"]\ngrounded_in = \"the sku_b build\"\n";

#[test]
fn each_internal_row_names_its_id_by_the_text_that_selects_it() {
    let dir = unique_dir("internal_named");
    let rows = [
        internal_row(
            "command_topic",
            "{ field = \"state\", wire = \"getlog\" }",
            "",
            7,
        ),
        internal_row(
            "mapjob_topic",
            "{ field = \"cmd\", element = \"stopPolling\" }",
            "protocol = \"command_array\"\n",
            2,
        ),
        internal_row(
            "command_topic",
            "{ field = \"net.reset\", wire = true }",
            "",
            9,
        ),
        internal_row(
            "command_topic",
            "{ field = \"state\", wire = \"sys\" }",
            "body = { object = \"sys\", selector = { field = \"op\", wire = \"reboot\" } }\n",
            4,
        ),
    ]
    .concat();
    let gen = generated_with(&dir, "sku_a", &rows);
    assert_eq!(
        named(&gen),
        [
            "DSH_INTERNAL_GETLOG=7u",
            "DSH_INTERNAL_STOPPOLLING=2u",
            "DSH_INTERNAL_NET_RESET=9u",
            "DSH_INTERNAL_REBOOT=4u"
        ]
    );
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rows_sharing_a_name_and_an_id_give_one_constant() {
    let dir = unique_dir("internal_shared");
    let rows = [
        internal_row(
            "command_topic",
            "{ field = \"state\", wire = \"getlog\" }",
            "",
            7,
        ),
        internal_row(
            "command_topic",
            "{ field = \"action\", wire = \"getlog\" }",
            "",
            7,
        ),
    ]
    .concat();
    let gen = generated_with(&dir, "sku_a", &rows);
    assert_eq!(named(&gen), ["DSH_INTERNAL_GETLOG=7u"]);
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_variant_names_the_ids_of_its_own_rows_only() {
    let dir = unique_dir("internal_variant");
    let getlog = "{ field = \"state\", wire = \"getlog\" }";
    let dump = "{ field = \"state\", wire = \"dump\" }";
    let rows = [
        internal_row("command_topic", getlog, ON_SKU_B, 7),
        internal_row("command_topic", dump, ON_SKU_A, 3),
        internal_row("command_topic", dump, ON_SKU_B, 4),
    ]
    .concat();
    let a = generated_with(&dir, "sku_a", &rows);
    let b = generated_with(&dir, "sku_b", &rows);
    assert_eq!(named(&a), ["DSH_INTERNAL_DUMP=3u"]);
    assert_eq!(
        named(&b),
        ["DSH_INTERNAL_GETLOG=7u", "DSH_INTERNAL_DUMP=4u"]
    );
    walk_dsh_tables(&a);
    walk_dsh_tables(&b);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_c_walk_refuses_a_constant_that_differs_from_the_rows() {
    let dir = unique_dir("internal_walk");
    let getlog = "{ field = \"state\", wire = \"getlog\" }";
    let gen = generated_with(&dir, "sku_a", &internal_row("command_topic", getlog, "", 7));
    assert!(dsh_walk_passes(&gen));
    let (h, c) = (gen.join("dsh_tables.h"), gen.join("dsh_tables.c"));
    let line = "#define DSH_INTERNAL_GETLOG 7u";
    let id_row = ".action = DSH_DO_INTERNAL, .answer = 0u, .internal_id = 7u";
    let answer_row = ".action = DSH_DO_ANSWER, .answer = 0u, .internal_id = 7u";
    // another id; no constant for the row; a constant of a row without an id; a non-internal row
    for (file, from, to) in [
        (&h, line, "#define DSH_INTERNAL_GETLOG 8u"),
        (&h, line, "#define DSH_INTERNAL_OTHER 7u"),
        (
            &h,
            line,
            "#define DSH_INTERNAL_GETLOG 7u\n#define DSH_INTERNAL_GO 0u",
        ),
        (&c, id_row, answer_row),
    ] {
        let kept = fs::read_to_string(file).unwrap();
        edit(file, from, to);
        assert!(!dsh_walk_passes(&gen), "{to}");
        fs::write(file, kept).unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_without_internal_ids_states_no_internal_constant() {
    let dir = unique_dir("internal_none");
    let rows = "\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = { field = \"state\", wire = \"getlog\" }\ninternal = \"the library handles it\"";
    let gen = generated_with(&dir, "sku_a", rows);
    assert!(named(&gen).is_empty());
    let h = fs::read_to_string(gen.join("dsh_tables.h")).unwrap();
    assert!(!h.contains("DSH_INTERNAL_"), "{h}");
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Generate the 1.1 model with `rows` appended after its stop row and assert it is
/// refused with `expected`.
/// @version 1
fn refused_with(tag: &str, rows: &str, expected: &str) {
    let dir = unique_dir(tag);
    let case = dir.join("model");
    let to = format!("{STOP_ROW}{rows}");
    edited_copy("v1_1", &case, &[("ns/drive.toml", STOP_ROW, to.as_str())]);
    let out = dir.join("out");
    let (ok, stderr) = generate(&case.join("contract.toml"), &out, &["--variant", "sku_a"]);
    assert!(!ok, "{tag} must be refused");
    assert!(
        stderr.contains(expected),
        "expected '{expected}' in:\n{stderr}"
    );
    assert!(!out.join("dsh_tables.h").exists(), "nothing is generated");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_verb_that_is_not_a_c_identifier_is_refused() {
    let row = internal_row(
        "command_topic",
        "{ field = \"state\", wire = \"get-log\" }",
        "",
        5,
    );
    refused_with(
        "internal_ident",
        &row,
        "shadow_commands command_topic state=get-log: internal constant DSH_INTERNAL_GET-LOG is not a C identifier",
    );
}

#[test]
fn a_name_over_63_characters_is_refused() {
    let verb = "a".repeat(51);
    let selector = format!("{{ field = \"state\", wire = \"{verb}\" }}");
    let row = internal_row("command_topic", &selector, "", 5);
    let name = format!("DSH_INTERNAL_{}", verb.to_uppercase());
    assert_eq!(name.len(), 64);
    refused_with(
        "internal_long",
        &row,
        &format!("internal constant {name} is longer than 63 characters"),
    );
}

#[test]
fn a_name_at_63_characters_is_accepted() {
    let dir = unique_dir("internal_63");
    let verb = "a".repeat(50);
    let selector = format!("{{ field = \"state\", wire = \"{verb}\" }}");
    let gen = generated_with(
        &dir,
        "sku_a",
        &internal_row("command_topic", &selector, "", 5),
    );
    assert_eq!(internal_defines(&gen)[0].0.len(), 63);
    walk_dsh_tables(&gen);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn one_name_for_two_ids_on_a_shared_variant_is_refused() {
    let rows = [
        internal_row(
            "command_topic",
            "{ field = \"state\", wire = \"getlog\" }",
            ON_SKU_A,
            7,
        ),
        internal_row(
            "command_topic",
            "{ field = \"action\", wire = \"getlog\" }",
            "",
            8,
        ),
    ]
    .concat();
    refused_with(
        "internal_collide",
        &rows,
        "shadow_commands command_topic state=getlog: internal constant DSH_INTERNAL_GETLOG also names shadow_commands command_topic action=getlog (internal_id 8)",
    );
}
