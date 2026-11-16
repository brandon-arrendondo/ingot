//! The reporter's tables: `dsh_reported` in emission order, and the error groups with their
//! bits.

use super::super::keys::Key;
use super::super::model::Tables;
use super::super::schema::{in_delta, in_full, Constant, Image, PathForm};
use super::desired::{echo_order, form};
use super::{id, index16, text, ty, Pools};
use std::collections::BTreeMap;

/// The fields every reported row of a kind other than a key leaves at their defaults.
const NOT_A_KEY: &str = ".outbound_unknown_wire = NULL, .telem = NULL, .key = 0x00000000U, \
     .applies = 0x00000000U, .enum_index = DSH_NONE_U16";

/// The JSON text of a constant row: its value, or the image's `"M.m.p"` or body hash.
///
/// v2: shared with the topic payload rows.
/// @version 2
/// @req REQ-ING-TOP-003
/// @req REQ-ING-DSH-002
/// @req REQ-ING-DSH-005
pub(super) fn constant_json(tables: &Tables, c: &Constant, hash: &str) -> String {
    let v = &tables.version;
    match (&c.value, c.image) {
        (Some(value), _) => serde_json::to_string(value).unwrap_or_default(),
        (None, Some(Image::ContractVersion)) => {
            format!("\"{}.{}.{}\"", v.major, v.minor, tables.patch)
        }
        (None, Some(Image::ContractHash)) => format!("\"{hash}\""),
        (None, None) => String::new(),
    }
}

/// `.scope` and `.flags` of a row in the full document and/or the delta, forced or not.
///
/// v2: shared with the topic payload rows.
/// @version 2
/// @req REQ-ING-TOP-003
/// @req REQ-ING-DSH-002
pub(super) fn placement(full: bool, delta: bool, forced: bool) -> String {
    let scope = match (full, delta) {
        (true, true) => "DSH_SC_FULL | DSH_SC_DELTA",
        (true, false) => "DSH_SC_FULL",
        _ => "DSH_SC_DELTA",
    };
    let flags = if forced { "DSH_RF_FORCED" } else { "0u" };
    format!(".scope = {scope}, .flags = {flags}")
}

/// A row that is not a key's: its path, kind, constant text, echo index and placement.
///
/// v2: shared with the topic payload rows.
/// @version 2
/// @req REQ-ING-TOP-003
/// @req REQ-ING-DSH-002
pub(super) fn other(
    path: &str,
    kind: &str,
    constant: Option<&str>,
    r: usize,
    placed: &str,
) -> String {
    format!(
        "{{ .path = {}, .constant = {}, {NOT_A_KEY}, .ref = {r}u, .max_items = 0u, \
         .kind = {kind}, .type = 0u, .form = DSH_FORM_PLAIN, {placed}, \
         .outbound_unknown = DSH_OUT_NONE }}",
        text(Some(path)),
        text(constant)
    )
}

/// `.outbound_unknown` and `.outbound_unknown_wire` of a key on a wire-string path.
///
/// @version 1
/// @req REQ-ING-DSH-002
fn outbound_rule<'k>(key: &'k Key, form: &str) -> (&'static str, Option<&'k str>) {
    let rule = key
        .wire
        .outbound_unknown
        .as_deref()
        .filter(|_| form == "DSH_FORM_ENUM");
    match rule {
        None => ("DSH_OUT_NONE", None),
        Some("raw") => ("DSH_OUT_RAW", None),
        Some(w) => ("DSH_OUT_WIRE", Some(w)),
    }
}

/// The form and enum of a STATE key at `f`: a rendered list reads its own enum.
///
/// @version 1
/// @req REQ-ING-DSH-002
fn key_form(key: &Key, f: &PathForm, pools: &mut Pools) -> (&'static str, Option<u16>, u16) {
    match (&key.wire.render, &key.wire.render_values) {
        (Some(r), Some(e)) => {
            let ns = key.path.split('.').next().unwrap_or_default();
            let i = pools.enums.index(&format!("{ns}::{}", r.enum_ref), e);
            ("DSH_FORM_CSV_INDEX_LIST", Some(i), r.max_items)
        }
        _ => {
            let (form, i) = form(key, f, pools);
            (form, i, 0)
        }
    }
}

