//! The keyspace declaration for tooling: per variant, the SKU-resolved
//! `keyspace.<variant>.json` with its gzip body and head file, and for the whole model the
//! all-variants `keyspace.json` with per-variant overrides. No values and no secret values
//! appear; order is namespace ID, class index, key index.

use super::model::Tables;
use super::rights::{READ, WRITE};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The keyspace format version; a consumer refuses an unknown major.
pub const SCHEMA_VERSION: &str = "1.0";

/// The SKU-resolved declaration of one variant's tables.
///
/// v2: the error groups.
/// v3: the shadow rows and sizes.
/// @version 3
/// @req REQ-ING-KSP-001
/// @req REQ-ING-SHD-002
pub fn resolved(tables: &Tables, commit: &str) -> Value {
    let mut doc = header(tables, commit);
    doc.insert("sku".into(), json!(tables.variant));
    doc.insert("namespaces".into(), namespaces(tables, &|_| json!(true)));
    doc.insert(
        "enums".into(),
        json!(enums(tables).values().collect::<Vec<_>>()),
    );
    doc.insert("commands".into(), commands(tables));
    doc.insert("rights".into(), rights(tables));
    doc.insert(
        "error_groups".into(),
        error_groups(tables, &[], &|_| json!(true)),
    );
    insert_shadow(&mut doc, tables);
    Value::Object(doc)
}

/// The shadow rows and sizes of `tables`, when the model maps anything.
///
/// @version 1
/// @req REQ-ING-SHD-002
fn insert_shadow(doc: &mut Map<String, Value>, tables: &Tables) {
    if let Some(shadow) = super::shadow::json(tables) {
        doc.insert("shadow".into(), shadow);
    }
}

/// The all-variants declaration: the base model plus, per enum, what each variant changes.
///
/// v2: the error groups.
/// v3: the shadow rows, with the base's sizes (an upper bound of every variant's).
/// @version 3
/// @req REQ-ING-KSP-001
/// @req REQ-ING-SHD-002
pub fn combined(base: &Tables, variants: &[Tables], commit: &str) -> Value {
    let mut doc = header(base, commit);
    let present = |path: &str| presence(variants, path);
    doc.insert("namespaces".into(), namespaces(base, &present));
    doc.insert("enums".into(), json!(enum_overrides(base, variants)));
    doc.insert("commands".into(), commands(base));
    doc.insert("rights".into(), rights(base));
    let groups = error_groups(base, variants, &present);
    doc.insert("error_groups".into(), groups);
    insert_shadow(&mut doc, base);
    Value::Object(doc)
}

/// Whether the key at `path` exists, by variant.
///
/// @version 1
/// @req REQ-ING-PRS-001
fn presence(variants: &[Tables], path: &str) -> Value {
    let by: Map<String, Value> = variants
        .iter()
        .map(|t| (t.variant.clone(), json!(t.key_index(path).is_some())))
        .collect();
    Value::Object(by)
}

/// The fields every keyspace file starts with.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn header(tables: &Tables, commit: &str) -> Map<String, Value> {
    let units: Vec<Value> = tables
        .units
        .iter()
        .flatten()
        .map(|(token, def)| json!({"token": token, "quantity": def.quantity, "meaning": def.meaning}))
        .collect();
    let mut doc = Map::new();
    doc.insert("schema_version".into(), json!(SCHEMA_VERSION));
    let (major, minor) = (tables.version.major, tables.version.minor);
    let contract =
        json!({"name": tables.name, "major": major, "minor": minor, "patch": tables.patch});
    doc.insert("contract".into(), contract);
    doc.insert("tiv".into(), json!("1.0"));
    let ingot = json!({"version": env!("CARGO_PKG_VERSION"), "commit": commit});
    doc.insert("ingot".into(), ingot);
    doc.insert("variants".into(), json!(tables.variants));
    doc.insert("units".into(), json!(units));
    doc
}

/// Namespaces, their classes and keys, in ID order; `present` gives each key's presence.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn namespaces(tables: &Tables, present: &dyn Fn(&str) -> Value) -> Value {
    let list: Vec<Value> = tables
        .layout
        .iter()
        .map(|ns| {
            let mut classes: Vec<(u8, &String)> = ns.classes.iter().map(|(n, i)| (*i, n)).collect();
            classes.sort();
            let classes: Vec<Value> = classes
                .into_iter()
                .map(|(index, name)| class(tables, &ns.name, name, index, present))
                .collect();
            json!({"id": ns.id, "name": ns.name, "classes": classes})
        })
        .collect();
    let mut sorted = list;
    sorted.sort_by_key(|n| n["id"].as_u64());
    json!(sorted)
}

