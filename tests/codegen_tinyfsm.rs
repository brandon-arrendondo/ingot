//! Integration tests for `--emit-tinyfsm` C++/tinyfsm event codegen.
//!
//! These assert the *structure* of the generated artifacts. The compile/link
//! proof against real tinyfsm lives in `tests/fixtures/cxx`, driven by
//! `invoke test`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Create a fresh, uniquely-named temp directory for one test.
fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_tinyfsm_{tag}_{}", std::process::id()));
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

#[test]
fn emit_tinyfsm_generates_event_structs_and_dispatch_switch() {
    let out = unique_dir("on");
    run_ingot(&[
        "--model",
        "examples/full.toml",
        "--output",
        out.to_str().unwrap(),
        "--emit-tinyfsm",
    ]);

    let hpp = read(&out.join("dm_key_events.hpp"));
    assert!(
        hpp.contains("#include \"tinyfsm.hpp\""),
        "missing include:\n{hpp}"
    );
    assert!(
        hpp.contains("struct FSM_EVENT_APPLIANCE_STATUS_MODE : tinyfsm::Event { };"),
        "missing empty event struct:\n{hpp}"
    );

    let wrapper_h = read(&out.join("dm_key_events_wrapper.hpp"));
    assert!(
        wrapper_h.contains("void send_tinyfsm_event_by_key(uint32_t key_id);"),
        "missing wrapper decl:\n{wrapper_h}"
    );

    let wrapper_c = read(&out.join("dm_key_events_wrapper.cpp"));
    assert!(
        wrapper_c.contains("#include \"fsmlist.hpp\""),
        "wrapper must include the consumer seam:\n{wrapper_c}"
    );
    assert!(
        wrapper_c.contains("case DM_KEY_APPLIANCE_STATUS_MODE:"),
        "missing switch case keyed on the ingot define:\n{wrapper_c}"
    );
    assert!(
        wrapper_c.contains("send_tinyfsm_event(FSM_EVENT_APPLIANCE_STATUS_MODE());"),
        "case must dispatch the matching event struct:\n{wrapper_c}"
    );

    let _ = fs::remove_dir_all(&out);
}

#[test]
fn without_flag_no_cxx_artifacts_are_emitted() {
    let out = unique_dir("off");
    run_ingot(&[
        "--model",
        "examples/full.toml",
        "--output",
        out.to_str().unwrap(),
    ]);

    for f in [
        "dm_key_events.hpp",
        "dm_key_events_wrapper.hpp",
        "dm_key_events_wrapper.cpp",
    ] {
        assert!(
            !out.join(f).exists(),
            "{f} must not be emitted without --emit-tinyfsm"
        );
    }

    let _ = fs::remove_dir_all(&out);
}

#[test]
fn emit_tinyfsm_with_no_event_keys_emits_nothing() {
    let dir = unique_dir("noevent");
    let model = dir.join("noevent.toml");
    fs::write(
        &model,
        "[meta]\n\
         id = \"plain\"\n\
         version = \"1.0.0\"\n\
         \n\
         [[classes]]\n\
         id = \"status\"\n\
         [[classes.keys]]\n\
         id = \"voltage\"\n\
         type = \"uint16\"\n\
         default = 0\n",
    )
    .unwrap();
    let out = dir.join("gen");
    run_ingot(&[
        "--model",
        model.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--emit-tinyfsm",
    ]);

    assert!(
        !out.join("dm_key_events.hpp").exists(),
        "a model with zero event keys must emit no event header even with the flag on"
    );
    assert!(!out.join("dm_key_events_wrapper.cpp").exists());

    let _ = fs::remove_dir_all(&dir);
}
