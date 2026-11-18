//! Helpers shared by the contract-mode integration tests: the generic model, running ingot,
//! editing a model copy and reading contract.json.
#![allow(dead_code)] // each test crate uses a subset

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// @brief Fresh, uniquely-named temp directory for one test.
/// @version 1
pub fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ingot_contract_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// @brief Path of a version of the generic model, e.g. `v1_0`.
/// @version 1
pub fn model(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/contract")
        .join(version)
}

/// @brief Copy directory `from` into `to`, recursively.
/// @version 1
pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// @brief Run the ingot binary with exactly `args`; returns (success, stderr).
/// @version 1
pub fn run_ingot(args: &[&str]) -> (bool, String) {
    let result = Command::new(env!("CARGO_BIN_EXE_ingot"))
        .args(args)
        .output()
        .expect("spawn ingot");
    (
        result.status.success(),
        String::from_utf8_lossy(&result.stderr).into_owned(),
    )
}

/// @brief Run contract mode on `contract` into `out` (variant sku_a unless `extra` names one);
/// returns (success, stderr).
/// v2: runs through run_ingot.
/// @version 2
pub fn generate(contract: &Path, out: &Path, extra: &[&str]) -> (bool, String) {
    let mut args = vec![
        "--contract",
        contract.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--allow-dirty-ingot",
    ];
    if !extra.contains(&"--variant") {
        args.extend(["--variant", "sku_a"]);
    }
    args.extend_from_slice(extra);
    run_ingot(&args)
}

/// @brief Generate `version` of the generic model into `out`, asserting success.
/// @version 1
pub fn generate_ok(version: &str, out: &Path, extra: &[&str]) {
    let (ok, stderr) = generate(&model(version).join("contract.toml"), out, extra);
    assert!(ok, "{version} must generate:\n{stderr}");
}

/// @brief The parsed contract.json in `gen`.
/// @version 1
pub fn contract_json(gen: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(gen.join("contract.json")).unwrap()).unwrap()
}

