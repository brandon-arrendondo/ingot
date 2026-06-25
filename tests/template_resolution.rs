//! Tests that ingot finds templates when run from a directory with no local
//! `templates/` copy — simulating what happens after a package install.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Absolute path to the crate-root `templates/` directory baked in at
/// compile time, used as a stand-in for `/usr/share/ingot/templates/`.
fn installed_templates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates")
}

/// Run `ingot` with `args` from `cwd`, returning the Command output.
fn run_ingot_from(cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ingot"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawn ingot")
}

/// Assert the output dir contains at least one generated .c file.
fn assert_generated(out: &std::path::Path) {
    let c_files: Vec<_> = fs::read_dir(out)
        .expect("read output dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "c"))
        .collect();
    assert!(
        !c_files.is_empty(),
        "expected generated .c files in {}, found none",
        out.display()
    );
}

/// Run from a bare tempdir (no `templates/` present) using `--templates`.
#[test]
fn explicit_flag_finds_templates_outside_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("generated");

    // Verify there really is no templates/ in the cwd.
    assert!(
        !tmp.path().join("templates").exists(),
        "test invariant: no templates/ in tempdir"
    );

    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/minimal.toml");
    let templates = installed_templates_dir();

    let result = run_ingot_from(
        tmp.path(),
        &[
            "--model",
            model.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--templates",
            templates.to_str().unwrap(),
        ],
    );

    assert!(
        result.status.success(),
        "ingot failed with --templates flag:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    assert_generated(&out);
}

/// Same scenario, but the path is supplied via the `INGOT_TEMPLATES_DIR` env var.
#[test]
fn env_var_finds_templates_outside_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("generated");

    assert!(
        !tmp.path().join("templates").exists(),
        "test invariant: no templates/ in tempdir"
    );

    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/minimal.toml");
    let templates = installed_templates_dir();

    let result = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .current_dir(tmp.path())
        .env("INGOT_TEMPLATES_DIR", templates.to_str().unwrap())
        .args([
            "--model",
            model.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn ingot");

    assert!(
        result.status.success(),
        "ingot failed with INGOT_TEMPLATES_DIR set:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    assert_generated(&out);
}

/// A non-existent explicit path must produce a clear error, not a panic.
#[test]
fn explicit_flag_nonexistent_path_gives_clear_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("generated");
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/minimal.toml");

    let result = run_ingot_from(
        tmp.path(),
        &[
            "--model",
            model.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--templates",
            "/nonexistent/path/to/templates",
        ],
    );

    assert!(
        !result.status.success(),
        "ingot should have failed for a nonexistent templates path"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("does not exist"),
        "expected 'does not exist' in stderr, got: {stderr}"
    );
}
