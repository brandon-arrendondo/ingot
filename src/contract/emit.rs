//! Render `dmt_tables.h` and `dmt_tables.c` from resolved tables.

use super::bounds::queue_bound;
use super::digest::digests;
use super::model::Tables;
use super::rights::pack;
use super::schema::Kind;
use super::types::OWNER_CORE_INDEX;
use crate::codegen::{join_list, render_template};
use std::path::Path;
use tera::{Context, Tera};

/// `0x%08XU`, the form every ID takes in the tables.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn id(value: u32) -> String {
    format!("{value:#010X}U")
}

/// A C initialiser `{ a, b, ... }` of byte values.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn bytes(values: &[u8]) -> String {
    let list: Vec<String> = values.iter().map(|b| format!("{b:#04x}U")).collect();
    format!("{{ {} }}", list.join(", "))
}

/// A C string literal, or `NULL`.
///
/// v2: shared with the shadow tables.
/// @version 2
/// @req REQ-ING-TIV-009
/// @req REQ-ING-DSH-001
pub(super) fn c_string(text: Option<&str>) -> String {
    text.map_or("NULL".to_string(), |t| {
        format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\""))
    })
}

/// The `dmt_keys` rows and the default blob.
///
/// v2: the interval comes from Key::effective_deferred_ms, shared with contract.json.
/// @version 2
/// @req REQ-ING-TIV-003
/// @req REQ-ING-TIV-004
fn key_rows(tables: &Tables) -> (Vec<String>, Vec<u8>) {
    let mut blob = Vec::new();
    let rows = tables
        .keys
        .iter()
        .map(|k| {
            let offset = blob.len() as u32;
            blob.extend(&k.default);
            let deferred = k.effective_deferred_ms();
            let alias = tables.id_of(k.alias_of.as_deref());
            format!(
                "{{ {}, {}u, {}u, {}u, {deferred}u, {}, {offset}u, {}u, {:#06x}u, {}u, {}u, {}u, {}u, {:#04x}u }}",
                id(k.id), k.max_size, k.rate_per_day, k.rate_burst, id(alias), k.default.len(),
                k.flags(), k.introduced.minor_in(tables.version.major), k.ty.code(),
                k.persist.code(), k.role.code(), k.xport()
            )
        })
        .collect();
    (rows, blob)
}

/// The `dmt_principals` rows.
///
/// @version 1
/// @req REQ-ING-TIV-006
/// @req REQ-ING-TIV-007
fn principal_rows(tables: &Tables) -> Vec<String> {
    (0..tables.principals.len())
        .map(|i| {
            let p = &tables.principals[i];
            format!(
                "{{ {}u, {}u, {}u, {:#04x}u, {}, {}u, {}u, {}u, {}u, {}u, {}u, {}u, {}u, {}u, {}u, {}u, 0u }}",
                p.uid, p.token, p.kind.code(), p.flags, id(tables.id_of(p.heartbeat.as_deref())),
                p.heartbeat_window_ms, queue_bound(tables, i), p.latency_budget_ms,
                p.caps.inflight_fd, p.caps.inflight_bytes, p.caps.open_fds, p.caps.trace_share_pct,
                p.resync_k, p.resync_t_ms, p.staging_bound_bytes,
                p.introduced.minor_in(tables.version.major)
            )
        })
        .collect()
}

/// `dmt_principal_names`: every principal's declared name, in table order.
///
/// v2: every principal (sub-consumers, UART and in-process ones too), never `NULL`.
/// @version 2
/// @req REQ-ING-TIV-006
fn principal_names(tables: &Tables) -> Vec<String> {
    tables
        .principals
        .iter()
        .map(|p| c_string(Some(p.name.as_str())))
        .collect()
}

/// The `dmt_commands` rows.
///
/// v2: a core-answered command's owner is `DMT_OWNER_CORE`.
/// @version 2
/// @req REQ-ING-TIV-005
fn command_rows(tables: &Tables) -> Vec<String> {
    tables
        .commands
        .iter()
        .map(|c| {
            let owner = match tables.principal_index(&c.owner) {
                Some(p) => p as u16,
                None => OWNER_CORE_INDEX,
            };
            format!(
                "{{ {}, {}, {}, {}u, {}u, {}u, {}u, {owner}u, {}u, 0u }}",
                id(c.keys[0]),
                id(c.keys[1]),
                id(c.keys[2]),
                c.max_payload,
                c.max_result,
                c.max_edits,
                c.dedupe_ring,
                c.persist.code()
            )
        })
        .collect()
}

