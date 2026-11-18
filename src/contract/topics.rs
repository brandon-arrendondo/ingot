//! Topic declarations and command envelopes, and their refusals.

use super::keys::Errors;
use super::model::Tables;
use super::schema::{Direction, Topic, TopicVar};
use super::topic_vars;
use std::collections::BTreeSet;

/// The per-device placeholders every template may use, each any number of times.
pub const PLACEHOLDERS: [&str; 5] = ["stage", "duid", "model", "thing", "shadow_name"];

/// A topic attribute that must be unique, read from a topic.
type TopicField = fn(&Topic) -> Option<String>;

/// The placeholder that names the shadow.
const SHADOW_NAME: &str = "{shadow_name}";

/// Every topic refusal.
///
/// v2: a template may use its topic's vars; the var rules.
/// v3: the declared placeholders.
/// v4: the var rules see the tables (a var's enum).
/// @version 4
/// @req REQ-ING-TOP-001
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-007
/// @req REQ-ING-TOP-008
pub fn check(tables: &Tables, errors: &mut Errors) {
    let topics = &tables.shadow.topics;
    let placeholders = &tables.shadow.placeholders;
    unique(topics, errors);
    for t in topics {
        if !template_ok(t, placeholders) {
            errors.push(format!(
                "topic {}: template {} uses a placeholder outside {{{}}}, the declared placeholders and the topic's vars",
                t.id,
                t.template,
                PLACEHOLDERS.join("}, {")
            ));
        }
        topic_vars::check(t, tables, errors);
        placement(topics, t, errors);
        envelope(tables, t, errors);
    }
    topic_vars::check_placeholders(tables, errors);
    shadow_names(topics, errors);
}

/// Whether every `{...}` of the topic's template is a per-device placeholder, a declared
/// placeholder (`placeholders`) or a var of the topic, and every brace is paired.
///
/// v2: the topic's vars.
/// v3: the declared placeholders.
/// @version 3
/// @req REQ-ING-TOP-001
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-007
fn template_ok(t: &Topic, placeholders: &[TopicVar]) -> bool {
    let known = |name: &str| {
        PLACEHOLDERS.contains(&name)
            || placeholders.iter().any(|p| p.name == name)
            || topic_vars::declared(t, name)
    };
    let mut pieces = t.template.split('{');
    let head = pieces.next().unwrap_or_default();
    !head.contains('}')
        && pieces.all(|p| {
            p.matches('}').count() == 1 && p.split_once('}').is_some_and(|(name, _)| known(name))
        })
}

/// Ids, templates, subscribe orders and local wrappers are unique.
///
/// @version 1
/// @req REQ-ING-TOP-001
fn unique(topics: &[Topic], errors: &mut Errors) {
    let fields: [(&str, TopicField); 4] = [
        ("id", |t| Some(t.id.clone())),
        ("template", |t| Some(t.template.clone())),
        ("subscribe_order", |t| {
            t.subscribe_order.map(|o| o.to_string())
        }),
        ("local_wrapper", |t| t.local_wrapper.clone()),
    ];
    for (label, value) in fields {
        let mut seen = BTreeSet::new();
        for v in topics.iter().filter_map(value) {
            if !seen.insert(v.clone()) {
                errors.push(format!("topics: {label} {v} is used twice"));
            }
        }
    }
}

/// Subscription attributes are on sub topics; an ack names a pub topic.
///
/// @version 1
/// @req REQ-ING-TOP-001
fn placement(topics: &[Topic], t: &Topic, errors: &mut Errors) {
    let sub_only = t.subscribe_order.is_some()
        || t.local_wrapper.is_some()
        || t.ack.is_some()
        || t.envelope.is_some()
        || t.settings_in_every_body;
    if t.direction == Direction::Pub && sub_only {
        errors.push(format!(
            "topic {}: subscribe_order, local_wrapper, ack, envelope and settings_in_every_body are for sub topics",
            t.id
        ));
    }
    let is_pub = |id: &String| {
        topics
            .iter()
            .any(|o| &o.id == id && o.direction == Direction::Pub)
    };
    if let Some(ack) = t.ack.as_ref().filter(|a| !is_pub(a)) {
        errors.push(format!("topic {}: ack {ack} is not a pub topic", t.id));
    }
}

/// An envelope id is required or has an absent value; a source's absent value is a member of its
/// enum.
///
/// @version 1
/// @req REQ-ING-TOP-001
fn envelope(tables: &Tables, t: &Topic, errors: &mut Errors) {
    let Some(env) = &t.envelope else {
        return;
    };
    if env
        .id
        .as_ref()
        .is_some_and(|id| id.required == id.absent.is_some())
    {
        errors.push(format!(
            "topic {}: the envelope id is required, or optional with an absent value",
            t.id
        ));
    }
    if let Some(s) = &env.source {
        let member = tables
            .enums
            .get(&s.enum_ref)
            .map(|e| e.contains_key(&s.absent));
        if member != Some(true) {
            errors.push(format!(
                "topic {}: the envelope source absent value {} is not a member of enum {}",
                t.id, s.absent, s.enum_ref
            ));
        }
    }
}

/// The path segment after `/shadow/name/`, when the template has one.
///
/// @version 1
/// @req REQ-ING-TOP-001
fn shadow_name(template: &str) -> Option<&str> {
    let (_, rest) = template.split_once("/shadow/name/")?;
    rest.split('/').next()
}

/// A literal shadow name where another topic uses `{shadow_name}` is refused unless the
/// topic keeps it explicitly; the flag is only for such a topic.
///
/// @version 1
/// @req REQ-ING-TOP-001
fn shadow_names(topics: &[Topic], errors: &mut Errors) {
    let placeholder = topics
        .iter()
        .any(|t| shadow_name(&t.template) == Some(SHADOW_NAME));
    for t in topics {
        let literal = shadow_name(&t.template).filter(|n| *n != SHADOW_NAME);
        match (literal, t.literal_shadow_name) {
            (Some(n), false) if placeholder => errors.push(format!(
                "topic {}: shadow name {n} is a literal while another topic uses {SHADOW_NAME}",
                t.id
            )),
            (Some(_), true) if placeholder => {}
            (_, true) => errors.push(format!(
                "topic {}: literal_shadow_name is for a literal shadow name beside {SHADOW_NAME}",
                t.id
            )),
            _ => {}
        }
    }
}
