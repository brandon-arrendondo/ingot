//! The shadow tables `dsh_tables.h` / `dsh_tables.c`: binder, reporter,
//! command-router, error-group and topic rows of one variant, against the fixed `dsh_types.h`.
//! The library's generic walkers read them; nothing in them is per-key code.

mod bounds;
mod commands;
mod desired;
mod enums;
mod payload;
mod reported;
mod topics;

use super::model::Tables;
use super::schema::KeyType;
use super::shadow::declared;
use super::shadow_size::trees;
use crate::codegen::render_template;
use enums::EnumPool;
use serde::Serialize;
use std::path::Path;
use tera::{Context, Tera};

/// One generated array: its C type, name, count define and initialiser rows.
#[derive(Serialize)]
pub struct Table {
    ctype: &'static str,
    name: &'static str,
    count: String,
    len: usize,
    rows: Vec<String>,
}

impl Table {
    /// The table `<ns>_<name>` of `rows`, counted by `<NS>_NUM_<NAME>`; with no rows it holds
    /// `zero`, one zero row, and its count stays 0.
    ///
    /// @version 1
    /// @req REQ-ING-DSH-001
    fn new(ctype: &'static str, name: &'static str, rows: Vec<String>, zero: &str) -> Table {
        let len = rows.len();
        let rows = if rows.is_empty() {
            vec![zero.to_string()]
        } else {
            rows
        };
        let (ns, rest) = name.split_once('_').unwrap_or_default();
        Table {
            ctype,
            name,
            count: format!("{}_NUM_{}", ns.to_uppercase(), rest.to_uppercase()),
            len,
            rows,
        }
    }

    /// `dsh_value_blob`: sixteen bytes to a row, counted in bytes by `DSH_VALUE_BLOB_LEN`.
    ///
    /// @version 1
    /// @req REQ-ING-DSH-001
    fn blob(blob: &[u8]) -> Table {
        let mut t = Table::new("uint8_t", "dsh_value_blob", blob_rows(blob), "0U");
        t.count = "DSH_VALUE_BLOB_LEN".to_string();
        t.len = blob.len();
        t
    }
}

/// The rows shared by several tables, filled while the tables are built: enums, constant value
/// bytes, discrete sets, coupled writes, command steps and their arguments, topic vars.
#[derive(Default)]
pub struct Pools {
    enums: EnumPool,
    blob: Vec<u8>,
    allowed: Vec<i64>,
    implies: Vec<String>,
    steps: Vec<String>,
    args: Vec<String>,
    vars: Vec<String>,
}

impl Pools {
    /// `{ off, len }` of `bytes`, appended to the value blob.
    ///
    /// @version 1
    /// @req REQ-ING-DSH-001
    fn value(&mut self, bytes: &[u8]) -> String {
        let off = self.blob.len();
        self.blob.extend_from_slice(bytes);
        format!("{{ {off}u, {}u }}", bytes.len())
    }
}

/// `0x%08XU`, the form every key ID takes in the tables.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn id(value: u32) -> String {
    format!("{value:#010X}U")
}

/// A C string literal, or `NULL`.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn text(t: Option<&str>) -> String {
    super::emit::c_string(t)
}

/// The `DMT_T_*` code of a key type.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn ty(t: KeyType) -> String {
    format!("{}u", t.code())
}

/// A 16-bit table index, or `DSH_NONE_U16`.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn index16(i: Option<u16>) -> String {
    i.map_or("DSH_NONE_U16".to_string(), |i| format!("{i}u"))
}

/// A flags initialiser: the named flags or'ed together, or `0u`.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn flags(set: &[&str]) -> String {
    if set.is_empty() {
        "0u".to_string()
    } else {
        set.join(" | ")
    }
}

/// The value blob's rows, sixteen bytes to a row.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn blob_rows(blob: &[u8]) -> Vec<String> {
    blob.chunks(16)
        .map(|c| {
            let bytes: Vec<String> = c.iter().map(|b| format!("{b:#04x}U")).collect();
            bytes.join(", ")
        })
        .collect()
}

/// The `dsh_apply_groups` rows: each group's id.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn group_names(tables: &Tables) -> Vec<String> {
    let groups = &tables.shadow.apply_groups;
    groups.iter().map(|g| text(Some(&g.id))).collect()
}

