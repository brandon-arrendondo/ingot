//! Principals and their resolved rights on every key.

use super::keys::{Errors, Key};
use super::schema::{Access, Caps, Grant, Kind, PrincipalDef};
use super::types::Version;
use std::collections::BTreeSet;

/// Read right in a rights cell.
pub const READ: u8 = 0x1;
/// Write right in a rights cell.
pub const WRITE: u8 = 0x2;

/// One resolved principal: a row of `dmt_principals`.
#[derive(Debug)]
pub struct Principal {
    pub name: String,
    pub uid: u32,
    pub token: u16,
    pub kind: Kind,
    pub flags: u8,
    pub heartbeat: Option<String>,
    pub heartbeat_window_ms: u32,
    pub latency_budget_ms: u32,
    pub caps: Caps,
    pub resync_k: u16,
    pub resync_t_ms: u32,
    pub staging_bound_bytes: u32,
    pub introduced: Version,
}

/// Resolve the principal declarations, sorted by (kind, uid, token).
///
/// @version 1
/// @req REQ-ING-TIV-006
pub fn principals(
    defs: Vec<PrincipalDef>,
    errors: &mut Errors,
) -> (Vec<Principal>, Vec<Vec<Grant>>) {
    let mut rows: Vec<(Principal, Vec<Grant>)> =
        defs.into_iter().map(|def| principal(def, errors)).collect();
    rows.sort_by_key(|(p, _)| (p.kind, p.uid, p.token));
    rows.into_iter().unzip()
}

/// Resolve one principal declaration.
///
/// @version 1
/// @req REQ-ING-TIV-006
fn principal(def: PrincipalDef, errors: &mut Errors) -> (Principal, Vec<Grant>) {
    let introduced = Version::parse(&def.introduced_in).unwrap_or_else(|e| {
        errors.push(format!("principal {}: introduced_in {e}", def.name));
        Version { major: 0, minor: 0 }
    });
    let flags = def.flags.iter().fold(0, |acc, f| acc | f.bit());
    let p = Principal {
        name: def.name,
        uid: def.uid,
        token: def.token,
        kind: def.kind,
        flags,
        heartbeat: def.heartbeat,
        heartbeat_window_ms: def.heartbeat_window_ms,
        latency_budget_ms: def.latency_budget_ms,
        caps: def.caps,
        resync_k: def.resync_k,
        resync_t_ms: def.resync_t_ms,
        staging_bound_bytes: def.staging_bound_bytes,
        introduced,
    };
    (p, def.grants)
}

/// Rights of every principal on every key, `[principal][key]`: grants on a namespace or a
/// (namespace, class) are combined, a grant on one key replaces them, and read on a secret
/// belongs to its readers only.
///
/// @version 1
/// @req REQ-ING-TIV-006
pub fn resolve(
    principals: &[Principal],
    grants: &[Vec<Grant>],
    keys: &[Key],
    errors: &mut Errors,
) -> Vec<Vec<u8>> {
    principals
        .iter()
        .zip(grants)
        .map(|(p, g)| keys.iter().map(|k| cell(p, g, k, errors)).collect())
        .collect()
}

/// The rights of principal `p` on key `k`.
///
/// v2: no separate key-grant exclusion from the scope fold: a key grant has no namespace
/// (check_grants refuses one with both), so scope_matches never matches it.
/// @version 2
/// @req REQ-ING-TIV-006
/// @req REQ-ING-TIV-011
fn cell(p: &Principal, grants: &[Grant], k: &Key, errors: &mut Errors) -> u8 {
    let mut bits = grants
        .iter()
        .filter(|g| scope_matches(g, &k.path))
        .fold(0, |acc, g| acc | access_bits(g.access));
    if let Some(g) = grants
        .iter()
        .rev()
        .find(|g| g.key.as_deref() == Some(&k.path))
    {
        bits = access_bits(g.access);
        if k.secret && bits & READ != 0 && !k.readers.contains(&p.name) {
            errors.push(format!(
                "{}: read granted to {}, which is not on the secret's reader list",
                k.path, p.name
            ));
        }
    }
    if k.secret {
        bits = (bits & WRITE) | if k.readers.contains(&p.name) { READ } else { 0 };
    }
    bits
}

