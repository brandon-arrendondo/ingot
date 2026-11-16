//! Checks against the previous release's contract.json (a minor only adds; a major reserves
//! what it removes; previous principals keep their identity and rights) and the release-review report for new principals.

use super::keys::Errors;
use super::previous_shadow;
use super::snapshot::{CommandJson, KeyJson, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

/// Roles whose writers a release review must see (with MFG keys). TRACE_ENABLE is not one: it
/// is required from the first contract with exactly one writer, previous principals keep their
/// rights in a minor and the role cannot move, so no new principal can ever write it.
const SENSITIVE_ROLES: [&str; 2] = ["RESET_REQUEST", "RESET_PRE_COMMAND"];

/// Compare `cur` with `prev`; refusals go to `errors`, release-review lines are returned.
///
/// @version 1
/// @req REQ-ING-TIV-012
pub fn compare(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) -> Vec<String> {
    let (p, c) = (version(prev), version(cur));
    if c < p {
        errors.push(format!(
            "contract {}.{} is older than the previous {}.{}",
            c.0, c.1, p.0, p.1
        ));
    }
    reserved_kept(prev, cur, errors);
    if c.0 > p.0 {
        removed_ids_reserved(prev, cur, errors);
        return Vec::new();
    }
    minor_additive(prev, cur, errors);
    review(prev, cur)
}

/// `(major, minor)` of a snapshot.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn version(s: &Snapshot) -> (u16, u16) {
    (s.contract.major, s.contract.minor)
}

/// Every previously reserved ID stays reserved.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn reserved_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    for id in prev.reserved.iter().filter(|id| !cur.reserved.contains(id)) {
        errors.push(format!("reserved ID {id:#010X} is no longer reserved"));
    }
}

/// In a major bump, every previous ID is either live under the same path or reserved.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn removed_ids_reserved(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    let live: BTreeMap<u32, &str> = cur.keys.iter().map(|k| (k.id, k.path.as_str())).collect();
    for key in &prev.keys {
        match live.get(&key.id) {
            Some(path) if *path != key.path => errors.push(format!(
                "ID {:#010X} of removed key {} is reused by {path}",
                key.id, key.path
            )),
            Some(_) => {}
            None if !cur.reserved.contains(&key.id) => errors.push(format!(
                "{} was removed but its ID {:#010X} is not reserved",
                key.path, key.id
            )),
            None => {}
        }
    }
}

/// Every previous key is still there with every attribute a minor may not change, its limits,
/// its shadow mapping and its struct value header.
///
/// v2: the struct value header.
/// @version 2
/// @req REQ-ING-TIV-012
/// @req REQ-ING-SHD-006
/// @req REQ-ING-HDR-003
fn keys_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    let cur_keys: BTreeMap<&str, &KeyJson> =
        cur.keys.iter().map(|k| (k.path.as_str(), k)).collect();
    for old in &prev.keys {
        match cur_keys.get(old.path.as_str()) {
            Some(new) => {
                key_unchanged(old, new, errors);
                limits_kept(old, new, errors);
                header_kept(old, new, errors);
                previous_shadow::key_mapping_kept(old, new, errors);
            }
            None => errors.push(format!(
                "{} was removed in a minor (a removal is a major)",
                old.path
            )),
        }
    }
}

/// Within a major: keys, principals and rights only grow, at a new minor, no well-known role
/// moves, and units, ranges and the unit vocabulary are kept.
///
/// v2: well-known roles stay on their keys.
/// v3: units, scale, sentinels and ranges (widen only), and the unit vocabulary.
/// v4: the error groups.
/// v5: the shadow wire mapping; the per-key checks moved to `keys_kept`.
/// v6: command outcomes.
/// v7: the codes of the enums topic vars name.
/// @version 7
/// @req REQ-ING-TIV-012
/// @req REQ-ING-SHD-006
/// @req REQ-ING-TIV-018
/// @req REQ-ING-TOP-008
fn minor_additive(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    keys_kept(prev, cur, errors);
    outcomes_kept(prev, cur, errors);
    var_enum_codes_kept(prev, cur, errors);
    let old_paths: BTreeSet<&str> = prev.keys.iter().map(|k| k.path.as_str()).collect();
    new_keys_at_this_version(prev, cur, &old_paths, errors);
    principals_unchanged(prev, cur, errors);
    rights_unchanged(prev, cur, &old_paths, errors);
    roles_kept(prev, cur, errors);
    units_kept(prev, cur, errors);
    error_groups_kept(prev, cur, errors);
    previous_shadow::rows_unchanged(prev, cur, errors);
}

/// Every unit token of `prev` keeps its quantity and meaning.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn units_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    for (token, def) in &prev.units {
        if cur.units.get(token) != Some(def) {
            errors.push(format!("unit {token} was removed or changed in a minor"));
        }
    }
}

