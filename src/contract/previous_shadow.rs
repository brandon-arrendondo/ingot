//! Within a major the shadow wire mapping only grows. A key that was mapped keeps
//! every path entry (form, scope, variants) and every wire attribute, and its accept aliases
//! only grow; every echo row, constant row, topic and command row of the previous contract is
//! still there unchanged. Additions are a minor.

use super::keys::Errors;
use super::payload;
use super::refuse_commands::selector_text;
use super::schema::{PathForm, Rejected, ShadowCommand, Topic, TopicVar};
use super::snapshot::{KeyJson, Mapping, Snapshot};
use super::wire::KeyWire;

/// The path entries of a mapping, each with its form (an entry written as a bare path is a
/// plain form).
///
/// @version 1
/// @req REQ-ING-SHD-006
fn entries(w: &KeyWire) -> Vec<PathForm> {
    if !w.shadow_forms.is_empty() {
        return w.shadow_forms.clone();
    }
    w.shadow
        .iter()
        .map(|p| PathForm {
            path: p.clone(),
            form: None,
            scope: None,
            variants: None,
        })
        .collect()
}

/// Whether every accept alias of `old` is still accepted for the same code in `new`.
///
/// @version 1
/// @req REQ-ING-SHD-006
fn accepts_kept(old: &Mapping, new: &Mapping) -> bool {
    let empty = Default::default();
    let new_accept = new.enum_accept.as_ref().unwrap_or(&empty);
    old.enum_accept.iter().flatten().all(|(code, aliases)| {
        let now = new_accept.get(code);
        aliases.iter().all(|a| now.is_some_and(|n| n.contains(a)))
    })
}

/// Whether a key's wire attributes other than its paths are unchanged.
///
/// @version 1
/// @req REQ-ING-SHD-006
fn attributes_kept(o: &KeyWire, n: &KeyWire) -> bool {
    (&o.applies, &o.apply_group, &o.implies, &o.on_reject)
        == (&n.applies, &n.apply_group, &n.implies, &n.on_reject)
        && (&o.inbound_unknown, &o.outbound_unknown, o.forced)
            == (&n.inbound_unknown, &n.outbound_unknown, n.forced)
        && (&o.render, &o.telem) == (&n.render, &n.telem)
}

/// A key that was mapped keeps its path entries, attributes and accept aliases (paths and
/// aliases may grow); a key that was not mapped may gain a mapping.
///
/// @version 1
/// @req REQ-ING-SHD-006
pub fn key_mapping_kept(old: &KeyJson, new: &KeyJson, errors: &mut Errors) {
    let (o, n) = (&old.limits.mapping, &new.limits.mapping);
    if o.wire.shadow.is_empty() {
        return;
    }
    let now = entries(&n.wire);
    let paths = entries(&o.wire).iter().all(|e| now.contains(e));
    if !(paths && attributes_kept(&o.wire, &n.wire) && accepts_kept(o, n)) {
        errors.push(format!(
            "{}: its shadow mapping changed in a minor (paths and accept aliases may only grow; \
             forms, scopes, variants and wire attributes are frozen)",
            old.path
        ));
    }
}

/// Every row of `old` is still in `new` unchanged; `name` names a row in the refusal.
///
/// @version 1
/// @req REQ-ING-SHD-006
fn rows_kept<T: PartialEq>(old: &[T], new: &[T], name: impl Fn(&T) -> String, errors: &mut Errors) {
    for row in old.iter().filter(|r| !new.contains(r)) {
        errors.push(format!(
            "{} changed or was removed in a minor (a major)",
            name(row)
        ));
    }
}

/// `shadow_commands <topic> <selector>`, a command row's name.
///
/// @version 1
/// @req REQ-ING-SHD-006
fn command_name(r: &ShadowCommand) -> String {
    format!("shadow_commands {} {}", r.topic, selector_text(&r.selector))
}

/// The topics of `rows` without their variants and their vars' enums: a topic is compared on
/// the snapshot's variant, so a minor may add variants to it, and a var's enum is compared by
/// `var_enums_kept`, so a minor may name one for the first time.
///
/// v2: without the vars' enums.
/// @version 2
/// @req REQ-ING-TOP-005
/// @req REQ-ING-TOP-008
fn topics_on_variant(rows: &[Topic]) -> Vec<Topic> {
    let plain = |v: &TopicVar| TopicVar {
        enum_ref: None,
        ..v.clone()
    };
    rows.iter()
        .map(|t| Topic {
            variants: None,
            vars: t.vars.iter().map(plain).collect(),
            ..t.clone()
        })
        .collect()
}

