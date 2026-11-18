//! Error groups: one bool STATE key per error, composed into a group word whose
//! bit positions, per resolved variant, are unique and append-only; the wire id of a bit is the
//! group's (per-variant) wire base plus the bit, published as `e<id>`.

use super::keys::{Errors, Key};
use super::model::Tables;
use super::schema::{ErrorGroupDef, KeyType, Role};
use std::collections::BTreeSet;

/// One error group resolved for a variant.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorGroup {
    pub id: String,
    pub width: u8,
    pub wire_base: u32,
    pub writer: String,
    pub reserved_bits: Vec<u8>,
    pub publish_max_bit: u8,
    pub shadow: Option<String>,
}

/// The groups as `variant` sees them (its wire base where it overrides one).
///
/// @version 1
/// @req REQ-ING-ERR-001
pub fn resolve(defs: &[ErrorGroupDef], variant: Option<&str>) -> Vec<ErrorGroup> {
    defs.iter()
        .map(|d| ErrorGroup {
            id: d.id.clone(),
            width: d.width,
            wire_base: variant
                .and_then(|v| d.wire_bases.get(v))
                .copied()
                .unwrap_or(d.wire_base),
            writer: d.writer.clone(),
            reserved_bits: d.reserved_bits.clone(),
            publish_max_bit: d.publish_max_bit.unwrap_or(d.width.saturating_sub(1)),
            shadow: d.shadow.clone(),
        })
        .collect()
}

/// Every error group and error key of the tables' variant.
///
/// @version 1
/// @req REQ-ING-ERR-001
pub fn check(tables: &Tables, errors: &mut Errors) {
    groups(tables, errors);
    let mut taken = BTreeSet::new();
    for (k, key) in tables.keys.iter().enumerate() {
        if let Some(e) = &key.error {
            match tables.error_groups.iter().find(|g| g.id == e.group) {
                Some(g) => error_key(tables, g, k, key, errors),
                None => errors.push(format!(
                    "{}: error group {} is not declared",
                    key.path, e.group
                )),
            }
            if !taken.insert((e.group.clone(), e.bit)) {
                errors.push(format!(
                    "{}: bit {} of error group {} is taken",
                    key.path, e.bit, e.group
                ));
            }
        }
    }
}

/// Group widths, sentinels, writers and non-overlapping wire-id ranges.
///
/// @version 1
/// @req REQ-ING-ERR-001
fn groups(tables: &Tables, errors: &mut Errors) {
    let mut ids = BTreeSet::new();
    for g in &tables.error_groups {
        let ok = [8, 16, 32, 64].contains(&g.width)
            && g.publish_max_bit < g.width
            && g.reserved_bits.iter().all(|b| *b < g.width)
            && tables.principal_index(&g.writer).is_some()
            && ids.insert(&g.id);
        if !ok {
            errors.push(format!(
                "error group {}: width 8, 16, 32 or 64, bits below it, a principal writer and a unique id",
                g.id
            ));
        }
    }
    for (i, a) in tables.error_groups.iter().enumerate() {
        for b in &tables.error_groups[i + 1..] {
            let end = |g: &super::error_groups::ErrorGroup| g.wire_base + u32::from(g.width);
            if a.wire_base < end(b) && b.wire_base < end(a) {
                errors.push(format!(
                    "error groups {} and {} have overlapping wire ids",
                    a.id, b.id
                ));
            }
        }
    }
}

/// One error key: a bool STATE key on a free bit below the width, written by the group's
/// writer only.
///
/// @version 1
/// @req REQ-ING-ERR-001
fn error_key(tables: &Tables, g: &ErrorGroup, k: usize, key: &Key, errors: &mut Errors) {
    let bit = key.error.as_ref().map_or(0, |e| e.bit);
    let mut refuse = |why: String| errors.push(format!("{}: {why}", key.path));
    if key.ty != KeyType::Bool || key.role != Role::State {
        refuse("an error key is a bool STATE key".into());
    }
    if bit >= g.width || g.reserved_bits.contains(&bit) {
        refuse(format!(
            "bit {bit} is reserved or not below width {} of group {}",
            g.width, g.id
        ));
    }
    let writers: Vec<&str> = (0..tables.principals.len())
        .filter(|&p| tables.writes(p, k))
        .map(|p| tables.principals[p].name.as_str())
        .collect();
    if writers != [g.writer.as_str()] {
        refuse(format!(
            "an error key is written by {} alone (writers: {writers:?})",
            g.writer
        ));
    }
}
