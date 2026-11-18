//! Refusals of the shadow wire mapping of keys and rows.

use super::enums::ResolvedEnum;
use super::keys::{Errors, Key};
use super::model::{variants_overlap, Tables};
use super::schema::{scopes_meet, EchoType, KeyType, PathForm, Role, WireForm};
use super::value::encode_default;
use std::collections::BTreeSet;

/// Every key and row refusal of the mapping.
///
/// v2: the path forms.
/// @version 2
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-002
/// @req REQ-ING-SHD-003
pub fn check(tables: &Tables, errors: &mut Errors) {
    for key in &tables.keys {
        placement(key, errors);
        form_rules(key, errors);
        enum_rules(key, errors);
        group_rules(tables, key, errors);
        applies_rule(tables, key, errors);
        twin_rule(tables, key, errors);
    }
    telem_names(tables, errors);
    rows(tables, errors);
}

/// Whether the key has a shadow path and the given role.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn shadowed(key: &Key, role: Role) -> bool {
    key.role == role && !key.wire.shadow.is_empty()
}

/// Whether a path is listed twice for one variant and one document (entries limited to disjoint
/// variants, or to disjoint scopes, may share a path, each with its own form).
///
/// v2: entries of disjoint scopes share a path.
/// @version 2
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-004
/// @req REQ-ING-SHD-010
fn listed_twice(forms: &[PathForm]) -> bool {
    forms.iter().enumerate().any(|(i, a)| {
        forms[i + 1..].iter().any(|b| {
            a.path == b.path
                && variants_overlap(a.variants.as_deref(), b.variants.as_deref())
                && scopes_meet(a.scope, b.scope)
        })
    })
}

/// Shadow paths on SETTINGS, STATE and CONFIG keys of a JSON-valued type, each listed once per
/// variant.
///
/// v2: once per variant.
/// v3: CONFIG keys are reported too.
/// @version 3
/// @req REQ-ING-SHD-008
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-004
fn placement(key: &Key, errors: &mut Errors) {
    let w = &key.wire;
    let mut refuse = |why: &str| errors.push(format!("{}: {why}", key.path));
    let mapped = !w.shadow.is_empty();
    if mapped && !(key.role == Role::Settings || key.role.reported()) {
        refuse("a shadow path is for SETTINGS, STATE and CONFIG keys");
    }
    if mapped && matches!(key.ty, KeyType::Binary | KeyType::Struct) {
        refuse("a binary or struct key has no shadow value");
    }
    if listed_twice(&w.shadow_forms) {
        refuse("a shadow path is listed twice");
    }
    attribute_placement(key, errors);
}

/// Forced and render on shadowed STATE keys (render on a string, with items), on_reject on
/// shadowed SETTINGS keys.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn attribute_placement(key: &Key, errors: &mut Errors) {
    let w = &key.wire;
    let mut refuse = |why: &str| errors.push(format!("{}: {why}", key.path));
    if w.forced && !shadowed(key, Role::State) {
        refuse("forced is for shadowed STATE keys");
    }
    let render_ok = shadowed(key, Role::State) && key.ty == KeyType::String;
    if w.render
        .as_ref()
        .is_some_and(|r| !render_ok || r.max_items == 0)
    {
        refuse("render is for shadowed STATE string keys, with max_items > 0");
    }
    if w.on_reject.is_some() && !shadowed(key, Role::Settings) {
        refuse("on_reject is for shadowed SETTINGS keys");
    }
}

/// The names of an enum's members whose wire string was not declared.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn implicit_wires(e: &ResolvedEnum) -> Vec<&String> {
    e.iter()
        .filter(|(_, m)| !m.explicit)
        .map(|(n, _)| n)
        .collect()
}

/// Why the key's `as` attributes are refused: `as` is for enum keys, and for bool keys only as
/// `"integer"` on a reported path (inbound a bool stays a JSON bool).
///
/// @version 1
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-010
fn as_rule(key: &Key) -> Option<&'static str> {
    let forms: Vec<WireForm> = key
        .wire
        .shadow_forms
        .iter()
        .filter_map(|f| f.form)
        .collect();
    if forms.is_empty() || key.enum_values.is_some() {
        None
    } else if key.ty != KeyType::Bool {
        Some("as is for enum and bool keys")
    } else if forms.contains(&WireForm::Enum) {
        Some("as = \"enum\" is for enum keys")
    } else if !key.role.reported() {
        Some("as = \"integer\" on a bool key is for reported paths")
    } else {
        None
    }
}

/// The path forms: `as` by `as_rule`, `scope` for reported paths (STATE and CONFIG), and a
/// forced key has a path in the delta.
///
/// v2: CONFIG keys are reported too.
/// v3: the `as` rule in `as_rule` (bool keys).
/// @version 3
/// @req REQ-ING-SHD-008
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-010
fn form_rules(key: &Key, errors: &mut Errors) {
    let forms = &key.wire.shadow_forms;
    let mut refuse = |why: &str| errors.push(format!("{}: {why}", key.path));
    if let Some(why) = as_rule(key) {
        refuse(why);
    }
    if !key.role.reported() && forms.iter().any(|f| f.scope.is_some()) {
        refuse("scope is for reported paths of STATE and CONFIG keys");
    }
    if key.wire.forced && !forms.iter().any(PathForm::in_delta) {
        refuse("forced needs a path in the delta");
    }
}

