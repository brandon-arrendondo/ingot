//! Integration test for `--target rust`: the generated `dm.rs` must actually
//! compile as a standalone `no_std` crate and its embedded `#[test]`s must
//! pass — not just look plausible. Mirrors how the C backend's output is
//! validated by compiling+running it (see `invoke test`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_rust_target_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_ingot(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .args(args)
        .status()
        .expect("spawn ingot");
    assert!(status.success(), "ingot failed for args {args:?}");
}

/// Generate `dm.rs` from `model`, compile it standalone as a `--test` crate,
/// run the resulting binary, and assert every embedded test passed.
fn assert_generated_rust_compiles_and_tests_pass(tag: &str, model: &str) {
    let out_dir = unique_dir(tag);
    run_ingot(&[
        "--model",
        model,
        "--output",
        out_dir.to_str().unwrap(),
        "--target",
        "rust",
    ]);

    let dm_rs = out_dir.join("dm.rs");
    assert!(dm_rs.is_file(), "expected {} to exist", dm_rs.display());

    let test_bin = out_dir.join("dm_test");
    let compile = Command::new("rustc")
        .args(["--edition", "2021", "--crate-type", "lib", "--test"])
        .arg(&dm_rs)
        .arg("-o")
        .arg(&test_bin)
        .output()
        .expect("spawn rustc");
    assert!(
        compile.status.success(),
        "generated dm.rs for {model} failed to compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(
        compile.stderr.is_empty(),
        "generated dm.rs for {model} compiled with warnings:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&test_bin)
        .output()
        .expect("run compiled tests");
    assert!(
        run.status.success(),
        "embedded tests failed for {model}:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );

    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn minimal_rust_target_compiles_and_tests_pass() {
    assert_generated_rust_compiles_and_tests_pass("minimal", "examples/minimal.toml");
}

#[test]
fn battery_rust_target_compiles_and_tests_pass() {
    assert_generated_rust_compiles_and_tests_pass("battery", "examples/battery.toml");
}

#[test]
fn full_rust_target_compiles_and_tests_pass() {
    assert_generated_rust_compiles_and_tests_pass("full", "examples/full.toml");
}
