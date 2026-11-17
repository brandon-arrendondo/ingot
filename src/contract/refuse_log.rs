//! Refusals of the log streams, the core's and the principals', on the selected variant: names,
//! key roles, level masks, the control, drop-counter and state keys, who may switch a stream,
//! backings, flush triggers, line caps, flash structure and versions. The budgets are in
//! bounds.rs.

use super::keys::Errors;
use super::log_streams::{streams, LogStream};
use super::model::Tables;
use super::schema::{Backing, KeyType};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

/// Run every log stream refusal.
///
/// v2: key roles, state keys, the stream's own process and flush triggers.
/// @version 2
/// @req REQ-ING-LOG-004
/// @req REQ-ING-LOG-003
/// @req REQ-ING-LOG-006
/// @req REQ-ING-LOG-008
pub fn check(tables: &Tables, errors: &mut Errors) {
    names(tables, errors);
    roles(tables, errors);
    for (p, s) in streams(tables) {
        let reasons = keys(tables, s)
            .into_iter()
            .chain(own_switch(tables, p, s))
            .chain(shape(tables, s))
            .chain(triggers(tables, s));
        errors.extend(reasons.map(|why| format!("log stream {}: {why}", s.name)));
    }
}

/// Stream names unique and lower-case identifiers.
///
/// @version 1
/// @req REQ-ING-LOG-004
fn names(tables: &Tables, errors: &mut Errors) {
    let mut seen = BTreeSet::new();
    for (_, s) in streams(tables) {
        if !seen.insert(s.name.as_str()) {
            errors.push(format!("log stream name {} is used twice", s.name));
        }
        if !identifier(&s.name) {
            errors.push(format!(
                "log stream name {} is not a lower-case identifier",
                s.name
            ));
        }
    }
}

/// Each key serves one log role in one stream: a key named again, as any of the ctl_key,
/// drop_counter_key and state_key of any stream, is refused where it is named again.
///
/// @version 1
/// @req REQ-ING-LOG-008
fn roles(tables: &Tables, errors: &mut Errors) {
    let mut seen: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for (_, s) in streams(tables) {
        let d = &s.def;
        let named = [
            ("ctl_key", &d.ctl_key),
            ("drop_counter_key", &d.drop_counter_key),
            ("state_key", &d.state_key),
        ];
        for (role, key) in named {
            match seen.entry(key.as_str()) {
                Entry::Occupied(first) => {
                    let (was, owner) = first.get();
                    errors.push(format!(
                        "log stream {}: {role} {key} is also the {was} of log stream {owner}",
                        s.name
                    ));
                }
                Entry::Vacant(slot) => {
                    slot.insert((role, s.name.as_str()));
                }
            }
        }
    }
}

/// Whether `name` is `[a-z][a-z0-9_]*`.
///
/// @version 1
/// @req REQ-ING-LOG-004
/// @return true for a lower-case identifier.
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Index of the key at `path` when it is an unsigned integer key.
///
/// @version 1
/// @req REQ-ING-LOG-004
/// @req REQ-ING-LOG-007
/// @return the index, or None.
fn unsigned_key(tables: &Tables, path: &str) -> Option<usize> {
    let unsigned = [KeyType::Uint8, KeyType::Uint16, KeyType::Uint32];
    tables
        .key_index(path)
        .filter(|&k| unsigned.contains(&tables.keys[k].ty))
}

/// What is wrong with the stream's control, drop-counter and state keys.
///
/// v2: the state key: an unsigned integer key no principal may write.
/// @version 2
/// @req REQ-ING-LOG-004
/// @req REQ-ING-LOG-007
/// @return the reasons.
fn keys(tables: &Tables, s: &LogStream) -> Vec<String> {
    let mut out = Vec::new();
    let ctl = &s.def.ctl_key;
    match tables.key_index(ctl) {
        None => out.push(format!("ctl_key {ctl} is not a key")),
        Some(k) if !(0..tables.principals.len()).any(|p| tables.writes(p, k)) => {
            out.push(format!("no principal may write ctl_key {ctl}"))
        }
        Some(_) => {}
    }
    let drop = &s.def.drop_counter_key;
    if unsigned_key(tables, drop).is_none() {
        out.push(format!(
            "drop_counter_key {drop} is not an unsigned integer key"
        ));
    }
    let state = &s.def.state_key;
    match unsigned_key(tables, state) {
        None => out.push(format!("state_key {state} is not an unsigned integer key")),
        Some(k) => {
            let writer = (0..tables.principals.len()).find(|&p| tables.writes(p, k));
            let name = writer.map(|p| &tables.principals[p].name);
            out.extend(name.map(|w| format!("principal {w} may write state_key {state}")));
        }
    }
    out
}