/// @brief The contract.json entry of principal `name`.
/// @version 1
pub fn principal(gen: &Path, name: &str) -> serde_json::Value {
    contract_json(gen)["principals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("principal {name}"))
        .clone()
}

/// @brief The digests of principal `name` in a generated contract.json.
/// @version 1
pub fn digests(gen: &Path, name: &str) -> Vec<String> {
    principal(gen, name)["digests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap().to_string())
        .collect()
}

/// @brief Replace `from` with `to` in `file`, asserting `from` occurs exactly once.
/// @version 1
pub fn edit(file: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(file).unwrap();
    assert_eq!(
        text.matches(from).count(),
        1,
        "{}: '{from}' must occur once",
        file.display()
    );
    fs::write(file, text.replacen(from, to, 1)).unwrap();
}

/// @brief Assert `contract` is refused against the contract.json in `previous_out`, with a log
/// containing `expected`.
/// @version 1
pub fn refused_against(contract: &Path, previous_out: &Path, expected: &str) {
    let previous = previous_out.join("contract.json");
    let out = previous_out.with_extension("out");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    let (ok, stderr) = generate(contract, &out, &args);
    assert!(!ok, "'{expected}' must be refused");
    assert!(
        stderr.contains(expected),
        "expected '{expected}' in:\n{stderr}"
    );
}

/// @brief The rights cell of principal `name` on key `path` in a contract.json ("" for none).
/// @version 1
pub fn right(gen: &Path, name: &str, path: &str) -> String {
    contract_json(gen)["rights"][name][path]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// @brief The contract.json entry of the key at `path`.
/// @version 1
pub fn key_json(gen: &Path, path: &str) -> serde_json::Value {
    contract_json(gen)["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["path"] == path)
        .unwrap_or_else(|| panic!("key {path}"))
        .clone()
}

/// @brief Copy `version` of the generic model into `dir` and apply `edits` (file, from, to).
/// @version 1
pub fn edited_copy(version: &str, dir: &Path, edits: &[(&str, &str, &str)]) {
    copy_dir(&model(version), dir);
    for (file, from, to) in edits {
        edit(&dir.join(file), from, to);
    }
}

/// @brief Assert two generations have the same store tables (dmt_types.h, dmt_tables.c, and
/// dmt_tables.h apart from the model hash) and the same digests for every principal.
/// v2: shared by the test crates; dmt_types.h too.
/// @version 2
pub fn assert_same_store_tables(a: &Path, b: &Path) {
    let read = |d: &Path, f: &str| fs::read_to_string(d.join(f)).unwrap();
    assert_eq!(read(a, "dmt_types.h"), read(b, "dmt_types.h"));
    assert_eq!(read(a, "dmt_tables.c"), read(b, "dmt_tables.c"));
    let hash_free = |d: &Path| {
        read(d, "dmt_tables.h")
            .lines()
            .filter(|l| !l.contains("MODEL_HASH"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(hash_free(a), hash_free(b));
    let principals = contract_json(a)["principals"].as_array().unwrap().clone();
    assert!(!principals.is_empty());
    for p in principals {
        let name = p["name"].as_str().unwrap();
        assert_eq!(digests(a, name), digests(b, name), "{name}");
    }
}

/// @brief Run `cc` (or `c++`) with `args`, asserting success.
/// v2: shared by the test crates.
/// @version 2
pub fn compile(cc: &str, args: &[&str]) {
    let out = Command::new(cc)
        .args(args)
        .output()
        .expect("spawn compiler");
    assert!(
        out.status.success(),
        "{cc} {args:?}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// @brief The `DSH_INTERNAL_*` defines of dsh_tables.h in `gen`, as (name, value) in order.
/// @version 1
pub fn internal_defines(gen: &Path) -> Vec<(String, String)> {
    let h = fs::read_to_string(gen.join("dsh_tables.h")).unwrap();
    h.lines()
        .filter(|l| l.starts_with("#define DSH_INTERNAL_"))
        .map(|l| {
            let mut words = l.split_whitespace().skip(1);
            let name = words.next().unwrap().to_string();
            (name, words.next().unwrap().to_string())
        })
        .collect()
}

/// @brief Write `dsh_internal_list.h` into `gen` for check_dsh.c: `DSH_INTERNAL_LIST`, one
/// `X(name)` per `DSH_INTERNAL_*` define of dsh_tables.h, so the walk can check each constant
/// it names against the command rows.
/// @version 1
fn write_internal_list(gen: &Path) {
    let items: String = internal_defines(gen)
        .iter()
        .map(|(name, _)| format!(" X({name})"))
        .collect();
    fs::write(
        gen.join("dsh_internal_list.h"),
        format!("#define DSH_INTERNAL_LIST{items}\n"),
    )
    .unwrap();
}

/// @brief Build check_dsh.c against the shadow tables in `gen` as strict C11 and run it;
/// whether the walk finds the tables consistent. The internal row constants of dsh_tables.h are
/// listed for it (`write_internal_list`).
/// @version 1
pub fn dsh_walk_passes(gen: &Path) -> bool {
    write_internal_list(gen);
    let check = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dsh/check_dsh.c");
    let exe = gen.join("check_dsh");
    let include = format!("-I{}", gen.display());
    let tables = gen.join("dsh_tables.c");
    let strict = [
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-pedantic",
        "-Werror",
        &include,
    ];
    let files = [
        check.to_str().unwrap(),
        tables.to_str().unwrap(),
        "-o",
        exe.to_str().unwrap(),
    ];
    compile("cc", &[&strict[..], &files[..]].concat());
    Command::new(&exe).status().unwrap().success()
}

/// @brief Walk the shadow tables in `gen` in C (`dsh_walk_passes`), asserting they are
/// consistent; and include the header from C++.
/// v2: shared by the test crates (was compile_and_walk in the shadow table tests).
/// v3: the walk is `dsh_walk_passes`.
/// @version 3
pub fn walk_dsh_tables(gen: &Path) {
    assert!(dsh_walk_passes(gen), "check_dsh");
    let include = format!("-I{}", gen.display());
    let cpp = gen.join("tu.cpp");
    fs::write(
        &cpp,
        "#include \"dsh_tables.h\"\nint main() { return dsh_if_major_1 != 1u; }\n",
    )
    .unwrap();
    compile(
        "c++",
        &[
            "-std=c++11",
            "-Wall",
            "-Werror",
            "-fsyntax-only",
            &include,
            cpp.to_str().unwrap(),
        ],
    );
}