/// One class and its keys (sorted by key ID, which is key-index order within a class).
///
/// @version 1
/// @req REQ-ING-KSP-001
fn class(
    tables: &Tables,
    ns: &str,
    name: &str,
    index: u8,
    present: &dyn Fn(&str) -> Value,
) -> Value {
    let prefix = format!("{ns}.{name}.");
    let keys: Vec<&super::keys::Key> = tables
        .keys
        .iter()
        .filter(|k| k.path.starts_with(&prefix))
        .collect();
    let role = keys.first().map(|k| format!("{:?}", k.role).to_lowercase());
    let keys: Vec<Value> = keys.iter().map(|k| key(k, present(&k.path))).collect();
    json!({"index": index, "name": name, "role": role, "keys": keys})
}

/// One key: identity, type, role, unit and limits, rate, persistence, secrecy, enum, doc and
/// the shadow wire attributes it declares.
///
/// v2: the shadow wire attributes.
/// @version 2
/// @req REQ-ING-KSP-001
/// @req REQ-ING-SHD-001
fn key(k: &super::keys::Key, present: Value) -> Value {
    let range = k
        .range
        .map(|r| json!({"min": r.min, "max": r.max, "step": r.step}));
    let mut v = json!({
        "id": k.id, "name": k.path, "type": k.ty.name(),
        "role": format!("{:?}", k.role).to_lowercase(),
        "unit": k.unit, "scale": k.scale, "range": range, "allowed": k.allowed, "unset": k.unset,
        "doc": k.doc, "secret": k.secret, "write_rate_per_day": k.rate_per_day,
        "persist": k.persist.name(), "enum": k.enum_name, "present": present,
    });
    if let (Some(obj), Value::Object(wire)) = (v.as_object_mut(), json!(k.wire)) {
        obj.extend(wire);
    }
    v
}

/// One enum value: code, name and reported wire string, the member it reports as and its accept
/// aliases where it has them.
///
/// @version 1
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-005
fn value_json(code: i64, name: &str, m: &super::enums::Member) -> Value {
    let mut v = json!({"code": code, "name": name, "wire": m.wire});
    if let Some(target) = &m.report_as {
        v["report_as"] = json!(target);
    }
    if !m.accept.is_empty() {
        v["accept"] = json!(m.accept);
    }
    v
}

/// Every enum the keys use, resolved for the tables' variant, by name.
///
/// v2: each value's accept aliases, where it has any.
/// v3: one value by `value_json`.
/// @version 3
/// @req REQ-ING-KSP-001
/// @req REQ-ING-SHD-001
fn enums(tables: &Tables) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for k in &tables.keys {
        if let (Some(name), Some(e)) = (&k.enum_name, &k.enum_values) {
            out.entry(name.clone()).or_insert_with(|| {
                let mut values: Vec<(i64, &String, &super::enums::Member)> =
                    e.iter().map(|(n, m)| (m.value, n, m)).collect();
                values.sort_by_key(|(code, _, _)| *code);
                let values: Vec<Value> = values
                    .into_iter()
                    .map(|(code, n, m)| value_json(code, n, m))
                    .collect();
                json!({"name": name, "values": values})
            });
        }
    }
    out
}

/// The base enums, each with `overrides.<variant>`: per code, the variant's name and wire
/// string where they differ, or `present: false` where the variant has no such code.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn enum_overrides(base: &Tables, variants: &[Tables]) -> Vec<Value> {
    let per_variant: Vec<(&str, BTreeMap<String, Value>)> = variants
        .iter()
        .map(|t| (t.variant.as_str(), enums(t)))
        .collect();
    enums(base)
        .into_iter()
        .map(|(name, mut e)| {
            e["overrides"] = overrides_of(&name, &e["values"], &per_variant);
            e
        })
        .collect()
}

/// The non-empty per-variant changes to the values of enum `name`.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn overrides_of(
    name: &str,
    base: &Value,
    per_variant: &[(&str, BTreeMap<String, Value>)],
) -> Value {
    let overrides: Map<String, Value> = per_variant
        .iter()
        .map(|(v, en)| {
            (
                v.to_string(),
                diff(base, en.get(name).map(|x| &x["values"])),
            )
        })
        .filter(|(_, d)| d.as_object().is_some_and(|o| !o.is_empty()))
        .collect();
    Value::Object(overrides)
}

/// Enum values by code.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn by_code(v: &Value) -> BTreeMap<i64, Value> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|m| (m["code"].as_i64().unwrap_or(0), m.clone()))
        .collect()
}

