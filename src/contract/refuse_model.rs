//! Structural refusals of contract mode: IDs, layout, key
//! attributes, well-known roles, principals.

use super::keys::{CmdPart, Errors, Key};
use super::model::Tables;
use super::schema::{Kind, Persist};
use super::types::{WkRule, OWNER_CORE, WK_RULES};
use std::collections::{BTreeMap, BTreeSet};

/// Largest namespace ID (10 bits) and class index (5 bits).
const MAX_NAMESPACE_ID: u16 = 1023;
const MAX_CLASS_INDEX: u8 = 31;

/// Run every structural check.
///
/// @version 1
/// @req REQ-ING-TIV-010
pub fn check(tables: &Tables, errors: &mut Errors) {
    layout(tables, errors);
    ids(tables, errors);
    for key in &tables.keys {
        key_attributes(tables, key, errors);
    }
    aliases(tables, errors);
    well_known(tables, errors);
    principals(tables, errors);
}

/// Namespace IDs and names unique and in range; class indices unique per namespace, in range.
///
/// v2: namespace names unique too (grants name namespaces); class checks in class_indices.
/// @version 2
/// @req REQ-ING-TIV-010
fn layout(tables: &Tables, errors: &mut Errors) {
    let mut ids = BTreeMap::new();
    let mut names = BTreeSet::new();
    for ns in &tables.layout {
        if ns.id > MAX_NAMESPACE_ID {
            errors.push(format!(
                "{}: namespace_id {} exceeds {MAX_NAMESPACE_ID}",
                ns.name, ns.id
            ));
        }
        if let Some(other) = ids.insert(ns.id, &ns.name) {
            errors.push(format!(
                "{} and {other} share namespace_id {}",
                ns.name, ns.id
            ));
        }
        if !names.insert(&ns.name) {
            errors.push(format!("namespace {} is declared by two files", ns.name));
        }
        class_indices(ns, errors);
    }
}

/// Class indices of one namespace unique and in range.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn class_indices(ns: &super::model::NsLayout, errors: &mut Errors) {
    let mut seen = BTreeSet::new();
    for (class, index) in &ns.classes {
        if *index > MAX_CLASS_INDEX || !seen.insert(*index) {
            errors.push(format!(
                "{}.{class}: class_index {index} out of range or reused",
                ns.name
            ));
        }
    }
}

/// No ID 0, no duplicate ID, no live key on a reserved ID.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn ids(tables: &Tables, errors: &mut Errors) {
    for pair in tables.keys.windows(2) {
        if pair[0].id == pair[1].id {
            errors.push(format!(
                "{} and {} encode to the same ID {:#010X}",
                pair[0].path, pair[1].path, pair[0].id
            ));
        }
    }
    for key in &tables.keys {
        if key.id == 0 {
            errors.push(format!(
                "{}: encodes to ID 0, which is reserved forever",
                key.path
            ));
        } else if tables.reserved.binary_search(&key.id).is_ok() {
            errors.push(format!("{}: ID {:#010X} is reserved", key.path, key.id));
        }
    }
}

/// Per-key refusals: rights-like flags, rates, intervals, versions, readers.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn key_attributes(tables: &Tables, key: &Key, errors: &mut Errors) {
    let mut refuse = |why: &str| errors.push(format!("{}: {why}", key.path));
    if key.declared_read_only || key.declared_thread_safe {
        refuse("read_only and thread_safe are not allowed: rights come from principals");
    }
    if key.persist != Persist::None && key.rate_per_day == 0 {
        refuse("a persistent key needs a write_rate");
    }
    if key.persist == Persist::Deferred && key.deferred_ms == 0 {
        refuse("a deferred key needs deferred_ms > 0");
    }
    if key.introduced > tables.version {
        refuse("introduced_in is later than the contract version");
    }
    if !key.secret && !key.readers.is_empty() {
        refuse("readers are only for secret keys");
    }
    if key
        .readers
        .iter()
        .any(|r| tables.principal_index(r).is_none())
    {
        refuse("a reader is not a principal");
    }
}

/// An alias targets a live, non-alias key of the same type.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn aliases(tables: &Tables, errors: &mut Errors) {
    for key in tables.keys.iter().filter(|k| k.alias_of.is_some()) {
        let target = key.alias_of.as_deref().and_then(|p| tables.key_index(p));
        let ok = target.is_some_and(|t| {
            let t = &tables.keys[t];
            t.alias_of.is_none() && t.ty == key.ty
        });
        if !ok {
            errors.push(format!(
                "{}: alias_of must name a live, non-alias key of the same type",
                key.path
            ));
        }
    }
}

/// Every required role mapped once, to a key of the kind R-WK states.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn well_known(tables: &Tables, errors: &mut Errors) {
    for (index, rule) in WK_RULES.iter().enumerate() {
        let mapped: Vec<&Key> = tables.keys.iter().filter(|k| k.wk == Some(index)).collect();
        match mapped.as_slice() {
            [] if rule.required => {
                errors.push(format!("well-known role {} is not mapped", rule.name))
            }
            [] => {}
            [key] => wk_key(tables, index, key, errors),
            _ => errors.push(format!(
                "well-known role {} is mapped more than once",
                rule.name
            )),
        }
    }
}

/// The key mapped to role `index` has the type, class, flags and size R-WK requires.
///
/// v2: also the minimum size.
/// v3: each mismatch names the attribute, what the role needs and what the key has.
/// @version 3
/// @req REQ-ING-TIV-010
fn wk_key(tables: &Tables, index: usize, key: &Key, errors: &mut Errors) {
    let rule = WK_RULES[index];
    for why in wk_mismatches(rule, key) {
        errors.push(format!("{}: role {} {why}", key.path, rule.name));
    }
    if rule.name == "DEVICE_ID" && !key.mfg {
        errors.push(format!(
            "{}: DEVICE_ID must be in an MFG namespace",
            key.path
        ));
    }
    if rule.name == "TRACE_ENABLE" {
        trace_enable(tables, key, errors);
    }
    wk_size(tables, rule, key, errors);
}

