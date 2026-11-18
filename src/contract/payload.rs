//! Topic payloads built from keys: `[[topic_payload]]` members of a pub topic,
//! each from a key (CONFIG, SETTINGS or STATE, any JSON-valued type) or a constant; their
//! refusals (T3) and their `dtp_payload` rows.

use super::keys::{Errors, Key};
use super::model::Tables;
use super::refuse_shadow::known_unknown;
use super::schema::{Direction, KeyType, PayloadRow};
use super::shadow_size::PathTree;
use std::collections::BTreeMap;

/// `topic_payload <topic> <path>`, a member's name.
///
/// @version 1
/// @req REQ-ING-TOP-003
pub fn name(r: &PayloadRow) -> String {
    format!("topic_payload {} {}", r.topic, r.path)
}

/// The key a member carries, when it names one that exists.
///
/// @version 1
/// @req REQ-ING-TOP-003
pub fn key_of<'a>(tables: &'a Tables, r: &PayloadRow) -> Option<&'a Key> {
    r.key
        .as_deref()
        .and_then(|k| tables.key_index(k))
        .map(|i| &tables.keys[i])
}

/// The refusal of a member's source: a pub topic, exactly one of key, value or image, a known
/// key with a JSON value.
///
/// @version 1
/// @req REQ-ING-TOP-003
fn source_rule(tables: &Tables, r: &PayloadRow) -> Option<String> {
    let pub_topic = tables
        .shadow
        .topics
        .iter()
        .any(|t| t.id == r.topic && t.direction == Direction::Pub);
    let sources = [r.key.is_some(), r.value.is_some(), r.image.is_some()];
    let key = key_of(tables, r);
    if !pub_topic {
        Some(format!("topic {} is not a pub topic", r.topic))
    } else if sources.iter().filter(|s| **s).count() != 1 {
        Some("a member has one of key, value or image".into())
    } else if r.key.is_some() && key.is_none() {
        Some(format!(
            "key {} is not a key of this variant",
            r.key.as_deref().unwrap_or_default()
        ))
    } else if key.is_some_and(|k| matches!(k.ty, KeyType::Binary | KeyType::Struct)) {
        Some("a binary or struct key has no JSON value".into())
    } else {
        None
    }
}

/// The refusal of a member's form: `as` only on an enum key, and `outbound_unknown` exactly on
/// an enum key's wire-string member, as `raw` or a wire string of the enum.
///
/// @version 1
/// @req REQ-ING-TOP-003
fn form_rule(key: Option<&Key>, r: &PayloadRow) -> Option<String> {
    let e = key.and_then(|k| k.enum_values.as_ref());
    let strings = e.filter(|_| r.form != Some(super::schema::WireForm::Integer));
    match (strings, r.outbound_unknown.as_deref()) {
        _ if r.form.is_some() && e.is_none() => Some("as is for enum keys".into()),
        (Some(_), None) => {
            Some("an enum key's wire-string member declares outbound_unknown".into())
        }
        (None, Some(_)) => Some("outbound_unknown is for an enum key's wire-string member".into()),
        (Some(e), Some(v)) if !known_unknown(e, v, false) => Some(format!(
            "outbound_unknown {v} is neither raw nor a wire string of the enum"
        )),
        _ => None,
    }
}

/// Every payload refusal (T3): each member's source and form, and the paths of each topic's
/// payload (dotted identifiers, one member per path, no path both a value and an object).
///
/// @version 1
/// @req REQ-ING-TOP-003
pub fn check(tables: &Tables, errors: &mut Errors) {
    let mut trees: BTreeMap<&str, PathTree> = BTreeMap::new();
    for r in &tables.shadow.payload {
        let why = source_rule(tables, r).or_else(|| form_rule(key_of(tables, r), r));
        if let Some(why) = why {
            errors.push(format!("{}: {why}", name(r)));
        }
        trees.entry(&r.topic).or_default().add(&name(r), &r.path, 0);
    }
    errors.extend(trees.into_values().flat_map(|t| t.errors));
}
