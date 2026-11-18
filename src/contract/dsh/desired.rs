//! The binder's tables: `dsh_desired` (one row per desired path of a SETTINGS key, sorted by
//! path), its discrete sets and coupled writes, and `dsh_echo` (echo-only paths, sorted by path).

use super::super::keys::Key;
use super::super::model::Tables;
use super::super::schema::{EchoType, OnReject, PathForm, Role};
use super::super::value::encode_default;
use super::enums::inbound;
use super::{id, index16, text, ty, Pools};

/// The `DSH_FORM_*` of key `key` at path `f`, and the enum the form reads (a bool at an integer
/// path is a reported 0 or 1; the refusals keep it off desired paths).
///
/// v2: a bool at an integer path.
/// @version 2
/// @req REQ-ING-DSH-001
/// @req REQ-ING-SHD-010
pub fn form(key: &Key, f: &PathForm, pools: &mut Pools) -> (&'static str, Option<u16>) {
    let index = match (&key.enum_name, &key.enum_values) {
        (Some(n), Some(e)) => Some(pools.enums.index(n, e)),
        _ => None,
    };
    let form = match index {
        None if f.integer() => "DSH_FORM_BOOL_INTEGER",
        None => "DSH_FORM_PLAIN",
        Some(_) if f.integer() => "DSH_FORM_INTEGER",
        Some(_) => "DSH_FORM_ENUM",
    };
    (form, index)
}

/// `DSH_IN_*` and the code of an unknown inbound string, on a wire-string path.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn inbound_rule(key: &Key, form: &str) -> (&'static str, i64) {
    let rule = key
        .wire
        .inbound_unknown
        .as_deref()
        .filter(|_| form == "DSH_FORM_ENUM");
    let code = |s: &str| {
        key.enum_values
            .as_ref()
            .and_then(|e| inbound(e).into_iter().find(|(t, _)| t == s))
            .map_or(0, |(_, c)| c)
    };
    match rule {
        None => ("DSH_IN_NONE", 0),
        Some("ignore") => ("DSH_IN_IGNORE", 0),
        Some("reject") => ("DSH_IN_REJECT", 0),
        Some(s) => ("DSH_IN_CODE", code(s)),
    }
}

/// `.min`, `.max`, `.step` and `.unset` of the key, and its `DSH_DF_*` flags.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn limits(key: &Key) -> (String, String) {
    let (min, max, step) = key.range.map_or((0, 0, 0), |r| (r.min, r.max, r.step));
    let mut set = Vec::new();
    if key.range.is_some() {
        set.push("DSH_DF_RANGE");
    }
    if key.unset.is_some() {
        set.push("DSH_DF_UNSET");
    }
    let flags = super::flags(&set);
    let unset = key.unset.unwrap_or(0);
    (
        format!(".min = {min}, .max = {max}, .step = {step}, .unset = {unset}"),
        flags,
    )
}

/// `.allowed_first` and `.allowed_count` of the key's discrete set, appended to the pool.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn allowed(key: &Key, pools: &mut Pools) -> String {
    let set = key.allowed.as_deref().unwrap_or_default();
    let first = pools.allowed.len();
    pools.allowed.extend_from_slice(set);
    format!(".allowed_first = {first}u, .allowed_count = {}u", set.len())
}

/// `.implies_first` and `.implies_count` of the key's coupled writes, appended to the pool.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn implies(tables: &Tables, key: &Key, pools: &mut Pools) -> String {
    let first = pools.implies.len();
    for imp in &key.wire.implies {
        let Some(t) = tables.key_index(&imp.key).map(|i| &tables.keys[i]) else {
            continue;
        };
        let bytes = encode_default(t.ty, &imp.value, t.max_size, t.enum_values.as_ref());
        let value = pools.value(&bytes.unwrap_or_default());
        pools.implies.push(format!(
            "{{ .value = {value}, .key = {}, .type = {} }}",
            id(t.id),
            ty(t.ty)
        ));
    }
    let count = pools.implies.len() - first;
    format!(".implies_first = {first}u, .implies_count = {count}u")
}

