//! Integration tests for `--table-types`, which writes `dmt_types.h`, the fixed TIV 1.0
//! table-interface header.
//!
//! The header's frozen content is pinned in C: `tests/fixtures/dmt/check_types.c` asserts every
//! value, struct size and field offset at compile time and checks the rights macro at run time.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// @brief Create a fresh, uniquely-named temp directory for one test.
/// @version 1
fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_tiv_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// @brief Run ingot with `args` from `cwd`, returning whether it succeeded.
/// @version 1
fn ingot_in(cwd: &Path, args: &[&str]) -> bool {
    Command::new(env!("CARGO_BIN_EXE_ingot"))
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("spawn ingot")
        .success()
}

/// @brief Write dmt_types.h into `out` from a working directory with no templates/.
/// @version 1
fn emit_types(out: &Path) {
    let cwd = out.parent().expect("output has a parent");
    assert!(
        ingot_in(cwd, &["--table-types", "--output", out.to_str().unwrap()]),
        "ingot --table-types failed"
    );
}

/// @brief Run the C compiler with `args`, panicking with its stderr on failure.
/// @version 1
fn cc(args: &[&str]) {
    let out = Command::new("cc").args(args).output().expect("spawn cc");
    assert!(
        out.status.success(),
        "cc {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn header_compiles_as_strict_c99() {
    let dir = unique_dir("c99");
    let gen = dir.join("gen");
    emit_types(&gen);
    let tu = dir.join("tu.c");
    fs::write(&tu, "#include \"dmt_types.h\"\nint dmt_tu_anchor;\n").unwrap();
    let obj = dir.join("tu.o");
    let include = format!("-I{}", gen.display());
    cc(&[
        "-std=c99",
        "-Wall",
        "-Wextra",
        "-pedantic",
        "-Werror",
        &include,
        "-c",
        tu.to_str().unwrap(),
        "-o",
        obj.to_str().unwrap(),
    ]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn frozen_values_layout_and_rights_macro() {
    let dir = unique_dir("frozen");
    let gen = dir.join("gen");
    emit_types(&gen);
    let check = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dmt/check_types.c");
    let exe = dir.join("check_types");
    let include = format!("-I{}", gen.display());
    cc(&[
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Werror",
        &include,
        check.to_str().unwrap(),
        "-o",
        exe.to_str().unwrap(),
    ]);
    let status = Command::new(&exe).status().expect("run check_types");
    assert!(status.success(), "DMT_RIGHTS check failed");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn header_bytes_depend_on_nothing_but_the_tiv() {
    let dir = unique_dir("bytes");
    let (a, b) = (dir.join("a"), dir.join("b"));
    emit_types(&a);
    emit_types(&b);
    let first = fs::read(a.join("dmt_types.h")).unwrap();
    assert_eq!(first, fs::read(b.join("dmt_types.h")).unwrap());
    let template = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/dmt_types.h");
    assert_eq!(
        first,
        fs::read(template).unwrap(),
        "rendering must not alter the header"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn table_types_takes_no_model() {
    let dir = unique_dir("nomodel");
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/minimal.toml");
    assert!(
        !ingot_in(&dir, &["--table-types", "--model", model.to_str().unwrap()]),
        "--table-types with --model must be refused"
    );
    let _ = fs::remove_dir_all(&dir);
}