/// The `dmt_uid_caps` rows (one zero row when there are none, so the array is never empty).
///
/// @version 1
/// @req REQ-ING-TIV-006
fn uid_cap_rows(tables: &Tables) -> Vec<String> {
    let rows: Vec<String> = tables
        .uid_caps
        .iter()
        .map(|c| {
            format!(
                "{{ {}u, {}u, {}u, {}u }}",
                c.uid, c.inflight_fd, c.inflight_bytes, c.open_fds
            )
        })
        .collect();
    if rows.is_empty() {
        vec!["{ 0u, 0u, 0u, 0u }".to_string()]
    } else {
        rows
    }
}

/// The rendering context of the two table files.
///
/// @version 1
/// @req REQ-ING-TIV-009
pub fn context(tables: &Tables, commit: &str) -> Context {
    let mut ctx = Context::new();
    insert_header(&mut ctx, tables, commit);
    insert_keys(&mut ctx, tables);
    insert_principals(&mut ctx, tables);
    insert_bounds(&mut ctx, tables);
    ctx
}

/// Contract version, variant and provenance.
///
/// @version 1
/// @req REQ-ING-TIV-009
/// @req REQ-ING-TIV-013
fn insert_header(ctx: &mut Context, tables: &Tables, commit: &str) {
    ctx.insert("name", &c_string(Some(&tables.name)));
    ctx.insert("major", &tables.version.major);
    ctx.insert("minor", &tables.version.minor);
    ctx.insert("patch", &tables.patch);
    ctx.insert("variant", &c_string(Some(&tables.variant)));
    ctx.insert("model_hash", &tables.model_hash);
    ctx.insert("ingot_version", env!("CARGO_PKG_VERSION"));
    ctx.insert("ingot_commit", commit);
}

/// The default blob's initialiser; one zero byte when empty, so the array is never empty.
///
/// @version 1
/// @req REQ-ING-TIV-004
fn blob_initialiser(blob: &[u8]) -> String {
    if blob.is_empty() {
        "{ 0U }".to_string()
    } else {
        bytes(blob)
    }
}

/// Keys, defaults, reserved IDs, commands and well-known roles.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn insert_keys(ctx: &mut Context, tables: &Tables) {
    let (keys, blob) = key_rows(tables);
    let reserved: Vec<String> = tables.reserved.iter().map(|r| id(*r)).collect();
    ctx.insert("keys", &keys);
    ctx.insert("blob_len", &blob.len());
    ctx.insert("blob", &blob_initialiser(&blob));
    ctx.insert("reserved", &join_list(&reserved));
    ctx.insert("num_reserved", &tables.reserved.len());
    ctx.insert("commands", &command_rows(tables));
    ctx.insert("wk", &join_list(&tables.wk().map(id)));
}

/// Principals, their names, caps, rights and digests.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn insert_principals(ctx: &mut Context, tables: &Tables) {
    let rights: Vec<String> = tables.rights.iter().map(|r| bytes(&pack(r))).collect();
    ctx.insert("principals", &principal_rows(tables));
    ctx.insert("principal_names", &principal_names(tables));
    ctx.insert("uid_caps", &uid_cap_rows(tables));
    ctx.insert("num_uid_caps", &tables.uid_caps.len());
    ctx.insert("rights", &rights);
    ctx.insert("digests", &digest_rows(tables));
}

/// Staging and in-process budgets (in-process ones are 0 without an INPROC principal).
///
/// @version 1
/// @req REQ-ING-TIV-007
fn insert_bounds(ctx: &mut Context, tables: &Tables) {
    let inproc = tables.principals.iter().any(|p| p.kind == Kind::Inproc);
    let only_inproc = |bytes: u32| if inproc { bytes } else { 0 };
    ctx.insert("staging_total", &tables.bounds.staging_total_bytes);
    ctx.insert(
        "inproc_queue",
        &only_inproc(tables.bounds.inproc_queue_bytes),
    );
    ctx.insert(
        "inproc_snapshot",
        &only_inproc(tables.bounds.inproc_snapshot_bytes),
    );
}

/// `dmt_digest` rows: per principal, one 32-byte initialiser per minor.
///
/// @version 1
/// @req REQ-ING-TIV-008
fn digest_rows(tables: &Tables) -> Vec<String> {
    digests(tables)
        .iter()
        .map(|minors| {
            let rows: Vec<String> = minors.iter().map(|d| bytes(d)).collect();
            format!("{{ {} }}", rows.join(",\n      "))
        })
        .collect()
}

/// Write `dmt_tables.h` and `dmt_tables.c` into `output_dir`.
///
/// @version 1
/// @req REQ-ING-TIV-009
pub fn write(
    tera: &Tera,
    tables: &Tables,
    commit: &str,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = context(tables, commit);
    render_template(tera, "dmt_tables.h", &ctx, output_dir)?;
    render_template(tera, "dmt_tables.c", &ctx, output_dir)?;
    Ok(())
}
