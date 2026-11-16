//! The shadow wire attributes of one key, resolved from its declaration; they are
//! written into contract.json and the keyspace files and never enter the store tables.

use super::enums::ResolvedEnum;
use super::keys::{ClassCtx, Errors};
use super::model::present_on;
use super::schema::{Implies, KeyDef, OnReject, PathForm, Render, Role};
use serde::{Deserialize, Serialize};

/// A key's shadow paths and how the binder and reporter treat them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyWire {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shadow: Vec<String>,
    /// Every path with its form and scope; written only when one declares either.
    #[serde(skip_serializing_if = "all_plain")]
    pub shadow_forms: Vec<PathForm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applies: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apply_group: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub implies: Vec<Implies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_reject: Option<OnReject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbound_unknown: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbound_unknown: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub forced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render: Option<Render>,
    /// The enum a rendered list indexes, resolved for the variant.
    #[serde(skip)]
    pub render_values: Option<ResolvedEnum>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telem: Option<String>,
}

/// Whether no path declares a form, a scope or variants (the forms are then left out of the
/// outputs).
///
/// @version 1
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-004
fn all_plain(forms: &[PathForm]) -> bool {
    forms.iter().all(PathForm::plain)
}

/// The paths of `forms`, in order, each once (entries for disjoint variants may share one).
///
/// v2: each once.
/// @version 2
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-004
fn path_names(forms: &[PathForm]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for f in forms {
        if !names.contains(&f.path) {
            names.push(f.path.clone());
        }
    }
    names
}

/// The declared paths of `def` that exist on `variant` (every path for the all-variants build).
///
/// @version 1
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-004
fn paths_on(def: &KeyDef, variant: Option<&str>) -> Vec<PathForm> {
    let all = def.shadow.as_ref().map(|p| p.list()).unwrap_or_default();
    all.into_iter()
        .filter(|f| present_on(f.variants.as_deref(), variant))
        .collect()
}

/// The wire attributes `def` declares; a shadowed SETTINGS key rejects by echo unless it says
/// otherwise, and a rendered list's enum is resolved from the key's namespace.
///
/// v2: each path's form and scope.
/// v3: only the paths of the variant.
/// @version 3
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-002
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-004
pub fn of(ctx: &ClassCtx, path: &str, def: &KeyDef, errors: &mut Errors) -> KeyWire {
    let shadow_forms = paths_on(def, ctx.variant);
    let shadow = path_names(&shadow_forms);
    let echo_default = ctx.class.role == Role::Settings && !shadow.is_empty();
    let on_reject = def.on_reject.or(echo_default.then_some(OnReject::Echo));
    let render_values = def
        .render
        .as_ref()
        .and_then(|r| render_enum(ctx, path, r, errors));
    KeyWire {
        shadow,
        shadow_forms,
        applies: def.applies.clone(),
        apply_group: def.apply_group.clone(),
        implies: def.implies.clone(),
        on_reject,
        inbound_unknown: def.inbound_unknown.clone(),
        outbound_unknown: def.outbound_unknown.clone(),
        forced: def.forced,
        render: def.render.clone(),
        render_values,
        telem: def.telem.clone(),
    }
}

/// The enum of the key's namespace a rendered list indexes, recording an error for an
/// undefined one.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn render_enum(
    ctx: &ClassCtx,
    path: &str,
    render: &Render,
    errors: &mut Errors,
) -> Option<ResolvedEnum> {
    let found = ctx.enums.get(&render.enum_ref).cloned();
    if found.is_none() {
        errors.push(format!("{path}: undefined enum '{}'", render.enum_ref));
    }
    found
}
