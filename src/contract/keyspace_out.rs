//! Write the keyspace files: the all-variants `keyspace.json`, this variant's resolved
//! `keyspace.<variant>.json`, its deterministic gzip body and its head file, and the schema.

use super::keys::Errors;
use super::keyspace;
use super::model::{self, Tables};
use crate::codegen::{render_template, write_generated};
use flate2::{write::GzEncoder, Compression, GzBuilder};
use std::io::Write;
use std::path::Path;
use tera::{Context, Tera};

/// The base tables and every declared variant's tables, for the all-variants file. A refusal
/// in any variant is a refusal of this run, named with the variant.
///
/// @version 1
/// @req REQ-ING-KSP-001
pub fn build_all(
    contract: &Path,
    declared: &[String],
    errors: &mut Errors,
) -> Result<(Tables, Vec<Tables>), String> {
    let (base, base_errors) = model::build(contract, None)?;
    errors.extend(base_errors.into_iter().map(|e| format!("base: {e}")));
    let mut variants = Vec::new();
    for v in declared {
        let (tables, v_errors) = model::build(contract, Some(v))?;
        errors.extend(v_errors.into_iter().map(|e| format!("variant {v}: {e}")));
        variants.push(tables);
    }
    Ok((base, variants))
}

/// Largest base64 body one MQTT publish carries (AWS IoT Core: 128 KB per publish payload).
const MQTT_PAYLOAD_BYTES: usize = 128 * 1024;

/// The resolved body and its gzip; refused when one MQTT message cannot carry it as base64.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn body(tables: &Tables, commit: &str) -> Result<(String, Vec<u8>), Box<dyn std::error::Error>> {
    let body = serde_json::to_string(&keyspace::resolved(tables, commit))?;
    let gz = gzip(body.as_bytes())?;
    if gz.len().div_ceil(3) * 4 > MQTT_PAYLOAD_BYTES {
        let len = gz.len();
        return Err(format!(
            "keyspace.{}: the gzip body is {len} bytes, over one {MQTT_PAYLOAD_BYTES}-byte MQTT message as base64",
            sku(tables)
        )
        .into());
    }
    Ok((body, gz))
}

/// Write the keyspace files into `out`; refuse a body that one MQTT message cannot carry.
/// Returns the resolved body's hash (the head's `hash`).
///
/// v2: returns the body's hash, for the image constants of the shadow tables.
/// @version 2
/// @req REQ-ING-KSP-001
/// @req REQ-ING-DSH-005
pub fn write(
    tera: &Tera,
    tables: &Tables,
    all: &(Tables, Vec<Tables>),
    commit: &str,
    out: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let (body, gz) = body(tables, commit)?;
    std::fs::create_dir_all(out)?;
    let combined = keyspace::combined(&all.0, &all.1, commit);
    write_generated(
        out.join("keyspace.json"),
        serde_json::to_string_pretty(&combined)?,
    )?;
    let hash = write_resolved(tables, &body, &gz, out)?;
    render_template(tera, "keyspace.schema.json", &Context::new(), out)?;
    Ok(hash)
}

/// Write the variant's resolved body, its gzip and its head file; returns the head's hash.
///
/// v2: returns the hash.
/// @version 2
/// @req REQ-ING-KSP-001
/// @req REQ-ING-DSH-005
fn write_resolved(tables: &Tables, body: &str, gz: &[u8], out: &Path) -> std::io::Result<String> {
    let stem = format!("keyspace.{}", sku(tables));
    let head = keyspace::head(tables, body.as_bytes(), gz.len());
    std::fs::write(out.join(format!("{stem}.json.gz")), gz)?;
    std::fs::write(out.join(format!("{stem}.json")), body)?;
    write_generated(out.join(format!("{stem}.head.json")), head.to_string())?;
    Ok(head["hash"].as_str().unwrap_or_default().to_string())
}

/// The variant's file-name part: its name, or `base` for a run without one.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn sku(tables: &Tables) -> &str {
    if tables.variant.is_empty() {
        "base"
    } else {
        &tables.variant
    }
}

/// Deterministic gzip of `bytes`: best compression, no file name, mtime 0.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut enc: GzEncoder<Vec<u8>> = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), Compression::best());
    enc.write_all(bytes)?;
    enc.finish()
}
