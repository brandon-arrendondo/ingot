//! Topic placeholders beyond the per-device ones, bounded in count and length:
//! - per-message vars, declared per topic: on a pub topic the publisher fills them at publish
//!   time; on a sub topic each is one whole topic level, which the consumer subscribes to as a
//!   single-level wildcard and extracts from a received topic; a var may name the model enum
//!   whose code it carries as decimal text;
//! - declared placeholders, contract-level: named values the consumer is configured with (a
//!   build or device value), usable in any template.

use super::enums::ResolvedEnum;
use super::keys::Errors;
use super::model::Tables;
use super::schema::{Direction, Topic, TopicVar};
use super::shadow_size::valid_path;
use super::topics::PLACEHOLDERS;
use std::collections::{BTreeMap, BTreeSet};

/// The most vars one topic declares (`DTP_VARS_MAX`), and the most declared placeholders
/// (`DTP_PLACEHOLDERS_MAX`).
pub const VARS_MAX: usize = 8;

/// The longest value a var or declared placeholder may declare, in bytes (`DTP_VAR_LEN_MAX`).
pub const VAR_LEN_MAX: u16 = 128;

/// Whether `name` is a var of `t`.
///
/// @version 1
/// @req REQ-ING-TOP-004
pub fn declared(t: &Topic, name: &str) -> bool {
    t.vars.iter().any(|v| v.name == name)
}

/// Why the name or length bound of `v`, a `kind` ("var" or "placeholder"), is refused.
///
/// @version 1
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-007
fn name_rules(kind: &str, v: &TopicVar) -> Vec<String> {
    let mut why = Vec::new();
    let name = &v.name;
    if !valid_path(name) || name.contains('.') || PLACEHOLDERS.contains(&name.as_str()) {
        why.push(format!(
            "{kind} {name} must be an identifier other than a built-in placeholder"
        ));
    }
    if v.max_len == 0 || v.max_len > VAR_LEN_MAX {
        why.push(format!("{kind} {name} needs max_len 1..={VAR_LEN_MAX}"));
    }
    why
}

/// Why the list `all` of `kind` is refused: more than the bound, a name declared twice, and each
/// entry's name rules.
///
/// @version 1
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-007
fn list_rules(kind: &str, all: &[TopicVar]) -> Vec<String> {
    let mut why = Vec::new();
    if all.len() > VARS_MAX {
        why.push(format!("more than {VARS_MAX} {kind}s"));
    }
    let mut seen = BTreeSet::new();
    for v in all {
        if !seen.insert(&v.name) {
            why.push(format!("{kind} {} is declared twice", v.name));
        }
        why.extend(name_rules(kind, v));
    }
    why
}

/// Whether `template` uses `{name}`.
///
/// @version 1
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-007
fn uses(template: &str, name: &str) -> bool {
    template.contains(&format!("{{{name}}}"))
}

/// Whether `{name}` is exactly one whole level of the template, once.
///
/// @version 1
/// @req REQ-ING-TOP-006
fn one_whole_level(template: &str, name: &str) -> bool {
    let placeholder = format!("{{{name}}}");
    let levels = template.split('/').filter(|l| *l == placeholder).count();
    levels == 1 && template.matches(&placeholder).count() == 1
}

/// Why var `v` of `t` is refused beyond the list rules: a template without it, a declared
/// placeholder's name, or on a sub topic not one whole level used once.
///
/// v2: a declared placeholder's name; the sub-topic rule here; name rules in `list_rules`.
/// @version 2
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-006
/// @req REQ-ING-TOP-007
fn var_rule(t: &Topic, v: &TopicVar, placeholders: &[TopicVar]) -> Vec<String> {
    let mut why = Vec::new();
    let name = &v.name;
    if !uses(&t.template, name) {
        why.push(format!("var {name} is not used by its template"));
    }
    if placeholders.iter().any(|p| &p.name == name) {
        why.push(format!("var {name} is a declared placeholder"));
    }
    if t.direction == Direction::Sub && !one_whole_level(&t.template, name) {
        why.push(format!(
            "var {name} must be one whole topic level, used once, on a sub topic"
        ));
    }
    why
}

/// Why a code of the enum `e` names is refused for var `v`: a negative code, or one whose
/// decimal text is longer than the var's max_len.
///
/// @version 1
/// @req REQ-ING-TOP-008
fn code_rule(v: &TopicVar, e: &str, member: &str, code: i64) -> Option<String> {
    let name = &v.name;
    if code < 0 {
        Some(format!(
            "var {name}: enum {e} value {member} has negative code {code}"
        ))
    } else if code.to_string().len() > usize::from(v.max_len) {
        Some(format!(
            "var {name}: enum {e} value {member} code {code} is longer than max_len {}",
            v.max_len
        ))
    } else {
        None
    }
}

/// Why the enum var `v` names is refused, as the variant resolves it (`enums`: every enum by
/// `namespace::enum`): an undefined enum, and each code `code_rule` refuses.
///
/// @version 1
/// @req REQ-ING-TOP-008
fn enum_rule(v: &TopicVar, enums: &BTreeMap<String, ResolvedEnum>) -> Vec<String> {
    let Some(e) = &v.enum_ref else {
        return Vec::new();
    };
    match enums.get(e) {
        None => vec![format!("var {}: undefined enum '{e}'", v.name)],
        Some(members) => members
            .iter()
            .filter_map(|(member, m)| code_rule(v, e, member, m.value))
            .collect(),
    }
}

/// Every var refusal of topic `t` in `tables` (its declared placeholders and enums).
///
/// v2: vars on sub topics, each one whole level.
/// v3: the list rules shared with declared placeholders; a placeholder's name refused.
/// v4: the enum a var names; the placeholders and enums taken from `tables`.
/// @version 4
/// @req REQ-ING-TOP-004
/// @req REQ-ING-TOP-006
/// @req REQ-ING-TOP-007
/// @req REQ-ING-TOP-008
pub fn check(t: &Topic, tables: &Tables, errors: &mut Errors) {
    let mut why = list_rules("var", &t.vars);
    for v in &t.vars {
        why.extend(var_rule(t, v, &tables.shadow.placeholders));
        why.extend(enum_rule(v, &tables.enums));
    }
    errors.extend(why.into_iter().map(|w| format!("topic {}: {w}", t.id)));
}

/// Every refusal of the declared placeholders: the list rules, one no topic uses, and one that
/// names an enum (only a topic var carries an enum's code).
///
/// v2: a placeholder naming an enum.
/// @version 2
/// @req REQ-ING-TOP-007
/// @req REQ-ING-TOP-008
pub fn check_placeholders(tables: &Tables, errors: &mut Errors) {
    let s = &tables.shadow;
    errors.extend(list_rules("placeholder", &s.placeholders));
    for p in &s.placeholders {
        if !s.topics.iter().any(|t| uses(&t.template, &p.name)) {
            errors.push(format!("placeholder {} is used by no topic", p.name));
        }
        if p.enum_ref.is_some() {
            errors.push(format!(
                "placeholder {} names an enum: only a topic var may",
                p.name
            ));
        }
    }
}
