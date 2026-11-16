//! Integration tests for `dm_key_has_event.hpp`, the has-event predicate emitted under
//! `--emit-tinyfsm`.
//!
//! The golden test pins the exact bytes for a small fixture model. The agreement test pins the
//! invariant on a richer model: the predicate is true for exactly the keys that have an event
//! struct in `dm_key_events.hpp` and a case in `send_tinyfsm_event_by_key()`, in the same order.
//! The compile-and-run proof against real tinyfsm lives in `tests/fixtures/cxx`, driven by
//! `invoke test`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const HAS_EVENT: &str = "dm_key_has_event.hpp";

/// @brief Create a fresh, uniquely-named temp directory for one test.
/// @version 1
fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_has_event_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// @brief Run ingot on `model` into `out` with `--emit-tinyfsm`, asserting success.
/// @version 1
///
/// CWD is the crate root under `cargo test`, so `templates/` resolves.
fn run_ingot_tinyfsm(model: &str, out: &Path) {
    let args = [
        "--model",
        model,
        "--output",
        out.to_str().unwrap(),
        "--emit-tinyfsm",
    ];
    let status = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .args(args)
        .status()
        .expect("spawn ingot");
    assert!(status.success(), "ingot failed for args {args:?}");
}

/// @brief Read a generated file, panicking with its path on failure.
/// @version 1
fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// @brief Key defines named by `case X:` labels, in order, from a generated switch.
/// @version 1
fn case_labels(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|l| l.trim().strip_prefix("case "))
        .filter_map(|l| l.strip_suffix(':'))
        .map(str::to_string)
        .collect()
}

/// @brief Event struct names, in order, from `dm_key_events.hpp`.
/// @version 1
fn event_structs(hpp: &str) -> Vec<String> {
    hpp.lines()
        .filter_map(|l| l.strip_prefix("struct "))
        .filter_map(|l| l.strip_suffix(" : tinyfsm::Event { };"))
        .map(str::to_string)
        .collect()
}

#[test]
fn has_event_matches_golden() {
    let out = unique_dir("golden");
    run_ingot_tinyfsm("tests/fixtures/events/model.toml", &out);

    let golden = read(Path::new(
        "tests/fixtures/events/dm_key_has_event.hpp.golden",
    ))
    .replace("@VERSION@", env!("CARGO_PKG_VERSION"));
    let actual = read(&out.join(HAS_EVENT));
    assert_eq!(actual, golden, "{HAS_EVENT} drifted from golden");

    let _ = fs::remove_dir_all(&out);
}

#[test]
fn has_event_agrees_with_event_structs_and_dispatch() {
    let out = unique_dir("agree");
    run_ingot_tinyfsm("examples/full.toml", &out);

    let predicate = case_labels(&read(&out.join(HAS_EVENT)));
    let dispatch = case_labels(&read(&out.join("dm_key_events_wrapper.cpp")));
    let structs = event_structs(&read(&out.join("dm_key_events.hpp")));
    assert!(!structs.is_empty(), "full.toml must carry event keys");

    assert_eq!(
        predicate, dispatch,
        "predicate must name exactly the keys the dispatch switch handles, in order"
    );
    let as_events: Vec<String> = predicate
        .iter()
        .map(|k| k.replacen("DM_KEY_", "FSM_EVENT_", 1))
        .collect();
    assert_eq!(
        as_events, structs,
        "predicate must name exactly the keys that have an event struct, in order"
    );

    let _ = fs::remove_dir_all(&out);
}