/// Every well-known role of `prev` is still on the same key: moving one would change what the
/// core reads and writes, which is a major (decided 2026-11-03).
///
/// @version 1
/// @req REQ-ING-TIV-012
fn roles_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    let roles = |s: &Snapshot| -> BTreeMap<String, String> {
        s.keys
            .iter()
            .filter_map(|k| k.wk.clone().map(|w| (w, k.path.clone())))
            .collect()
    };
    let now = roles(cur);
    for (role, path) in roles(prev) {
        let moved_to = now.get(&role).map_or("no key", String::as_str);
        if moved_to != path {
            errors.push(format!(
                "well-known role {role} moved from {path} to {moved_to} in a minor (a major)"
            ));
        }
    }
}

/// Keys new since `prev` carry this version, and adding any needs a minor bump.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn new_keys_at_this_version(
    prev: &Snapshot,
    cur: &Snapshot,
    old_paths: &BTreeSet<&str>,
    errors: &mut Errors,
) {
    let now = format!("{}.{}", cur.contract.major, cur.contract.minor);
    let added: Vec<&KeyJson> = cur
        .keys
        .iter()
        .filter(|k| !old_paths.contains(k.path.as_str()))
        .collect();
    for key in added.iter().filter(|k| k.introduced_in != now) {
        errors.push(format!(
            "{}: a new key must have introduced_in {now}",
            key.path
        ));
    }
    if !added.is_empty() && prev.contract.minor == cur.contract.minor {
        errors.push("keys were added without a minor bump".to_string());
    }
}

/// An existing key keeps its ID (which encodes its type), version and every attribute a minor
/// may not change.
///
/// v2: no separate type comparison; the type is part of the ID.
/// v3: the error group and bit.
/// @version 3
/// @req REQ-ING-TIV-012
fn key_unchanged(old: &KeyJson, new: &KeyJson, errors: &mut Errors) {
    let same = old.id == new.id
        && old.introduced_in == new.introduced_in
        && old.persist == new.persist
        && old.secret == new.secret
        && old.survives_reset == new.survives_reset
        && old.deferred_ms == new.deferred_ms
        && (old.alias_of.is_none() || old.alias_of == new.alias_of)
        && old.command == new.command
        && new.max_size >= old.max_size
        && old.limits.error == new.limits.error
        && enum_kept(old, new);
    if !same {
        errors.push(format!(
            "{}: ID, type, introduced_in, persistence, secret, survives-reset, interval, alias, error bit, \
             command, enum values or a smaller max_size changed in a minor",
            old.path
        ));
    }
}

/// An existing key keeps its struct value header exactly (none stays none): the header is part
/// of the value's layout, so changing it is a major, as a type change is.
///
/// @version 1
/// @req REQ-ING-HDR-003
fn header_kept(old: &KeyJson, new: &KeyJson, errors: &mut Errors) {
    if old.header != new.header {
        errors.push(format!(
            "{}: its header was added, removed or changed in a minor (a major, as a type change is)",
            old.path
        ));
    }
}

/// An existing key keeps its unit, scale and sentinel, and accepts every value it accepted
/// before (a range or allowed set may only widen).
///
/// v2: the limits are KeyJson::limits.
/// @version 2
/// @req REQ-ING-RNG-001
fn limits_kept(old: &KeyJson, new: &KeyJson, errors: &mut Errors) {
    let same = old.unit == new.unit
        && old.limits.scale == new.limits.scale
        && old.limits.unset == new.limits.unset;
    if !(same && still_accepted(old, new)) {
        errors.push(format!(
            "{}: unit, scale or unset changed, or its range or allowed set narrowed, in a minor",
            old.path
        ));
    }
}

/// Whether `new` accepts every value `old` did (an unlimited key accepts everything).
///
/// v2: the limits are KeyJson::limits.
/// @version 2
/// @req REQ-ING-RNG-001
fn still_accepted(old: &KeyJson, new: &KeyJson) -> bool {
    let accepts = |v: i64| match (&new.limits.range, &new.limits.allowed) {
        (Some(r), _) => r.contains(v),
        (None, Some(a)) => a.contains(&v),
        (None, None) => true,
    };
    match (&old.limits.range, &old.limits.allowed) {
        (Some(r), _) => {
            new.limits.range.is_none() && new.limits.allowed.is_none()
                || new
                    .limits
                    .range
                    .is_some_and(|n| n.contains(r.min) && n.contains(r.max) && r.step % n.step == 0)
        }
        (None, Some(a)) => a.iter().all(|v| accepts(*v)),
        (None, None) => new.limits.range.is_none() && new.limits.allowed.is_none(),
    }
}

/// Whether `new` keeps every (name, code) member of `old`: both absent, or both present and
/// every old name still has its code (a minor may add members).
///
/// @version 1
/// @req REQ-ING-TIV-016
/// @req REQ-ING-TIV-018
fn members_kept(old: Option<&BTreeMap<String, i64>>, new: Option<&BTreeMap<String, i64>>) -> bool {
    match (old, new) {
        (None, None) => true,
        (Some(a), Some(b)) => a.iter().all(|(name, v)| b.get(name) == Some(v)),
        _ => false,
    }
}

