//! Rights refusals: secrets, transports, aliases, reset/MFG/trace writes, heartbeats,
//! core-written keys and command owners.

use super::keys::Errors;
use super::model::Tables;
use super::rights::{READ, WRITE};
use super::schema::{KeyType, Kind};
use super::types::{wk_index, OWNER_CORE, WK_RULES};

/// Run every rights check.
///
/// @version 1
/// @req REQ-ING-TIV-011
pub fn check(tables: &Tables, errors: &mut Errors) {
    for p in 0..tables.principals.len() {
        for k in 0..tables.keys.len() {
            transport(tables, p, k, errors);
            alias(tables, p, k, errors);
        }
        uart_writes(tables, p, errors);
    }
    heartbeats(tables, errors);
    core_written(tables, errors);
    trace_enable_writers(tables, errors);
    command_owners(tables, errors);
}

/// No secret over UART; no struct key for a UART or in-process principal.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn transport(tables: &Tables, p: usize, k: usize, errors: &mut Errors) {
    let (principal, key) = (&tables.principals[p], &tables.keys[k]);
    if tables.rights[p][k] == 0 {
        return;
    }
    let refused = match principal.kind {
        Kind::Uart => key.secret || key.ty == KeyType::Struct,
        Kind::Inproc => key.ty == KeyType::Struct,
        Kind::Unix => false,
    };
    if refused {
        errors.push(format!(
            "{}: {:?} principal {} may not have rights on a {} key",
            key.path,
            principal.kind,
            principal.name,
            if key.secret { "secret" } else { "struct" }
        ));
    }
}

/// An alias has exactly its target's rights.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn alias(tables: &Tables, p: usize, k: usize, errors: &mut Errors) {
    let target = tables.keys[k]
        .alias_of
        .as_deref()
        .and_then(|t| tables.key_index(t));
    if let Some(t) = target {
        if tables.rights[p][k] != tables.rights[p][t] {
            errors.push(format!(
                "{}: principal {} has different rights on the alias and its target",
                tables.keys[k].path, tables.principals[p].name
            ));
        }
    }
}

/// A UART principal writes no reset command, MFG key or trace-enable command.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn uart_writes(tables: &Tables, p: usize, errors: &mut Errors) {
    if tables.principals[p].kind != Kind::Uart {
        return;
    }
    let sensitive = ["RESET_REQUEST", "RESET_PRE_COMMAND", "TRACE_ENABLE"].map(wk_index);
    for (k, key) in tables.keys.iter().enumerate() {
        if tables.writes(p, k) && (key.mfg || sensitive.contains(&key.wk)) {
            errors.push(format!(
                "{}: UART principal {} may not write reset, MFG or trace-enable keys",
                key.path, tables.principals[p].name
            ));
        }
    }
}

/// A heartbeat key belongs to one principal, which is its only writer. Two principals
/// sharing a heartbeat cannot both be its only writer, so the writer check refuses that too.
///
/// v2: no separate owner count: it was implied by the writer check.
/// @version 2
/// @req REQ-ING-TIV-011
fn heartbeats(tables: &Tables, errors: &mut Errors) {
    for (p, principal) in tables.principals.iter().enumerate() {
        let Some(k) = principal
            .heartbeat
            .as_deref()
            .and_then(|h| tables.key_index(h))
        else {
            continue;
        };
        let writers: Vec<usize> = (0..tables.principals.len())
            .filter(|&q| tables.writes(q, k))
            .collect();
        if writers != [p] {
            errors.push(format!(
                "{}: heartbeat of {} must be written by it alone and by no other principal",
                tables.keys[k].path, principal.name
            ));
        }
    }
}

/// No principal writes a core-written well-known key.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn core_written(tables: &Tables, errors: &mut Errors) {
    for (k, key) in tables.keys.iter().enumerate() {
        let core = key.wk.is_some_and(|w| WK_RULES[w].core_written);
        if core && (0..tables.principals.len()).any(|p| tables.writes(p, k)) {
            errors.push(format!(
                "{}: is written only by the core; no principal may write it",
                key.path
            ));
        }
    }
}

/// Exactly one principal writes the trace-enable command.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn trace_enable_writers(tables: &Tables, errors: &mut Errors) {
    let Some(k) = tables
        .keys
        .iter()
        .position(|key| key.wk == wk_index("TRACE_ENABLE"))
    else {
        return;
    };
    let writers = (0..tables.principals.len())
        .filter(|&p| tables.writes(p, k))
        .count();
    if writers != 1 {
        errors.push(format!(
            "{}: the trace-enable command needs exactly one writer, has {writers}",
            tables.keys[k].path
        ));
    }
}

/// Commands the core answers itself: owner must be `core`.
const CORE_ANSWERED: [&str; 2] = ["RESET_REQUEST", "TRACE_ENABLE"];

/// Command owners (amended refusal C, 2026-11-03): the core answers RESET_REQUEST and
/// TRACE_ENABLE and nobody writes their results; any other command has a principal owner that
/// reads the command and writes the result; nobody writes any dedupe key (the core keeps them).
///
/// v2: core-answered commands, and dedupe keys kept by the core.
/// @version 2
/// @req REQ-ING-TIV-011
fn command_owners(tables: &Tables, errors: &mut Errors) {
    for command in &tables.commands {
        let index = |id: u32| tables.keys.iter().position(|k| k.id == id);
        let [cmd, result, dedupe] = command.keys.map(|id| index(id).unwrap_or(0));
        let role = tables.keys[cmd].wk.map(|w| WK_RULES[w].name);
        let core_answered = role.is_some_and(|r| CORE_ANSWERED.contains(&r));
        if command.owner == OWNER_CORE {
            core_owner(tables, &command.path, core_answered, result, errors);
        } else {
            principal_owner(
                tables,
                &command.path,
                &command.owner,
                core_answered,
                [cmd, result],
                errors,
            );
        }
        if writers(tables, dedupe) > 0 {
            errors.push(format!(
                "{}: no principal may write a dedupe key; the core keeps it",
                command.path
            ));
        }
    }
}

/// Number of principals with write on key `k`.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn writers(tables: &Tables, k: usize) -> usize {
    (0..tables.principals.len())
        .filter(|&p| tables.writes(p, k))
        .count()
}

/// A `core`-owned command is core-answered, and nobody writes its result.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn core_owner(
    tables: &Tables,
    path: &str,
    core_answered: bool,
    result: usize,
    errors: &mut Errors,
) {
    if !core_answered {
        errors.push(format!(
            "{path}: owner core is only for RESET_REQUEST and TRACE_ENABLE"
        ));
    }
    if writers(tables, result) > 0 {
        errors.push(format!(
            "{path}: the result is answered by the core; no principal may write it"
        ));
    }
}

/// A principal-owned command is not core-answered, and its owner reads the command and writes
/// the result.
///
/// @version 1
/// @req REQ-ING-TIV-011
fn principal_owner(
    tables: &Tables,
    path: &str,
    owner: &str,
    core_answered: bool,
    [cmd, result]: [usize; 2],
    errors: &mut Errors,
) {
    if core_answered {
        errors.push(format!("{path}: RESET_REQUEST and TRACE_ENABLE must be answered by the core (owner = \"core\")"));
    }
    match tables.principal_index(owner) {
        None => errors.push(format!("{path}: owner {owner} is not a principal")),
        Some(p) if tables.rights[p][cmd] & READ == 0 || tables.rights[p][result] & WRITE == 0 => {
            errors.push(format!(
                "{path}: owner {owner} needs read on the command and write on its result"
            ))
        }
        Some(_) => {}
    }
}