/// Every var of `old` that named an enum still names it in `new` (a topic or var that is gone
/// is refused with its topic row).
///
/// @version 1
/// @req REQ-ING-TOP-008
fn var_enums_kept(old: &[Topic], new: &[Topic], errors: &mut Errors) {
    for t in old {
        let now = new.iter().find(|n| n.id == t.id).map(|n| &n.vars);
        for v in t.vars.iter().filter(|v| v.enum_ref.is_some()) {
            let kept = now
                .into_iter()
                .flatten()
                .find(|n| n.name == v.name)
                .is_none_or(|n| n.enum_ref == v.enum_ref);
            if !kept {
                errors.push(format!(
                    "topic {}: var {} changed or dropped its enum in a minor (a major)",
                    t.id, v.name
                ));
            }
        }
    }
}

/// The contract-level rows of `prev` are all still in `cur` unchanged; the rejected-settings
/// list by its path and topics without their variants, on the snapshot's variant (both may gain
/// variants).
///
/// v2: topic payload members.
/// v3: the rejected-settings list.
/// v4: the list compared by path, so a minor may add variants.
/// v5: topics compared without their variants.
/// v6: the declared placeholders.
/// v7: the vars' enums, by `var_enums_kept`.
/// @version 7
/// @req REQ-ING-SHD-006
/// @req REQ-ING-TOP-003
/// @req REQ-ING-SHD-011
/// @req REQ-ING-TOP-005
/// @req REQ-ING-TOP-007
/// @req REQ-ING-TOP-008
pub fn rows_unchanged(prev: &Snapshot, cur: &Snapshot, errors: &mut Errors) {
    let (old, new) = match (prev.shadow_rows(), cur.shadow_rows()) {
        (Ok(o), Ok(n)) => (o, n),
        (Err(e), _) | (_, Err(e)) => return errors.push(e),
    };
    rows_kept(
        &old.echo,
        &new.echo,
        |e| format!("shadow_echo {}", e.path),
        errors,
    );
    rows_kept(
        &old.constants,
        &new.constants,
        |c| format!("shadow_constant {}", c.path),
        errors,
    );
    rows_kept(
        &topics_on_variant(&old.topics),
        &topics_on_variant(&new.topics),
        |t| format!("topic {}", t.id),
        errors,
    );
    var_enums_kept(&old.topics, &new.topics, errors);
    rows_kept(&old.commands, &new.commands, command_name, errors);
    rows_kept(&old.payload, &new.payload, payload::name, errors);
    let path = |s: &Option<Rejected>| s.iter().map(|r| r.path.clone()).collect::<Vec<_>>();
    rows_kept(
        &path(&old.rejected),
        &path(&new.rejected),
        |p| format!("shadow_rejected {p}"),
        errors,
    );
    rows_kept(
        &old.placeholders,
        &new.placeholders,
        |p| format!("placeholder {}", p.name),
        errors,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{Implies, OnReject, Render, RenderKind};

    /// @brief A mapping with every wire attribute set.
    /// @version 1
    fn full() -> KeyWire {
        KeyWire {
            shadow: vec!["state.a".into()],
            applies: Some("ns.settings.a".into()),
            apply_group: Some("g".into()),
            implies: vec![Implies {
                key: "ns.settings.b".into(),
                value: toml::Value::Integer(1),
            }],
            on_reject: Some(OnReject::Echo),
            inbound_unknown: Some("x".into()),
            outbound_unknown: Some("y".into()),
            forced: true,
            render: Some(Render {
                kind: RenderKind::CsvIndexList,
                enum_ref: "e".into(),
                max_items: 2,
            }),
            telem: Some("t".into()),
            ..KeyWire::default()
        }
    }

    #[test]
    fn every_wire_attribute_is_frozen() {
        let base = full();
        assert!(attributes_kept(&base, &base));
        let changes: [fn(&mut KeyWire); 9] = [
            |w| w.applies = None,
            |w| w.apply_group = None,
            |w| w.implies.clear(),
            |w| w.on_reject = Some(OnReject::ClearDesired),
            |w| w.inbound_unknown = None,
            |w| w.outbound_unknown = None,
            |w| w.forced = false,
            |w| w.render = None,
            |w| w.telem = None,
        ];
        for (i, change) in changes.iter().enumerate() {
            let mut w = full();
            change(&mut w);
            assert!(!attributes_kept(&base, &w), "attribute {i}");
        }
    }

    #[test]
    fn a_bare_path_is_a_plain_entry_and_aliases_only_grow() {
        let bare = full();
        let plain = entries(&bare);
        assert_eq!(plain.len(), 1);
        assert!(plain[0].plain() && plain[0].path == "state.a");
        let aliases = |v: &[&str]| Mapping {
            enum_accept: Some([(2, v.iter().map(|s| s.to_string()).collect())].into()),
            wire: KeyWire::default(),
        };
        assert!(accepts_kept(&aliases(&["a"]), &aliases(&["a", "b"])));
        assert!(!accepts_kept(&aliases(&["a", "b"]), &aliases(&["a"])));
        assert!(!accepts_kept(&aliases(&["a"]), &Mapping::default()));
    }
}
