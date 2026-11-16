//! Refusals of the log streams on the selected variant: names, level masks, the control and
//! drop-counter keys, backings, line caps, flash structure and versions. The budgets are in
//! bounds.rs.

use super::keys::Errors;
use super::log_streams::{streams, LogStream};
use super::model::Tables;
use super::schema::KeyType;
use std::collections::BTreeSet;

/// Run every log stream refusal.
///
/// @version 1
/// @req REQ-ING-LOG-004
/// @req REQ-ING-LOG-003
pub fn check(tables: &Tables, errors: &mut Errors) {
    names(tables, errors);
    for (_, s) in streams(tables) {
        let mut refuse = |why: String| errors.push(format!("log stream {}: {why}", s.name));
        for why in keys(tables, s).into_iter().chain(shape(tables, s)) {
            refuse(why);
        }
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

/// What is wrong with the stream's control and drop-counter keys.
///
/// @version 1
/// @req REQ-ING-LOG-004
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
    let unsigned = [KeyType::Uint8, KeyType::Uint16, KeyType::Uint32];
    if !tables
        .key_index(drop)
        .is_some_and(|k| unsigned.contains(&tables.keys[k].ty))
    {
        out.push(format!(
            "drop_counter_key {drop} is not an unsigned integer key"
        ));
    }
    out
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