/// One `dsh_reported` row of STATE key `key` at `f`.
///
/// v2: shared with the topic payload rows.
/// @version 2
/// @req REQ-ING-TOP-003
/// @req REQ-ING-DSH-002
pub(super) fn key_row(tables: &Tables, key: &Key, f: &PathForm, pools: &mut Pools) -> String {
    let (form, enum_index, max_items) = key_form(key, f, pools);
    let (rule, wire) = outbound_rule(key, form);
    let forced = key.wire.forced && f.in_delta();
    format!(
        "{{ .path = {}, .constant = NULL, .outbound_unknown_wire = {}, .telem = {}, .key = {}, \
         .applies = {}, .enum_index = {}, .ref = 0u, .max_items = {max_items}u, \
         .kind = DSH_RK_KEY, .type = {}, .form = {form}, {}, .outbound_unknown = {rule} }}",
        text(Some(&f.path)),
        text(wire),
        text(key.wire.telem.as_deref()),
        id(key.id),
        id(tables.id_of(key.wire.applies.as_deref())),
        index16(enum_index),
        ty(key.ty),
        placement(f.in_full(), f.in_delta(), forced),
    )
}

/// The error-group paths, each once, in group declaration order.
///
/// @version 1
/// @req REQ-ING-DSH-002
fn error_paths(tables: &Tables) -> Vec<&str> {
    let mut paths: Vec<&str> = Vec::new();
    for p in tables
        .error_groups
        .iter()
        .filter_map(|g| g.shadow.as_deref())
    {
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths
}

/// The echo rows with their paths, each in the documents of its scope.
///
/// @version 1
/// @req REQ-ING-DSH-002
/// @req REQ-ING-SHD-007
fn echo_rows(tables: &Tables) -> Vec<(String, String)> {
    echo_order(tables)
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let placed = placement(in_full(e.scope), in_delta(e.scope), false);
            (
                e.path.clone(),
                other(&e.path, "DSH_RK_ECHO", None, i, &placed),
            )
        })
        .collect()
}

/// contract.toml's rows with their paths: constants, echo paths (by their scope), error-group
/// paths, then the rejected-settings list (delta only).
///
/// v2: an echo row's scope (`echo_rows`).
/// v3: the rejected-settings list.
/// @version 3
/// @req REQ-ING-DSH-002
/// @req REQ-ING-SHD-007
/// @req REQ-ING-SHD-011
fn contract_rows(tables: &Tables, hash: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let both = placement(true, true, false);
    for c in &tables.shadow.constants {
        let json = constant_json(tables, c, hash);
        let placed = placement(true, c.forced, c.forced);
        let row = other(&c.path, "DSH_RK_CONSTANT", Some(&json), 0, &placed);
        rows.push((c.path.clone(), row));
    }
    rows.extend(echo_rows(tables));
    for p in error_paths(tables) {
        rows.push((p.to_string(), other(p, "DSH_RK_ERRORS", None, 0, &both)));
    }
    if let Some(r) = &tables.shadow.rejected {
        let delta = placement(false, true, false);
        let row = other(&r.path, "DSH_RK_REJECTED", None, 0, &delta);
        rows.push((r.path.clone(), row));
    }
    rows
}