/// Every principal of the process of stream principal `p` (same kind and uid: a token issuer
/// and its token sub-consumers) that may write the stream's ctl_key; the core's stream (`None`)
/// has none.
///
/// @version 1
/// @req REQ-ING-LOG-008
/// @return the reasons.
fn own_switch(tables: &Tables, p: Option<usize>, s: &LogStream) -> Vec<String> {
    let ctl = &s.def.ctl_key;
    let (Some(p), Some(k)) = (p, tables.key_index(ctl)) else {
        return Vec::new();
    };
    let me = &tables.principals[p];
    let process = |q: &usize| {
        let other = &tables.principals[*q];
        other.kind == me.kind && other.uid == me.uid
    };
    (0..tables.principals.len())
        .filter(|q| process(q) && tables.writes(*q, k))
        .map(|q| {
            let name = &tables.principals[q].name;
            format!("principal {name} runs in the stream's own process and may write ctl_key {ctl}")
        })
        .collect()
}

/// What is wrong with the stream's levels, backing, line cap, flash figures and version on the
/// tables' variant.
///
/// @version 1
/// @req REQ-ING-LOG-004
/// @req REQ-ING-LOG-003
/// @return the reasons.
fn shape(tables: &Tables, s: &LogStream) -> Vec<String> {
    let (d, v) = (&s.def, tables.variant.as_str());
    let mut out = Vec::new();
    let (ram, file) = s.levels();
    if file & !ram != 0 {
        out.push("default_level_file is outside default_level_ram".to_string());
    }
    let backing = s.backing(v);
    if s.allowed(v) & (1 << backing.code()) == 0 {
        let name = backing.name();
        out.push(format!(
            "backing {name} is not an allowed backing on this variant"
        ));
    }
    if d.max_line_bytes == 0 || (s.may_enable(v) && d.max_line_bytes > d.ram_cap_bytes) {
        out.push(format!(
            "max_line_bytes {} must be above 0 and within ram_cap_bytes {}",
            d.max_line_bytes, d.ram_cap_bytes
        ));
    }
    if s.may_flash(v) {
        out.extend(flash(s));
    }
    if s.introduced > tables.version {
        out.push("introduced_in is later than the contract".to_string());
    }
    out
}

/// What is wrong with the stream's flush triggers: triggers with no flash backing on any
/// variant, and flash on trigger allowed on the tables' variant with no trigger.
///
/// @version 1
/// @req REQ-ING-LOG-009
/// @return the reasons.
fn triggers(tables: &Tables, s: &LogStream) -> Vec<String> {
    let flush = &s.def.flush;
    let mut out = Vec::new();
    if !flush.is_empty() && !s.may_flash_on_any(&tables.variants) {
        out.push("flush triggers need a flash backing, which no variant allows".to_string());
    }
    let on_trigger = 1 << Backing::FlashOnTrigger.code();
    if flush.is_empty() && s.allowed(&tables.variant) & on_trigger != 0 {
        out.push(
            "flash_on_trigger is allowed on this variant but no flush trigger is declared"
                .to_string(),
        );
    }
    out
}

/// What is wrong with a flash stream's segment, rate and line figures.
///
/// @version 1
/// @req REQ-ING-LOG-003
/// @req REQ-ING-LOG-004
/// @return the reasons.
fn flash(s: &LogStream) -> Vec<String> {
    let d = &s.def;
    let mut out = Vec::new();
    if d.flash_seg_bytes == 0 || d.flash_seg_count == 0 || d.flash_rate_bytes_per_day == 0 {
        out.push(
            "a flash backing needs flash_seg_bytes, flash_seg_count and flash_rate_bytes_per_day \
             above 0"
                .to_string(),
        );
    }
    if d.max_line_bytes > d.flash_seg_bytes || d.max_line_bytes > d.flash_rate_burst_bytes {
        out.push(format!(
            "max_line_bytes {} must be within flash_seg_bytes {} and flash_rate_burst_bytes {}",
            d.max_line_bytes, d.flash_seg_bytes, d.flash_rate_burst_bytes
        ));
    }
    out
}
