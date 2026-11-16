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

const MARKER: &str = "/* templates from the working directory */";

/// @brief A `templates/` copy in `cwd` whose `jenkins_hash.h` and `dmt_types.h` are the marker.
/// @version 1
fn marked_templates_in(cwd: &std::path::Path) {
    let dir = cwd.join("templates");
    fs::create_dir(&dir).unwrap();
    for entry in fs::read_dir(installed_templates_dir()).unwrap() {
        let path = entry.unwrap().path();
        fs::copy(&path, dir.join(path.file_name().unwrap())).unwrap();
    }
    fs::write(dir.join("jenkins_hash.h"), MARKER).unwrap();
    fs::write(dir.join("dmt_types.h"), MARKER).unwrap();
}

/// Model mode without --templates uses a `templates/` in the working directory before the
/// built-in templates, as upstream's search path did.
#[test]
fn model_mode_uses_templates_in_the_working_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    marked_templates_in(tmp.path());
    let out = tmp.path().join("generated");
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/minimal.toml");
    let args = [
        "--model",
        model.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ];
    let result = run_ingot_from(tmp.path(), &args);
    assert!(result.status.success(), "{result:?}");
    let header = fs::read_to_string(out.join("jenkins_hash.h")).unwrap();
    assert_eq!(header.trim_end(), MARKER);
}

/// A stale `templates/` in the working directory, without a template this ingot added, still
/// generates it from the built-in copy and names the template and the directory in a warning.
#[test]
fn a_stale_searched_directory_falls_back_per_template() {
    let tmp = tempfile::tempdir().expect("tempdir");
    marked_templates_in(tmp.path());
    fs::remove_file(tmp.path().join("templates/dm_key_event_list.hpp")).unwrap();
    let out = tmp.path().join("generated");
    let stderr = tinyfsm_full_from(tmp.path(), &out);
    let list = fs::read_to_string(out.join("dm_key_event_list.hpp")).unwrap();
    assert!(list.contains("dm_event_list"), "{list}");
    let dir = tmp.path().join("templates");
    let named = format!("dm_key_event_list.hpp is not in {}", dir.display());
    assert!(stderr.contains(&named), "{stderr}");
}

/// @brief Generate examples/full.toml with --emit-tinyfsm from `cwd` into `out`, asserting
/// success.
/// @version 1
/// @return ingot's stderr
fn tinyfsm_full_from(cwd: &std::path::Path, out: &std::path::Path) -> String {
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/full.toml");
    let (m, o) = (model.to_str().unwrap(), out.to_str().unwrap());
    let result = run_ingot_from(cwd, &["--model", m, "--output", o, "--emit-tinyfsm"]);
    assert!(result.status.success(), "{result:?}");
    String::from_utf8_lossy(&result.stderr).into_owned()
}

/// Contract mode and --table-types never search: a `templates/` in the working directory
/// leaves their output the built-in templates'.
#[test]
fn contract_mode_and_table_types_ignore_the_search_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    marked_templates_in(tmp.path());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract = root.join("examples/contract/v1_0/contract.toml");
    let (gen, types) = (tmp.path().join("gen"), tmp.path().join("types"));
    let c = contract.to_str().unwrap();
    let g = gen.to_str().unwrap();
    let contract_args = [
        "--contract",
        c,
        "--variant",
        "sku_a",
        "--output",
        g,
        "--allow-dirty-ingot",
    ];
    assert_built_in_types(tmp.path(), &contract_args, &gen);
    let types_args = ["--table-types", "--output", types.to_str().unwrap()];
    assert_built_in_types(tmp.path(), &types_args, &types);
}

/// @brief Run ingot from `cwd` with `args` and assert `out/dmt_types.h` is the built-in one.
/// @version 1
fn assert_built_in_types(cwd: &std::path::Path, args: &[&str], out: &std::path::Path) {
    let result = run_ingot_from(cwd, args);
    assert!(result.status.success(), "{result:?}");
    let header = fs::read_to_string(out.join("dmt_types.h")).unwrap();
    assert!(
        header.contains("DMT_"),
        "built-in dmt_types.h expected in {out:?}"
    );
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
