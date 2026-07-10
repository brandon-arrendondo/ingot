//! Regression test for deterministic codegen.
//!
//! Generating twice from identical input must produce byte-identical output
//! across *every* generated file. This guards the whole class of bug where the
//! perfect-hash tables were entropy-seeded: `boolean_storage.c`,
//! `integer_storage.c`, and `string_storage.c` previously varied run-to-run in
//! both their `*_HASH_SEED_*` defines and table layout, which would re-diff on
//! every regen of committed generated code.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Create a fresh, uniquely-named temp directory for one run. The tag must be
/// distinct per call so parallel tests (same pid) don't collide.
fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_determinism_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Run the ingot binary, asserting success. CWD is the crate root during
/// `cargo test`, so `templates/` and `examples/` resolve.
fn run_ingot(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .args(args)
        .status()
        .expect("spawn ingot");
    assert!(status.success(), "ingot failed for args {args:?}");
}

/// Read every file under `root` into a `relative-path -> bytes` map, so two
/// independent output dirs compare equal regardless of their temp prefixes.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("strip root prefix")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, fs::read(&path).expect("read file"));
            }
        }
    }
    out
}

/// Generate `model` twice (with `extra` flags) into two temp dirs and assert
/// the full output is byte-identical, file-set and contents alike.
fn assert_byte_identical(tag: &str, model: &str, extra: &[&str]) {
    let dir_a = unique_dir(&format!("{tag}_a"));
    let dir_b = unique_dir(&format!("{tag}_b"));

    let invoke = |out: &Path| {
        let mut args = vec!["--model", model, "--output", out.to_str().unwrap()];
        args.extend_from_slice(extra);
        run_ingot(&args);
    };
    invoke(&dir_a);
    invoke(&dir_b);

    let snap_a = snapshot(&dir_a);
    let snap_b = snapshot(&dir_b);

    assert!(!snap_a.is_empty(), "no files generated for {model}");
    assert_eq!(
        snap_a.keys().collect::<Vec<_>>(),
        snap_b.keys().collect::<Vec<_>>(),
        "generated file set differs between runs for {model} {extra:?}",
    );
    for (name, bytes_a) in &snap_a {
        assert!(
            bytes_a == &snap_b[name],
            "file `{name}` differs between two runs of {model} {extra:?} — codegen is non-deterministic",
        );
    }

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn battery_is_byte_identical_across_runs() {
    assert_byte_identical("battery", "examples/battery.toml", &[]);
}

#[test]
fn minimal_is_byte_identical_across_runs() {
    assert_byte_identical("minimal", "examples/minimal.toml", &[]);
}

#[test]
fn full_is_byte_identical_across_runs() {
    // full.toml exercises every storage type (bool/int/string/persistence),
    // i.e. all three previously-nondeterministic generators at once.
    assert_byte_identical("full", "examples/full.toml", &[]);
}

#[test]
fn full_with_tinyfsm_is_byte_identical_across_runs() {
    assert_byte_identical("full_tinyfsm", "examples/full.toml", &["--emit-tinyfsm"]);
}

#[test]
fn full_no_events_is_byte_identical_across_runs() {
    assert_byte_identical("full_no_events", "examples/full.toml", &["--no-events"]);
}

#[test]
fn full_rust_target_is_byte_identical_across_runs() {
    assert_byte_identical("full_rust", "examples/full.toml", &["--target", "rust"]);
}