/// Every table of the variant, in the order the header declares them.
///
/// v2: the topic payloads.
/// v3: the topic vars (`dtp_vars`, appended).
/// v4: the declared placeholders (`dtp_placeholders`, appended); group names in `group_names`.
/// v5: the zero row of a table whose first field is a struct is designated, so a variant without
/// coupled writes or commands builds with -Wextra -Werror.
/// @version 5
/// @req REQ-ING-DSH-001
/// @req REQ-ING-DSH-002
/// @req REQ-ING-DSH-003
/// @req REQ-ING-TOP-002
/// @req REQ-ING-TOP-003
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-007
fn tables_of(tables: &Tables, hash: &str) -> Vec<Table> {
    let mut pools = Pools::default();
    let (payload, slices) = payload::rows(tables, hash, &mut pools);
    let topics = topics::rows(tables, &slices, &mut pools);
    let desired = desired::rows(tables, &mut pools);
    let echo = desired::echo_rows(tables);
    let reported = reported::rows(tables, hash, &mut pools);
    let (groups, bits) = reported::error_rows(tables);
    let commands = commands::rows(tables, &mut pools);
    let groups_list = group_names(tables);
    let allowed: Vec<String> = pools.allowed.iter().map(|v| v.to_string()).collect();
    let (enums, outbound, inbound) = pools.enums.rows();
    let placeholders = tables
        .shadow
        .placeholders
        .iter()
        .map(topics::var_row)
        .collect();
    vec![
        Table::blob(&pools.blob),
        Table::new("dsh_enum_t", "dsh_enums", enums, "{ 0 }"),
        Table::new("dsh_outbound_t", "dsh_outbound", outbound, "{ 0 }"),
        Table::new("dsh_inbound_t", "dsh_inbound", inbound, "{ 0 }"),
        Table::new("char *const", "dsh_apply_groups", groups_list, "NULL"),
        Table::new("int64_t", "dsh_allowed", allowed, "0"),
        Table::new(
            "dsh_implies_t",
            "dsh_implies",
            pools.implies,
            "{ .key = 0u }",
        ),
        Table::new("dsh_desired_t", "dsh_desired", desired, "{ 0 }"),
        Table::new("dsh_echo_t", "dsh_echo", echo, "{ 0 }"),
        Table::new("dsh_reported_t", "dsh_reported", reported, "{ 0 }"),
        Table::new("dsh_error_group_t", "dsh_error_groups", groups, "{ 0 }"),
        Table::new("dsh_error_bit_t", "dsh_error_bits", bits, "{ 0 }"),
        Table::new("dsh_arg_t", "dsh_args", pools.args, "{ 0 }"),
        Table::new("dsh_step_t", "dsh_steps", pools.steps, "{ 0 }"),
        Table::new("dsh_command_t", "dsh_commands", commands, "{ .topic = 0u }"),
        Table::new("dtp_topic_t", "dtp_topics", topics, "{ 0 }"),
        Table::new("dsh_reported_t", "dtp_payload", payload, "{ 0 }"),
        Table::new("dtp_var_t", "dtp_vars", pools.vars, "{ 0 }"),
        Table::new("dtp_var_t", "dtp_placeholders", placeholders, "{ 0 }"),
    ]
}

/// The rendering context of the two files.
///
/// v2: the interface version the tables are written against.
/// v3: the consumer bounds (interface minor 8).
/// v4: the internal row constants (interface minor 10).
/// @version 4
/// @req REQ-ING-DSH-001
/// @req REQ-ING-DSH-005
/// @req REQ-ING-DSH-006
/// @req REQ-ING-DSH-008
pub fn context(tables: &Tables, commit: &str, hash: &str) -> Context {
    let t = trees(tables);
    let v = &tables.version;
    let mut ctx = Context::new();
    ctx.insert("interface", crate::codegen::tables::TYPES_HEADERS[1].1);
    ctx.insert("name", &tables.name);
    ctx.insert(
        "version",
        &format!("{}.{}.{}", v.major, v.minor, tables.patch),
    );
    ctx.insert("variant", &tables.variant);
    ctx.insert("ingot_version", env!("CARGO_PKG_VERSION"));
    ctx.insert("ingot_commit", commit);
    ctx.insert("contract_hash", hash);
    ctx.insert("full_max_bytes", &t.reported.bytes());
    ctx.insert("delta_forced_bytes", &t.forced.bytes());
    ctx.insert("bounds", &bounds::of(tables));
    ctx.insert("internal", &super::internal_ids::constants(tables));
    ctx.insert("tables", &tables_of(tables, hash));
    ctx
}

/// Write `dsh_tables.h` and `dsh_tables.c` when the model maps anything to the shadow; warn
/// when the full document can exceed the composer buffer `buffer`.
///
/// @version 1
/// @req REQ-ING-DSH-001
/// @req REQ-ING-DSH-005
pub fn write(
    tera: &Tera,
    tables: &Tables,
    (commit, hash): (&str, &str),
    buffer: Option<u32>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !declared(tables) {
        return Ok(());
    }
    let full = trees(tables).reported.bytes();
    if let Some(b) = buffer.filter(|b| full > *b as usize) {
        log::warn!(
            "the full shadow document is up to {full} bytes, over the {b}-byte composer buffer"
        );
    }
    let ctx = context(tables, commit, hash);
    render_template(tera, "dsh_tables.h", &ctx, out)?;
    render_template(tera, "dsh_tables.c", &ctx, out)?;
    Ok(())
}
