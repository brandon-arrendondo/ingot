//! The command router's tables: `dsh_commands` (the variant's rows in declaration order) and
//! their `dsh_args`; value names of an enum key are resolved to numbers here.

use super::super::keys::Key;
use super::super::model::Tables;
use super::super::refuse_commands::by_name;
use super::super::schema::{
    Arg, Body, BodyKind, Encoding, Protocol, Required, Selector, ShadowCommand, WireLit,
};
use super::super::value::encode_default;
use super::{id, index16, text, ty, Pools};

/// A `dsh_selector_t` initialiser.
///
/// @version 1
/// @req REQ-ING-DSH-003
fn selector(s: &Selector) -> String {
    let (kind, wire) = match (&s.wire, &s.element) {
        (Some(WireLit::Text(w)), _) => ("DSH_SEL_TEXT", Some(w.as_str())),
        (Some(WireLit::Flag(true)), _) => ("DSH_SEL_TRUE", None),
        (Some(WireLit::Flag(false)), _) => ("DSH_SEL_FALSE", None),
        (None, Some(e)) => ("DSH_SEL_ELEMENT", Some(e.as_str())),
        (None, None) => ("DSH_SEL_OBJECT", None),
    };
    format!(
        "{{ .field = {}, .wire = {}, .kind = {kind} }}",
        text(Some(&s.field)),
        text(wire)
    )
}

/// `.body_selector`, `.body_object` and `.body` of a row.
///
/// @version 1
/// @req REQ-ING-DSH-003
fn body(r: &ShadowCommand) -> String {
    let none = "{ .field = NULL, .wire = NULL, .kind = DSH_SEL_TEXT }";
    let (sel, object, kind) = match &r.body {
        Body::Object(o) => (
            selector(&o.selector),
            Some(o.object.as_str()),
            "DSH_BODY_OBJECT",
        ),
        Body::Kind(BodyKind::Settings) => (none.to_string(), None, "DSH_BODY_SETTINGS"),
        Body::Kind(_) => (none.to_string(), None, "DSH_BODY_FIELDS"),
    };
    format!(
        ".body_selector = {sel}, .body_object = {}, .body = {kind}",
        text(object)
    )
}

/// `.action` and `.answer` of a row: a settings body, an answer, an internal verb, or steps.
///
/// @version 1
/// @req REQ-ING-DSH-003
fn action(r: &ShadowCommand) -> String {
    let settings = r.body == Body::Kind(BodyKind::Settings);
    let action = match (r.answer, &r.internal) {
        _ if settings => "DSH_DO_SETTINGS",
        (Some(_), _) => "DSH_DO_ANSWER",
        (None, Some(_)) => "DSH_DO_INTERNAL",
        (None, None) => "DSH_DO_STEPS",
    };
    format!(".action = {action}, .answer = {}u", r.answer.unwrap_or(0))
}

/// `.flags` and `.value` of an argument: a constant, a required field, an absent value or
/// nothing written when absent.
///
/// @version 1
/// @req REQ-ING-DSH-003
fn presence(key: &Key, a: &Arg, pools: &mut Pools) -> (String, &'static str) {
    let flag = match (&a.value, a.required, &a.absent) {
        (Some(_), _, _) => "DSH_AF_CONSTANT",
        (None, Required::Flag(true), _) => "DSH_AF_REQUIRED",
        (None, _, Some(_)) => "DSH_AF_ABSENT",
        (None, _, None) => "DSH_AF_OMIT",
    };
    let bytes = a.value.as_ref().or(a.absent.as_ref()).map(|v| {
        by_name(key, v)
            .and_then(|v| encode_default(key.ty, &v, key.max_size, key.enum_values.as_ref()))
            .unwrap_or_default()
    });
    let value = bytes.map_or("{ 0u, 0u }".to_string(), |b| pools.value(&b));
    (value, flag)
}