/// Whether the key has a path that carries its enum's wire strings.
///
/// @version 1
/// @req REQ-ING-SHD-003
fn wire_strings(key: &Key) -> bool {
    key.wire.shadow_forms.iter().any(|f| !f.integer())
}

/// A shadowed key's enum where a path carries its wire strings (and a rendered
/// list's) has explicit wire strings; such a SETTINGS enum key declares inbound_unknown and
/// such a reported (STATE or CONFIG) enum key outbound_unknown, and nothing else declares either.
///
/// v2: only where a path carries the wire strings (an integer path carries the code).
/// v3: CONFIG keys are reported too.
/// @version 3
/// @req REQ-ING-SHD-008
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-003
fn enum_rules(key: &Key, errors: &mut Errors) {
    let strings = wire_strings(key);
    let enums = [
        key.enum_values.as_ref().filter(|_| strings),
        key.wire.render_values.as_ref(),
    ];
    for name in enums
        .iter()
        .filter(|_| !key.wire.shadow.is_empty())
        .filter_map(|e| *e)
        .flat_map(implicit_wires)
    {
        errors.push(format!(
            "{}: enum value {name} needs an explicit wire string on a shadowed key",
            key.path
        ));
    }
    let e = key.enum_values.as_ref().filter(|_| strings);
    let inbound = e.filter(|_| shadowed(key, Role::Settings));
    unknown_rule(
        key,
        "inbound_unknown",
        &key.wire.inbound_unknown,
        inbound,
        errors,
    );
    let outbound = e.filter(|_| key.role.reported() && !key.wire.shadow.is_empty());
    unknown_rule(
        key,
        "outbound_unknown",
        &key.wire.outbound_unknown,
        outbound,
        errors,
    );
}

/// One direction of a shadowed enum: `declared` is required exactly where `applies_to` is the key's enum,
/// and is `reject` or `ignore` (inbound) or `raw` (outbound), or a string of the enum that way.
///
/// v2: `ignore` inbound; the rule is for keys with a wire-string path.
/// @version 2
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-003
fn unknown_rule(
    key: &Key,
    attr: &str,
    declared: &Option<String>,
    applies_to: Option<&ResolvedEnum>,
    errors: &mut Errors,
) {
    let inbound = attr == "inbound_unknown";
    let why = match (applies_to, declared.as_deref()) {
        (None, Some(_)) => Some(format!(
            "{attr} is for shadowed {} enum keys with a wire-string path",
            role_of(inbound)
        )),
        (Some(_), None) => Some(format!("a shadowed enum key declares {attr}")),
        (Some(_), Some("index")) => Some(format!(
            "{attr} index (an unchecked read) must be resolved before generation"
        )),
        (Some(e), Some(v)) if !known_unknown(e, v, inbound) => Some(format!(
            "{attr} {v} is neither {} nor {} of the enum",
            if inbound { "reject, ignore" } else { "raw" },
            if inbound {
                "an inbound string"
            } else {
                "a wire string"
            }
        )),
        _ => None,
    };
    if let Some(why) = why {
        errors.push(format!("{}: {why}", key.path));
    }
}

/// The role an unknown rule is for.
///
/// v2: CONFIG keys are reported too.
/// @version 2
/// @req REQ-ING-SHD-008
/// @req REQ-ING-SHD-001
fn role_of(inbound: bool) -> &'static str {
    if inbound {
        "SETTINGS"
    } else {
        "STATE and CONFIG"
    }
}

/// Whether `v` is an accepted unknown rule: `reject`, `ignore` or an inbound string, or `raw`
/// or a wire string.
///
/// v2: `ignore` (the unknown string is dropped, nothing is written).
/// v3: shared with topic payload members.
/// @version 3
/// @req REQ-ING-TOP-003
/// @req REQ-ING-SHD-001
pub(super) fn known_unknown(e: &ResolvedEnum, v: &str, inbound: bool) -> bool {
    if inbound {
        matches!(v, "reject" | "ignore") || e.values().any(|m| m.inbound().any(|s| s == v))
    } else {
        v == "raw" || e.values().any(|m| m.wire == v)
    }
}