/// Per code: what `variant` changes in `base` values (absent codes are `present: false`).
///
/// v2: the variant's value as a whole (accept aliases too), without its code.
/// @version 2
/// @req REQ-ING-KSP-001
/// @req REQ-ING-SHD-001
fn diff(base: &Value, variant: Option<&Value>) -> Value {
    let (b, v) = (by_code(base), variant.map(by_code).unwrap_or_default());
    let mut out = Map::new();
    for (code, m) in v.iter().filter(|(c, m)| b.get(c) != Some(m)) {
        let mut changed = m.clone();
        if let Some(obj) = changed.as_object_mut() {
            obj.remove("code");
        }
        out.insert(code.to_string(), changed);
    }
    for code in b.keys().filter(|c| !v.contains_key(c)) {
        out.insert(code.to_string(), json!({"present": false}));
    }
    Value::Object(out)
}

/// Command types: path, command, result and dedupe key IDs, owner, persistence, the wire rows
/// that reach each one and, for an internal command, why it is not on the wire.
///
/// v2: the wire rows and the internal reason.
/// v3: a row reaches every command it submits, its further commands too.
/// @version 3
/// @req REQ-ING-KSP-001
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-004
fn commands(tables: &Tables) -> Value {
    let list: Vec<Value> = tables
        .commands
        .iter()
        .map(|c| {
            let rows = &tables.shadow.commands;
            let wire: Vec<_> = rows.iter().filter(|r| r.submits(&c.path)).collect();
            json!({"name": c.path, "id": c.keys[0], "result": c.keys[1], "dedupe": c.keys[2],
                   "owner": c.owner, "persist": c.persist.name(), "wire": wire,
                   "internal": c.internal})
        })
        .collect();
    json!(list)
}

/// Each principal's identity and its rights on every key it may read or write.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn rights(tables: &Tables) -> Value {
    let list: Vec<Value> = tables
        .principals
        .iter()
        .zip(&tables.rights)
        .map(|(p, row)| {
            let keys: Vec<Value> = tables
                .keys
                .iter()
                .zip(row)
                .filter(|(_, bits)| **bits != 0)
                .map(|(k, bits)| json!({"key": k.path, "access": access(*bits)}))
                .collect();
            json!({"principal": p.name, "uid": p.uid, "token": p.token,
                   "kind": format!("{:?}", p.kind).to_lowercase(), "keys": keys})
        })
        .collect();
    json!(list)
}

/// `r`, `w` or `rw` of a non-zero rights cell.
///
/// @version 1
/// @req REQ-ING-KSP-001
fn access(bits: u8) -> &'static str {
    match (bits & READ != 0, bits & WRITE != 0) {
        (true, true) => "rw",
        (true, false) => "r",
        _ => "w",
    }
}

/// The head file of a resolved body: version, SKU, the body's SHA-256 (also its ETag) and
/// its sizes.
///
/// @version 1
/// @req REQ-ING-KSP-001
pub fn head(tables: &Tables, body: &[u8], gz_len: usize) -> Value {
    use sha2::{Digest, Sha256};
    let hash = super::model::hex(&Sha256::digest(body));
    json!({"name": tables.name, "major": tables.version.major, "minor": tables.version.minor,
           "patch": tables.patch, "sku": tables.variant, "hash": hash, "size": body.len(),
           "gz_size": gz_len, "schema_version": SCHEMA_VERSION})
}

/// The error groups with their bits; in the all-variants file, `overrides.<variant>`
/// holds a variant's own wire base, and each bit its presence by variant.
///
/// @version 1
/// @req REQ-ING-KSP-001
/// @req REQ-ING-ERR-001
fn error_groups(tables: &Tables, variants: &[Tables], present: &dyn Fn(&str) -> Value) -> Value {
    let list: Vec<Value> = tables
        .error_groups
        .iter()
        .map(|g| {
            let bits = group_bits(tables, &g.id, present);
            let overrides: Map<String, Value> = variants
                .iter()
                .filter_map(|t| {
                    let base = t.error_groups.iter().find(|v| v.id == g.id)?.wire_base;
                    (base != g.wire_base).then(|| (t.variant.clone(), json!({"wire_base": base})))
                })
                .collect();
            json!({"id": g.id, "width": g.width, "wire_base": g.wire_base, "writer": g.writer,
                   "reserved_bits": g.reserved_bits, "publish_max_bit": g.publish_max_bit,
                   "shadow": g.shadow, "bits": bits, "overrides": overrides})
        })
        .collect();
    json!(list)
}

/// The bits of group `id`, by bit: the key and its presence.
///
/// @version 1
/// @req REQ-ING-KSP-001
/// @req REQ-ING-ERR-001
fn group_bits(tables: &Tables, id: &str, present: &dyn Fn(&str) -> Value) -> Vec<Value> {
    let mut bits: Vec<(u8, &str)> = tables
        .keys
        .iter()
        .filter_map(|k| {
            k.error
                .as_ref()
                .filter(|e| e.group == id)
                .map(|e| (e.bit, k.path.as_str()))
        })
        .collect();
    bits.sort();
    bits.into_iter()
        .map(|(bit, key)| json!({"bit": bit, "key": key, "present": present(key)}))
        .collect()
}