/// The apply group's index, or `DSH_NONE_U8`.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn group(tables: &Tables, key: &Key) -> String {
    let groups = &tables.shadow.apply_groups;
    key.wire
        .apply_group
        .as_ref()
        .and_then(|g| groups.iter().position(|d| &d.id == g))
        .map_or("DSH_NONE_U8".to_string(), |i| format!("{i}u"))
}

/// One `dsh_desired` row of SETTINGS key `key` at `f`.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn row(tables: &Tables, key: &Key, f: &PathForm, pools: &mut Pools) -> String {
    let (form, enum_index) = form(key, f, pools);
    let (rule, code) = inbound_rule(key, form);
    let (limits, flags) = limits(key);
    let reject = match key.wire.on_reject {
        Some(OnReject::ClearDesired) => "DSH_REJ_CLEAR_DESIRED",
        _ => "DSH_REJ_ECHO",
    };
    format!(
        "{{ .path = {}, {limits}, .inbound_unknown_code = {code}, .key = {}, .max_size = {}u, \
         .enum_index = {}, {}, {}, .type = {}, .form = {form}, .apply_group = {}, \
         .on_reject = {reject}, .inbound_unknown = {rule}, .flags = {flags} }}",
        text(Some(&f.path)),
        id(key.id),
        key.max_size,
        index16(enum_index),
        allowed(key, pools),
        implies(tables, key, pools),
        ty(key.ty),
        group(tables, key),
    )
}

/// Every desired path form of every SETTINGS key, in key order: one `dsh_desired` row each.
///
/// @version 1
/// @req REQ-ING-DSH-001
/// @req REQ-ING-DSH-006
pub fn forms(tables: &Tables) -> impl Iterator<Item = (&Key, &PathForm)> {
    tables
        .keys
        .iter()
        .filter(|k| k.role == Role::Settings)
        .flat_map(|k| k.wire.shadow_forms.iter().map(move |f| (k, f)))
}

/// Every desired path of every SETTINGS key, sorted by path.
///
/// v2: the forms from `forms`, shared with the consumer bounds.
/// @version 2
/// @req REQ-ING-DSH-001
pub fn rows(tables: &Tables, pools: &mut Pools) -> Vec<String> {
    let mut rows: Vec<(String, String)> = forms(tables)
        .map(|(key, f)| (f.path.clone(), row(tables, key, f, pools)))
        .collect();
    rows.sort();
    rows.into_iter().map(|(_, r)| r).collect()
}

/// The `DSH_ECHO_*` of an echo type.
///
/// @version 1
/// @req REQ-ING-DSH-001
fn echo_type(t: EchoType) -> &'static str {
    match t {
        EchoType::Bool => "DSH_ECHO_BOOL",
        EchoType::Int => "DSH_ECHO_INT",
        EchoType::Float => "DSH_ECHO_FLOAT",
        EchoType::String => "DSH_ECHO_STRING",
    }
}

/// The echo-only paths sorted by path, in the order `dsh_echo` holds them.
///
/// @version 1
/// @req REQ-ING-DSH-001
pub fn echo_order(tables: &Tables) -> Vec<&super::super::schema::Echo> {
    let mut list: Vec<_> = tables.shadow.echo.iter().collect();
    list.sort_by(|a, b| a.path.cmp(&b.path));
    list
}

/// The `dsh_echo` rows: each echo-only path with the key command bodies apply it to.
///
/// @version 1
/// @req REQ-ING-DSH-001
pub fn echo_rows(tables: &Tables) -> Vec<String> {
    echo_order(tables)
        .into_iter()
        .map(|e| {
            let key = e
                .command_key
                .as_deref()
                .and_then(|k| tables.key_index(k))
                .map(|i| &tables.keys[i]);
            format!(
                "{{ .path = {}, .command_key = {}, .max_size = {}u, .type = {}, .command_key_type = {} }}",
                text(Some(&e.path)),
                id(key.map_or(0, |k| k.id)),
                e.max_size.unwrap_or(0),
                echo_type(e.ty),
                key.map_or("0u".to_string(), |k| ty(k.ty))
            )
        })
        .collect()
}