/// Apply groups on SETTINGS keys, declared; implies writes of other keys of the same
/// group, each value valid for its key (only a SETTINGS key may join a group, so the target is
/// one).
///
/// v2: no separate role check of the target (a STATE key in a group is refused already).
/// @version 2
/// @req REQ-ING-SHD-002
fn group_rules(tables: &Tables, key: &Key, errors: &mut Errors) {
    let w = &key.wire;
    let mut refuse = |why: String| errors.push(format!("{}: {why}", key.path));
    if let Some(g) = &w.apply_group {
        if key.role != Role::Settings {
            refuse("apply_group is for SETTINGS keys".into());
        }
        if !tables.shadow.apply_groups.iter().any(|d| &d.id == g) {
            refuse(format!("apply group {g} is not declared"));
        }
    }
    for imp in &w.implies {
        let target = tables.key_index(&imp.key).map(|i| &tables.keys[i]);
        let same = target.filter(|t| {
            t.path != key.path && w.apply_group.is_some() && t.wire.apply_group == w.apply_group
        });
        match same {
            None => refuse(format!(
                "implies {} is outside the key's apply group",
                imp.key
            )),
            Some(t) => {
                let enc = encode_default(t.ty, &imp.value, t.max_size, t.enum_values.as_ref());
                if let Err(e) = enc {
                    refuse(format!("implies {}: {e}", imp.key));
                }
            }
        }
    }
}

/// `applies` is on a STATE key and names a SETTINGS key of the same type and enum.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn applies_rule(tables: &Tables, key: &Key, errors: &mut Errors) {
    let Some(target) = &key.wire.applies else {
        return;
    };
    let ok = key.role == Role::State
        && tables.key_index(target).is_some_and(|i| {
            let t = &tables.keys[i];
            t.role == Role::Settings && t.ty == key.ty && t.enum_name == key.enum_name
        });
    if !ok {
        errors.push(format!(
            "{}: applies must be on a STATE key and name a SETTINGS key of the same type and enum",
            key.path
        ));
    }
}

/// A shadowed SETTINGS key has exactly one reported STATE twin that applies it.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn twin_rule(tables: &Tables, key: &Key, errors: &mut Errors) {
    if !shadowed(key, Role::Settings) {
        return;
    }
    let twins = tables
        .keys
        .iter()
        .filter(|t| shadowed(t, Role::State) && t.wire.applies.as_deref() == Some(&key.path))
        .count();
    if twins != 1 {
        errors.push(format!(
            "{}: a shadowed SETTINGS key needs one reported STATE twin that applies it (has {twins})",
            key.path
        ));
    }
}

/// Telemetry names are non-empty and unique.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn telem_names(tables: &Tables, errors: &mut Errors) {
    let mut seen = BTreeSet::new();
    for key in &tables.keys {
        if let Some(t) = &key.wire.telem {
            if t.is_empty() || !seen.insert(t) {
                errors.push(format!(
                    "{}: telem name '{t}' is empty or used twice",
                    key.path
                ));
            }
        }
    }
}

/// The rows: apply group ids unique; an echo string has a max_size and only it;
/// an echo's command key is a SETTINGS key; a constant has a scalar value or an image; a
/// rejected-settings list needs a desired path to reject.
///
/// v2: the rejected-settings list.
/// @version 2
/// @req REQ-ING-SHD-002
/// @req REQ-ING-SHD-011
fn rows(tables: &Tables, errors: &mut Errors) {
    let s = &tables.shadow;
    let mut ids = BTreeSet::new();
    for g in s.apply_groups.iter().filter(|g| !ids.insert(&g.id)) {
        errors.push(format!("apply group {} is declared twice", g.id));
    }
    echo_rows(tables, errors);
    constant_rows(tables, errors);
    let desired = tables.keys.iter().any(|k| shadowed(k, Role::Settings));
    if s.rejected.is_some() && !desired {
        errors.push("shadow_rejected: the model has no desired path to reject".into());
    }
}

/// An echo string has a max_size above 0 and only a string has one; a command key is a
/// SETTINGS key.
///
/// @version 1
/// @req REQ-ING-SHD-002
fn echo_rows(tables: &Tables, errors: &mut Errors) {
    let settings = |p: &String| {
        tables
            .key_index(p)
            .is_some_and(|i| tables.keys[i].role == Role::Settings)
    };
    for e in &tables.shadow.echo {
        if (e.ty == EchoType::String) != e.max_size.is_some() || e.max_size == Some(0) {
            errors.push(format!(
                "shadow_echo {}: a string echo needs max_size > 0, and only it has one",
                e.path
            ));
        }
        if e.command_key.as_ref().is_some_and(|p| !settings(p)) {
            errors.push(format!(
                "shadow_echo {}: command_key must name a SETTINGS key",
                e.path
            ));
        }
    }
}

/// A constant has a bool, number or string value, or an image, not both.
///
/// @version 1
/// @req REQ-ING-SHD-002
fn constant_rows(tables: &Tables, errors: &mut Errors) {
    for c in &tables.shadow.constants {
        let scalar = c.value.as_ref().map(|v| {
            matches!(
                v,
                toml::Value::Boolean(_)
                    | toml::Value::Integer(_)
                    | toml::Value::Float(_)
                    | toml::Value::String(_)
            )
        });
        if scalar.is_some() == c.image.is_some() || scalar == Some(false) {
            errors.push(format!(
                "shadow_constant {}: declares a bool, number or string value, or an image",
                c.path
            ));
        }
    }
}