/// Whether a namespace or (namespace, class) grant covers the key at `path`.
///
/// @version 1
/// @req REQ-ING-TIV-006
fn scope_matches(grant: &Grant, path: &str) -> bool {
    let mut parts = path.split('.');
    let (ns, class) = (parts.next(), parts.next());
    grant.namespace.as_deref() == ns && grant.class.as_deref().is_none_or(|c| Some(c) == class)
}

/// The rights bits an access grants.
///
/// @version 1
/// @req REQ-ING-TIV-006
fn access_bits(access: Access) -> u8 {
    match access {
        Access::None => 0,
        Access::R => READ,
        Access::W => WRITE,
        Access::Rw => READ | WRITE,
    }
}

/// Refuse grants that name nothing, a class without its namespace, both a key and a scope,
/// or a key or scope that no key of the model has. `known` is every key path on any variant,
/// so a grant naming a key that exists on another variant only is accepted (it grants
/// nothing here).
///
/// v2: a class-only grant and a grant whose scope covers no key are refused (both matched
/// nothing in silence); the per-grant check moved into grant_error.
/// v3: checked against every variant's key paths (per-variant presence).
/// @version 3
/// @req REQ-ING-TIV-002
/// @req REQ-ING-PRS-001
pub fn check_grants(
    principals: &[Principal],
    grants: &[Vec<Grant>],
    known: &BTreeSet<String>,
    errors: &mut Errors,
) {
    for (p, list) in principals.iter().zip(grants) {
        for why in list.iter().filter_map(|g| grant_error(g, known)) {
            errors.push(format!("principal {}: {why}", p.name));
        }
    }
}

/// Why grant `g` is refused, if it is.
///
/// v2: against the model's key paths on any variant.
/// @version 2
/// @req REQ-ING-TIV-002
fn grant_error(g: &Grant, known: &BTreeSet<String>) -> Option<String> {
    match (&g.key, &g.namespace, &g.class) {
        (Some(path), None, None) => {
            (!known.contains(path)).then(|| format!("grant on unknown key {path}"))
        }
        (None, Some(ns), class) => (!known.iter().any(|p| scope_matches(g, p))).then(|| {
            format!(
                "grant on {ns}{}{} covers no key",
                if class.is_some() { "." } else { "" },
                class.as_deref().unwrap_or("")
            )
        }),
        _ => Some("a grant names either a key or a namespace (and class)".to_string()),
    }
}

/// Pack one principal's rights row in the TIV layout: key index i in byte i / 4, read at
/// bit 2*(i % 4), write at bit 2*(i % 4) + 1.
///
/// @version 1
/// @req REQ-ING-TIV-006
pub fn pack(row: &[u8]) -> Vec<u8> {
    let mut packed = vec![0u8; row.len().div_ceil(4)];
    for (i, bits) in row.iter().enumerate() {
        packed[i / 4] |= (bits & 3) << ((i % 4) * 2);
    }
    packed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_follows_the_tiv_layout() {
        let row = [READ, WRITE, 0, READ | WRITE, WRITE];
        assert_eq!(pack(&row), vec![0x01 | (0x02 << 2) | (0x03 << 6), 0x02]);
        assert!(pack(&[]).is_empty());
    }

    #[test]
    fn scopes_match_namespace_and_class() {
        let g = |ns: &str, class: Option<&str>| Grant {
            namespace: Some(ns.into()),
            class: class.map(Into::into),
            key: None,
            access: Access::R,
        };
        assert!(scope_matches(&g("drive", None), "drive.state.speed"));
        assert!(scope_matches(
            &g("drive", Some("state")),
            "drive.state.speed"
        ));
        assert!(!scope_matches(
            &g("drive", Some("settings")),
            "drive.state.speed"
        ));
        assert!(!scope_matches(&g("driver", None), "drive.state.speed"));
    }
}