/// Every previous enum member keeps its value, and every previous number its wire string.
///
/// v2: wire strings too (a snapshot from before wire strings has none to keep).
/// v3: the members compared by `members_kept`.
/// @version 3
/// @req REQ-ING-TIV-012
/// @req REQ-ING-TIV-016
fn enum_kept(old: &KeyJson, new: &KeyJson) -> bool {
    let values = members_kept(old.enum_values.as_ref(), new.enum_values.as_ref());
    let empty = BTreeMap::new();
    let new_wire = new.enum_wire.as_ref().unwrap_or(&empty);
    let wires = old
        .enum_wire
        .iter()
        .flatten()
        .all(|(code, wire)| new_wire.get(code) == Some(wire));
    values && wires
}

/// Every previous command that named outcomes still names them, each previous outcome with its
/// code (a command removed in a minor is refused with its keys).
///
/// @version 1
/// @req REQ-ING-TIV-018
fn outcomes_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    let now: BTreeMap<&str, &CommandJson> =
        cur.commands.iter().map(|c| (c.path.as_str(), c)).collect();
    for old in prev.commands.iter().filter(|c| c.outcome.is_some()) {
        let Some(new) = now.get(old.path.as_str()) else {
            continue;
        };
        if !members_kept(old.outcome.as_ref(), new.outcome.as_ref()) {
            errors.push(format!(
                "{}: its outcomes were removed or changed in a minor",
                old.path
            ));
        }
    }
}

/// Every previous enum a topic var named keeps each (name, code) pair while a var still names
/// it (a dropped reference is refused with its var).
///
/// @version 1
/// @req REQ-ING-TOP-008
fn var_enum_codes_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    for (name, old) in &prev.var_enums {
        let new = cur.var_enums.get(name);
        if new.is_some_and(|n| !members_kept(Some(old), Some(n))) {
            errors.push(format!(
                "enum {name}: a value a topic var carries was removed or changed its code in a minor"
            ));
        }
    }
}

/// Every previous principal is still there with the same uid, token, kind and flags.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn principals_unchanged(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    for old in &prev.principals {
        let kept = cur.principals.iter().any(|new| {
            new.name == old.name
                && (new.uid, new.token, &new.kind, new.flags)
                    == (old.uid, old.token, &old.kind, old.flags)
        });
        if !kept {
            errors.push(format!(
                "principal {} was removed or its uid, token, kind or flags changed in a minor",
                old.name
            ));
        }
    }
}

/// Previous principals keep exactly their rights on previous keys.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn rights_unchanged(
    prev: &Snapshot,
    cur: &Snapshot,
    old_paths: &BTreeSet<&str>,
    errors: &mut Errors,
) {
    let empty = BTreeMap::new();
    for (name, old) in &prev.rights {
        let new = cur.rights.get(name).unwrap_or(&empty);
        let changed = old_paths
            .iter()
            .any(|path| old.get(*path) != new.get(*path));
        if changed {
            errors.push(format!(
                "principal {name}: rights on existing keys changed in a minor"
            ));
        }
    }
}

/// Release-review lines: new principals granted read on an existing secret, or write on an
/// existing MFG or reset key.
///
/// @version 1
/// @req REQ-ING-TIV-012
fn review(prev: &Snapshot, cur: &Snapshot) -> Vec<String> {
    let old: BTreeMap<&str, &KeyJson> = prev.keys.iter().map(|k| (k.path.as_str(), k)).collect();
    let known: BTreeSet<&str> = prev.principals.iter().map(|p| p.name.as_str()).collect();
    let mut lines = Vec::new();
    for (name, cells) in cur
        .rights
        .iter()
        .filter(|(n, _)| !known.contains(n.as_str()))
    {
        for (path, access) in cells {
            let Some(key) = old.get(path.as_str()) else {
                continue;
            };
            if sensitive(key, access) {
                lines.push(format!("new principal {name}: {access} on existing {path}"));
            }
        }
    }
    lines
}

/// Whether `access` on `key` is a grant a release review must acknowledge.
///
/// v2: reset roles only (see SENSITIVE_ROLES).
/// @version 2
/// @req REQ-ING-TIV-012
fn sensitive(key: &KeyJson, access: &str) -> bool {
    let writes = access.contains('w');
    let reset = key
        .wk
        .as_deref()
        .is_some_and(|w| SENSITIVE_ROLES.contains(&w));
    (key.secret && access.contains('r')) || (writes && (reset || key.mfg))
}

/// Every previous error group keeps its width, wire base, writer and published range, and its
/// reserved bits only grow.
///
/// @version 1
/// @req REQ-ING-ERR-001
fn error_groups_kept(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    for old in &prev.error_groups {
        let kept = cur.error_groups.iter().any(|new| {
            (
                &new.id,
                new.width,
                new.wire_base,
                &new.writer,
                new.publish_max_bit,
            ) == (
                &old.id,
                old.width,
                old.wire_base,
                &old.writer,
                old.publish_max_bit,
            ) && old
                .reserved_bits
                .iter()
                .all(|b| new.reserved_bits.contains(b))
        });
        if !kept {
            errors.push(format!(
                "error group {} was removed or changed in a minor",
                old.id
            ));
        }
    }
}
