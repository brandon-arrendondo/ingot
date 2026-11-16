//! Contract mode: generate the table-interface (TIV 1.0) store tables of a keyed model.
//!
//! The input is `contract.toml` plus its namespace files (docs/contract-mode.rst); the outputs
//! are `dmt_types.h`, `dmt_tables.h`, `dmt_tables.c`, `contract.json` and, against a previous
//! contract, `release-review.txt`. Every refusal is an error, and nothing is written then.

pub mod bounds;
pub mod constants;
pub mod digest;
pub mod dsh;
pub mod emit;
pub mod enums;
pub mod error_groups;
pub mod header;
pub mod internal_ids;
pub mod keys;
pub mod keyspace;
pub mod keyspace_out;
pub mod model;
pub mod payload;
pub mod previous;
pub mod previous_shadow;
pub mod ranges;
pub mod refuse_commands;
pub mod refuse_model;
pub mod refuse_rights;
pub mod refuse_shadow;
pub mod rights;
pub mod schema;
pub mod schema_shadow;
pub mod sdk;
pub mod shadow;
pub mod shadow_size;
pub mod snapshot;
pub mod topic_vars;
pub mod topics;
pub mod types;
pub mod value;
pub mod variants;
pub mod wire;

use crate::codegen::{tables::write_table_types, write_generated};
use std::path::Path;
use tera::Tera;

/// The ingot commit this binary was built from (`-dirty` with uncommitted changes).
const COMMIT: &str = env!("INGOT_GIT_COMMIT");

/// Inputs of one contract-mode run.
pub struct Options<'a> {
    pub contract: &'a Path,
    pub variant: Option<&'a str>,
    pub previous: Option<&'a Path>,
    pub output: &'a Path,
    pub allow_dirty: bool,
    /// The shadow composer's buffer in bytes, when given (a larger full document is warned about).
    pub shadow_buffer: Option<u32>,
    /// The consumer's SDK runtime binding (a template directory), when SDKs are wanted.
    pub sdk_binding: Option<&'a Path>,
}

/// The commit to stamp into the outputs; a dirty or unknown build is refused unless allowed.
///
/// @version 1
/// @req REQ-ING-TIV-013
pub fn provenance(commit: &str, allow_dirty: bool) -> Result<String, String> {
    let clean = !commit.ends_with("-dirty") && commit != "unknown";
    if clean || allow_dirty {
        Ok(commit.to_string())
    } else {
        Err(format!(
            "ingot was built from {commit}; contract mode needs a clean commit (or --allow-dirty-ingot)"
        ))
    }
}

/// Generate the contract-mode outputs, or fail with every refusal logged.
///
/// v2: `--variant` checked against the declared variants; every variant (and the base) is
/// resolved for the all-variants keyspace.
/// v3: error groups checked.
/// v4: the keyspace body's hash goes to the shadow tables.
/// @version 4
/// @req REQ-ING-ERR-001
/// @req REQ-ING-TIV-009
/// @req REQ-ING-TIV-010
/// @req REQ-ING-TIV-011
/// @req REQ-ING-TIV-012
/// @req REQ-ING-KSP-001
/// @req REQ-ING-DSH-005
pub fn generate(opts: &Options, tera: &Tera) -> Result<(), Box<dyn std::error::Error>> {
    let commit = provenance(COMMIT, opts.allow_dirty)?;
    let (tables, mut errors) = model::build(opts.contract, opts.variant)?;
    variants::check_selected(&tables.variants, opts.variant, &mut errors);
    let all = keyspace_out::build_all(opts.contract, &tables.variants, &mut errors)?;
    refuse_model::check(&tables, &mut errors);
    refuse_rights::check(&tables, &mut errors);
    bounds::check(&tables, &mut errors);
    error_groups::check(&tables, &mut errors);
    let snapshot = snapshot::snapshot(&tables, &commit);
    let review = match opts.previous {
        Some(path) => Some(previous::compare(
            &read_snapshot(path)?,
            &snapshot,
            &mut errors,
        )),
        None => None,
    };
    refuse(&errors)?;
    let hash = keyspace_out::write(tera, &tables, &all, &commit, opts.output)?;
    write_outputs(opts, tera, &tables, (&snapshot, &hash), review.as_deref())
}

/// Write every contract-mode output for accepted tables; `hash` is the keyspace body's.
///
/// v2: the shadow tables.
/// v3: the per-principal SDKs.
/// v4: only through a given binding.
/// v5: the enum constants header.
/// @version 5
/// @req REQ-ING-TIV-017
/// @req REQ-ING-SDK-004
/// @req REQ-ING-TIV-009
/// @req REQ-ING-TIV-012
/// @req REQ-ING-DSH-001
/// @req REQ-ING-SDK-001
fn write_outputs(
    opts: &Options,
    tera: &Tera,
    tables: &model::Tables,
    (snapshot, hash): (&snapshot::Snapshot, &str),
    review: Option<&[String]>,
) -> Result<(), Box<dyn std::error::Error>> {
    let commit = &snapshot.ingot.commit;
    write_table_types(tera, opts.output)?;
    emit::write(tera, tables, commit, opts.output)?;
    constants::write(tera, tables, commit, opts.output)?;
    dsh::write(
        tera,
        tables,
        (commit, hash),
        opts.shadow_buffer,
        opts.output,
    )?;
    if let Some(binding) = opts.sdk_binding {
        sdk::write(binding, tables, commit, opts.output)?;
    }
    let json = serde_json::to_string_pretty(snapshot)?;
    write_generated(opts.output.join("contract.json"), json)?;
    if let Some(lines) = review {
        write_generated(opts.output.join("release-review.txt"), review_text(lines))?;
    }
    log::info!(
        "Contract {} {}.{}: {} keys, {} principals",
        tables.name,
        tables.version.major,
        tables.version.minor,
        tables.keys.len(),
        tables.principals.len()
    );
    Ok(())
}

/// Read a previous release's contract.json.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn read_snapshot(path: &Path) -> Result<snapshot::Snapshot, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Log every refusal and fail when there is any.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn refuse(errors: &[String]) -> Result<(), String> {
    for e in errors {
        log::error!("{e}");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} refusal(s); nothing was generated",
            errors.len()
        ))
    }
}

/// The release-review report.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn review_text(lines: &[String]) -> String {
    if lines.is_empty() {
        "no sensitive grants to new principals".to_string()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_or_unknown_builds_need_the_override() {
        assert_eq!(provenance("abc123", false), Ok("abc123".to_string()));
        assert!(provenance("abc123-dirty", false).is_err());
        assert!(provenance("unknown", false).is_err());
        assert_eq!(
            provenance("abc123-dirty", true),
            Ok("abc123-dirty".to_string())
        );
    }

    #[test]
    fn review_text_says_when_there_is_nothing() {
        assert_eq!(review_text(&[]), "no sensitive grants to new principals");
        assert_eq!(review_text(&["a".into(), "b".into()]), "a\nb");
    }
}
