//! Declared variants: when contract.toml lists its variants, --variant and every per-variant
//! entry of the namespace files must name one of them.

use super::keys::Errors;
use super::refuse_commands::selector_text;
use super::schema::{ClassDef, KeyDef, NamespaceFile};

/// Refuse an undeclared or missing `--variant`. A model that declares no variants accepts any.
///
/// v2: the per-variant entries are checked by check_entries.
/// @version 2
/// @req REQ-ING-TIV-015
pub fn check_selected(declared: &[String], variant: Option<&str>, errors: &mut Errors) {
    if declared.is_empty() {
        return;
    }
    let list = declared.join(", ");
    match variant {
        Some(v) if declared.iter().any(|d| d == v) => {}
        Some(v) => errors.push(format!("--variant {v} is not declared (declared: {list})")),
        None => errors.push(format!(
            "the model declares variants; --variant must name one of: {list}"
        )),
    }
}

/// Refuse any per-variant entry of the namespace files, or of the contract-level rows
/// (`contract_uses`), for an undeclared variant.
///
/// v2: the contract-level rows.
/// @version 2
/// @req REQ-ING-TIV-015
/// @req REQ-ING-SHD-004
pub fn check_entries(
    declared: &[String],
    namespaces: &[NamespaceFile],
    contract_uses: &[(String, String)],
    errors: &mut Errors,
) {
    if declared.is_empty() {
        return;
    }
    let list = declared.join(", ");
    for (path, v) in uses(namespaces)
        .into_iter()
        .chain(contract_uses.iter().cloned())
    {
        if !declared.contains(&v) {
            errors.push(format!(
                "{path}: variant {v} is not declared (declared: {list})"
            ));
        }
    }
}

/// Every (where, variant) a namespace file names: per-variant defaults, enum variants and the
/// variants keys and commands are limited to.
///
/// v2: key and command `variants` lists.
/// v3: shadow command rows' `variants` lists.
/// @version 3
/// @req REQ-ING-TIV-015
/// @req REQ-ING-PRS-001
/// @req REQ-ING-CMD-001
fn uses(namespaces: &[NamespaceFile]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for ns in namespaces {
        for (name, def) in &ns.enums {
            let path = format!("{}::{name}", ns.meta.id);
            out.extend(def.variants.keys().map(|v| (path.clone(), v.clone())));
        }
        for r in &ns.shadow_commands {
            let path = format!("shadow_commands {} {}", r.topic, selector_text(&r.selector));
            out.extend(
                r.variants
                    .iter()
                    .flatten()
                    .map(|v| (path.clone(), v.clone())),
            );
        }
        for class in &ns.classes {
            class_uses(&ns.meta.id, class, &mut out);
        }
    }
    out
}

/// Every variant a key's shadow paths are limited to.
///
/// @version 1
/// @req REQ-ING-SHD-004
fn path_variants(key: &KeyDef) -> Vec<String> {
    let paths = key.shadow.as_ref().map(|p| p.list()).unwrap_or_default();
    paths
        .into_iter()
        .flat_map(|f| f.variants)
        .flatten()
        .collect()
}

/// The (where, variant) pairs of one class: per-variant defaults, key, shadow path and command
/// limits.
///
/// v2: shadow path limits.
/// @version 2
/// @req REQ-ING-TIV-015
/// @req REQ-ING-PRS-001
/// @req REQ-ING-SHD-004
fn class_uses(ns: &str, class: &ClassDef, out: &mut Vec<(String, String)>) {
    let path = |id: &str| format!("{ns}.{}.{id}", class.id);
    for key in &class.keys {
        let on_paths = path_variants(key);
        let named = key.defaults.keys().chain(key.variants.iter().flatten());
        out.extend(named.chain(&on_paths).map(|v| (path(&key.id), v.clone())));
    }
    for cmd in &class.commands {
        out.extend(
            cmd.variants
                .iter()
                .flatten()
                .map(|v| (path(&cmd.id), v.clone())),
        );
    }
}
