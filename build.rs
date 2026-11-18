//! Embed the git commit ingot is built from as `INGOT_GIT_COMMIT`, suffixed `-dirty` when
//! tracked files differ from it, or `unknown` outside a git checkout. Contract mode stamps it
//! into the tables and refuses a dirty or unknown build; `ingot --version` prints it.

use std::process::Command;

/// Output of `git <args>`, trimmed, or `None` when git is missing or fails.
///
/// @version 1
/// @req REQ-ING-TIV-013
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The git files whose change moves HEAD or the index. In a submodule or a worktree `.git` is
/// a file, so the real directories come from git: HEAD and the index live in the git dir,
/// branch refs in the common dir.
///
/// @version 1
/// @req REQ-ING-TIV-013
fn git_inputs() -> Vec<String> {
    let dir = git(&["rev-parse", "--absolute-git-dir"]);
    let common = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let mut inputs = Vec::new();
    if let Some(dir) = dir {
        inputs.push(format!("{dir}/HEAD"));
        inputs.push(format!("{dir}/index"));
    }
    if let Some(common) = common {
        inputs.push(format!("{common}/refs"));
        inputs.push(format!("{common}/packed-refs"));
    }
    inputs
}

/// Emit the commit and the paths whose change must re-run this script.
///
/// v2: the git inputs are resolved by git (a submodule's or worktree's `.git` is a file, so
/// `.git/HEAD` never existed there and every build re-ran this script and rebuilt ingot).
/// @version 2
/// @req REQ-ING-TIV-013
fn main() {
    for path in git_inputs()
        .into_iter()
        .chain(["src", "templates", "Cargo.toml"].map(String::from))
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let commit = match git(&["rev-parse", "HEAD"]) {
        Some(head) => {
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
                .is_none_or(|status| !status.is_empty());
            if dirty {
                format!("{head}-dirty")
            } else {
                head
            }
        }
        None => "unknown".to_string(),
    };
    println!("cargo:rustc-env=INGOT_GIT_COMMIT={commit}");
}