/// What `key` lacks for `rule`: a command for a command role, otherwise a plain key of the
/// role's type, persistence class and survives-reset, each named with what the key has.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn wk_mismatches(rule: WkRule, key: &Key) -> Vec<String> {
    let is_command = key.command.as_ref().map(|(_, p)| *p) == Some(CmdPart::Command);
    let Some(ty) = rule.ty else {
        return (!is_command)
            .then(|| "needs a command key".to_string())
            .into_iter()
            .collect();
    };
    let mut out = Vec::new();
    if is_command {
        out.push("needs a plain key, not a command".to_string());
    }
    if key.ty != ty {
        out.push(format!("needs type {} (has {})", ty.name(), key.ty.name()));
    }
    if key.persist != rule.persist {
        let (need, has) = (rule.persist.name(), key.persist.name());
        out.push(format!("needs persistence {need} (has {has})"));
    }
    if key.survives_reset != rule.survives_reset {
        let (need, has) = (rule.survives_reset, key.survives_reset);
        out.push(format!("needs survives_reset = {need} (has {has})"));
    }
    out
}

/// The key is at least as large as the store core reads it (`min_size + per_principal * N`).
///
/// @version 1
/// @req REQ-ING-TIV-010
fn wk_size(tables: &Tables, rule: WkRule, key: &Key, errors: &mut Errors) {
    let principals = tables.principals.len() as u32;
    let need = rule.min_size + rule.per_principal * principals;
    if key.max_size < need {
        errors.push(format!(
            "{}: {} needs max_size >= {need} (has {})",
            key.path, rule.name, key.max_size
        ));
    }
}

/// Size of the signed trace-enable envelope (the store core's trace-enable envelope).
const TRACE_ENVELOPE_BYTES: u32 = 118;

/// The trace-enable command: non-persistent, binary, with room for the signed envelope.
///
/// v2: payload_max must hold the 118-byte envelope.
/// @version 2
/// @req REQ-ING-TIV-010
fn trace_enable(tables: &Tables, key: &Key, errors: &mut Errors) {
    let command = tables.commands.iter().find(|c| c.keys[0] == key.id);
    let ok = command.is_some_and(|c| c.persist == Persist::None && c.max_payload > 0)
        && key.ty == super::schema::KeyType::Binary;
    if !ok {
        errors.push(format!(
            "{}: TRACE_ENABLE needs a non-persistent binary command with payload_max > 0",
            key.path
        ));
    }
    if command.is_some_and(|c| c.max_payload < TRACE_ENVELOPE_BYTES) {
        errors.push(format!(
            "{}: TRACE_ENABLE needs payload_max >= {TRACE_ENVELOPE_BYTES} for its signed envelope",
            key.path
        ));
    }
}

/// Principal names unique and not the reserved owner `core`; (kind, uid, token) unique;
/// heartbeats complete; per-uid caps present.
///
/// v2: `core` is reserved for core-answered command owners.
/// @version 2
/// @req REQ-ING-TIV-010
fn principals(tables: &Tables, errors: &mut Errors) {
    let mut names = BTreeSet::new();
    for (i, p) in tables.principals.iter().enumerate() {
        if !names.insert(&p.name) || p.name == OWNER_CORE {
            errors.push(format!(
                "principal name {} is used twice or is reserved",
                p.name
            ));
        }
        if i > 0 && {
            let q = &tables.principals[i - 1];
            (q.kind, q.uid, q.token) == (p.kind, p.uid, p.token)
        } {
            errors.push(format!(
                "principal {}: (kind, uid, token) is used twice",
                p.name
            ));
        }
        if p.introduced > tables.version {
            errors.push(format!(
                "principal {}: introduced_in is later than the contract",
                p.name
            ));
        }
        heartbeat(tables, i, errors);
    }
    uid_caps(tables, errors);
    if tables.principals.is_empty() {
        errors.push("the model declares no principals".to_string());
    }
}

/// A watchdog-required principal has a heartbeat key and window; a heartbeat names a key.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn heartbeat(tables: &Tables, p: usize, errors: &mut Errors) {
    let principal = &tables.principals[p];
    let known = principal
        .heartbeat
        .as_deref()
        .map(|h| tables.key_index(h).is_some());
    if known == Some(false) {
        errors.push(format!(
            "principal {}: heartbeat is not a key",
            principal.name
        ));
    }
    let required = principal.flags & 0x01 != 0;
    if required && (known != Some(true) || principal.heartbeat_window_ms == 0) {
        errors.push(format!(
            "principal {}: watchdog_required needs a heartbeat key and heartbeat_window_ms",
            principal.name
        ));
    }
}

/// Every UNIX uid has exactly one uid_caps row and no other uid has one.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn uid_caps(tables: &Tables, errors: &mut Errors) {
    let unix: BTreeSet<u32> = tables
        .principals
        .iter()
        .filter(|p| p.kind == Kind::Unix)
        .map(|p| p.uid)
        .collect();
    let rows: Vec<u32> = tables.uid_caps.iter().map(|c| c.uid).collect();
    let listed: BTreeSet<u32> = rows.iter().copied().collect();
    if rows.len() != listed.len() || listed != unix {
        errors.push(format!(
            "uid_caps must list each UNIX uid once (UNIX uids {unix:?}, listed {rows:?})"
        ));
    }
}