/// One `dsh_args` row.
///
/// @version 1
/// @req REQ-ING-DSH-003
fn arg(tables: &Tables, a: &Arg, pools: &mut Pools) -> String {
    let key = &tables.keys[tables.key_index(&a.key).unwrap_or_default()];
    let enum_index = match (&key.enum_name, &key.enum_values) {
        (Some(n), Some(e)) => Some(pools.enums.index(n, e)),
        _ => None,
    };
    let (value, flag) = presence(key, a, pools);
    let encoding = match a.encoding {
        None => "DSH_ENC_NONE",
        Some(Encoding::Base64) => "DSH_ENC_BASE64",
        Some(Encoding::DecimalText) => "DSH_ENC_DECIMAL_TEXT",
    };
    format!(
        "{{ .field = {}, .value = {value}, .key = {}, .max_size = {}u, .element = {}, \
         .enum_index = {}, .type = {}, .flags = {flag}, .ec = {}u, .encoding = {encoding} }}",
        text(a.field.as_deref()),
        id(key.id),
        key.max_size,
        index16(a.element),
        index16(enum_index),
        ty(key.ty),
        a.ec.unwrap_or(0),
    )
}

/// One `dsh_steps` row: `command` (none: STATE writes only) with `args`, appended to the pools.
/// The step names its command key's type, so a struct command goes through a buffer.
///
/// v2: the command key's type.
/// @version 2
/// @req REQ-ING-DSH-003
fn step(tables: &Tables, command: Option<&String>, args: &[Arg], pools: &mut Pools) {
    let key = command
        .and_then(|c| tables.commands.iter().find(|x| &x.path == c))
        .and_then(|c| tables.keys.iter().find(|k| k.id == c.keys[0]));
    let first = pools.args.len();
    for a in args {
        let row = arg(tables, a, pools);
        pools.args.push(row);
    }
    pools.steps.push(format!(
        "{{ .command = {}, .args_first = {first}u, .args_count = {}u, .command_type = {} }}",
        id(key.map_or(0, |k| k.id)),
        args.len(),
        key.map_or("0u".to_string(), |k| ty(k.ty))
    ));
}

/// The steps of a row that runs steps: its command (or STATE writes) with its arguments, then
/// each further command with its own.
///
/// v2: further commands.
/// @version 2
/// @req REQ-ING-DSH-003
/// @req REQ-ING-CMD-004
fn steps(tables: &Tables, r: &ShadowCommand, pools: &mut Pools) -> String {
    let first = pools.steps.len();
    let settings = r.body == Body::Kind(BodyKind::Settings);
    if !settings && r.answer.is_none() && r.internal.is_none() {
        for (command, args) in r.steps() {
            step(tables, command, args, pools);
        }
    }
    let count = pools.steps.len() - first;
    format!(".steps_first = {first}u, .steps_count = {count}u")
}

/// One `dsh_commands` row; its steps and arguments are appended to the pools.
///
/// v2: an internal row's id (0: none).
/// @version 2
/// @req REQ-ING-DSH-003
/// @req REQ-ING-CMD-006
fn row(tables: &Tables, r: &ShadowCommand, pools: &mut Pools) -> String {
    let topic = tables.shadow.topics.iter().position(|t| t.id == r.topic);
    let protocol = match r.protocol {
        Protocol::Single => "DSH_PROTO_SINGLE",
        Protocol::CommandArray => "DSH_PROTO_COMMAND_ARRAY",
    };
    format!(
        "{{ .selector = {}, {}, .internal = {}, {}, .topic = {}u, .protocol = {protocol}, {}, \
         .internal_id = {}u }}",
        selector(&r.selector),
        body(r),
        text(r.internal.as_deref()),
        steps(tables, r, pools),
        topic.unwrap_or_default(),
        action(r),
        r.internal_id.unwrap_or(0),
    )
}

/// The variant's command rows, in declaration order.
///
/// @version 1
/// @req REQ-ING-DSH-003
pub fn rows(tables: &Tables, pools: &mut Pools) -> Vec<String> {
    tables
        .shadow
        .commands
        .iter()
        .map(|r| row(tables, r, pools))
        .collect()
}