/// Every reported row with its path, in declaration order: contract.toml's constants, echo
/// paths and error-group paths, then the reported (STATE and CONFIG) keys' paths in key ID
/// order.
///
/// v2: CONFIG keys.
/// @version 2
/// @req REQ-ING-DSH-002
/// @req REQ-ING-SHD-008
fn declared(tables: &Tables, hash: &str, pools: &mut Pools) -> Vec<(String, String)> {
    let mut rows = contract_rows(tables, hash);
    for key in tables.keys.iter().filter(|k| k.role.reported()) {
        for f in &key.wire.shadow_forms {
            rows.push((f.path.clone(), key_row(tables, key, f, pools)));
        }
    }
    rows
}

/// `rows` in emission order: a stable sort on each path prefix's first-appearance rank, so rows
/// sharing an object prefix are contiguous and siblings keep their first-declaration order.
///
/// @version 1
/// @req REQ-ING-DSH-002
pub fn tree_order<T>(rows: Vec<(String, T)>) -> Vec<T> {
    let mut rank: BTreeMap<String, usize> = BTreeMap::new();
    let mut keyed: Vec<(Vec<usize>, T)> = rows
        .into_iter()
        .map(|(path, row)| {
            let mut prefix = String::new();
            let key = path
                .split('.')
                .map(|seg| {
                    prefix.push_str(seg);
                    prefix.push('.');
                    let next = rank.len();
                    *rank.entry(prefix.clone()).or_insert(next)
                })
                .collect();
            (key, row)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, r)| r).collect()
}

/// The `dsh_reported` rows in emission order.
///
/// @version 1
/// @req REQ-ING-DSH-002
pub fn rows(tables: &Tables, hash: &str, pools: &mut Pools) -> Vec<String> {
    tree_order(declared(tables, hash, pools))
}

/// The `dsh_error_groups` and `dsh_error_bits` rows: each group in declaration order, its bits
/// sorted.
///
/// @version 1
/// @req REQ-ING-DSH-004
pub fn error_rows(tables: &Tables) -> (Vec<String>, Vec<String>) {
    let mut groups = Vec::new();
    let mut bits = Vec::new();
    for g in &tables.error_groups {
        let mut mine: Vec<(u8, u32)> = tables
            .keys
            .iter()
            .filter_map(|k| {
                k.error
                    .as_ref()
                    .filter(|e| e.group == g.id)
                    .map(|e| (e.bit, k.id))
            })
            .collect();
        mine.sort();
        let reserved = g.reserved_bits.iter().fold(0u64, |m, b| m | 1u64 << b);
        groups.push(format!(
            "{{ .id = {}, .shadow = {}, .reserved = {reserved:#x}ULL, .wire_base = {}u, \
             .bits_first = {}u, .bits_count = {}u, .width = {}u, .publish_max_bit = {}u }}",
            text(Some(&g.id)),
            text(g.shadow.as_deref()),
            g.wire_base,
            bits.len(),
            mine.len(),
            g.width,
            g.publish_max_bit
        ));
        bits.extend(
            mine.iter()
                .map(|(bit, key)| format!("{{ .key = {}, .bit = {bit}u }}", id(*key))),
        );
    }
    (groups, bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_sharing_a_prefix_become_contiguous_in_first_declaration_order() {
        let rows = [
            "state.a",
            "sv",
            "state.b.x",
            "config.cv",
            "state.b.y",
            "state.c",
            "sv",
        ];
        let rows: Vec<(String, usize)> = rows
            .iter()
            .enumerate()
            .map(|(i, p)| (p.to_string(), i))
            .collect();
        assert_eq!(tree_order(rows), vec![0, 2, 4, 5, 1, 6, 3]);
    }

    #[test]
    fn placement_names_the_documents_and_the_forced_flag() {
        assert_eq!(
            placement(true, true, true),
            ".scope = DSH_SC_FULL | DSH_SC_DELTA, .flags = DSH_RF_FORCED"
        );
        assert_eq!(
            placement(true, false, false),
            ".scope = DSH_SC_FULL, .flags = 0u"
        );
        assert_eq!(
            placement(false, true, false),
            ".scope = DSH_SC_DELTA, .flags = 0u"
        );
    }
}
