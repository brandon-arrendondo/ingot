//! Refusals of the shadow command rows: their requests and arguments, and every command of the
//! variant enumerated on the wire or marked internal with a reason.

use super::keys::{Errors, Key};
use super::model::{variants_overlap, Tables};
use super::schema::{
    Arg, Body, BodyKind, Direction, Encoding, KeyType, Protocol, Required, Role, Selector,
    ShadowCommand, WireLit,
};
use super::shadow_size::valid_path;
use super::value::{encode_default, int_range};

/// Every command-row refusal.
///
/// v2: the arguments of every step, and the further commands.
/// v3: the arguments are checked against their step's command alone.
/// v4: an internal row's id.
/// @version 4
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-004
/// @req REQ-ING-CMD-006
pub fn check(tables: &Tables, errors: &mut Errors) {
    let rows = &tables.shadow.commands;
    for r in rows {
        let mut refuse = |why: String| errors.push(format!("{}: {why}", label(r)));
        topic_rules(tables, r, &mut refuse);
        selector_rules(tables, r, &mut refuse);
        body_rules(tables, r, &mut refuse);
        internal_id_rule(r, &mut refuse);
        also_rules(tables, r, &mut refuse);
        for (command, args) in r.steps() {
            for a in args {
                arg_rules(tables, command, a, &mut refuse);
            }
        }
    }
    duplicates(rows, errors);
    enumerated(tables, errors);
}

/// Further commands follow a row's own command, and each is a command of the variant.
///
/// @version 1
/// @req REQ-ING-CMD-004
fn also_rules(tables: &Tables, r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    if !r.also.is_empty() && r.command.is_none() {
        refuse("also follows the row's own command".into());
    }
    for s in &r.also {
        if !tables.commands.iter().any(|c| c.path == s.command) {
            refuse(format!(
                "also: command {} is not a command of this variant",
                s.command
            ));
        }
    }
}

/// `field=wire`, `field[element]` or `field{}`.
///
/// @version 1
/// @req REQ-ING-CMD-001
pub fn selector_text(s: &Selector) -> String {
    match (&s.wire, &s.element) {
        (Some(WireLit::Text(w)), _) => format!("{}={w}", s.field),
        (Some(WireLit::Flag(b)), _) => format!("{}={b}", s.field),
        (None, Some(e)) => format!("{}[{e}]", s.field),
        (None, None) => format!("{}{{}}", s.field),
    }
}

/// `shadow_commands <topic> <selector>[ <object>.<selector>]`.
///
/// v2: shared with the internal row constants.
/// @version 2
/// @req REQ-ING-CMD-001
/// @req REQ-ING-DSH-008
pub(super) fn label(r: &ShadowCommand) -> String {
    let inner = match &r.body {
        Body::Object(o) => format!(" {}.{}", o.object, selector_text(&o.selector)),
        Body::Kind(_) => String::new(),
    };
    format!(
        "shadow_commands {} {}{inner}",
        r.topic,
        selector_text(&r.selector)
    )
}

/// The row's topic is a sub topic; a per-variant row names the build it is grounded in.
///
/// @version 1
/// @req REQ-ING-CMD-001
fn topic_rules(tables: &Tables, r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    let topic = tables.shadow.topics.iter().find(|t| t.id == r.topic);
    if topic.is_none_or(|t| t.direction != Direction::Sub) {
        refuse(format!("topic {} is not a sub topic", r.topic));
    }
    if r.variants.is_some() != r.grounded_in.is_some() {
        refuse("a per-variant row names the build it is grounded in, and only it".into());
    }
}

/// How many of wire, element and object a selector uses.
///
/// @version 1
/// @req REQ-ING-CMD-001
fn forms(s: &Selector) -> usize {
    usize::from(s.wire.is_some()) + usize::from(s.element.is_some()) + usize::from(s.object)
}

/// One selector form; element selectors are exactly the command_array protocol, whose topic has
/// an envelope id; an object body selects its verb by wire; every request field of the row is
/// dotted identifiers (`field_rules`).
///
/// v2: the request fields.
/// @version 2
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-005
fn selector_rules(tables: &Tables, r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    field_rules(r, refuse);
    if forms(&r.selector) != 1 {
        refuse("the selector has one of wire, element or object".into());
    }
    let array = r.protocol == Protocol::CommandArray;
    if r.selector.element.is_some() != array {
        refuse("an element selector is the command_array protocol, and only it".into());
    }
    let topic = tables.shadow.topics.iter().find(|t| t.id == r.topic);
    let id = topic
        .and_then(|t| t.envelope.as_ref())
        .and_then(|e| e.id.as_ref());
    if array && id.is_none() {
        refuse("a command_array topic needs an envelope id".into());
    }
    object_rule(r, refuse);
}

/// Every request field of a row (selector, body object and its selector, each argument's) is
/// dotted identifiers: each dot is one nested JSON object (`net.reset` is
/// `{"net":{"reset":...}}`; a member literally named "net.reset" never matches).
///
/// @version 1
/// @req REQ-ING-CMD-005
fn field_rules(r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    let mut fields = vec![r.selector.field.as_str()];
    if let Body::Object(o) = &r.body {
        fields.extend([o.object.as_str(), o.selector.field.as_str()]);
    }
    for (_, args) in r.steps() {
        fields.extend(args.iter().filter_map(|a| a.field.as_deref()));
    }
    for f in fields.into_iter().filter(|f| !valid_path(f)) {
        refuse(format!(
            "field '{f}' is not dotted identifiers (each dot is one nested object)"
        ));
    }
}

/// An object body selects its verb by wire, and by nothing else.
///
/// @version 1
/// @req REQ-ING-CMD-001
fn object_rule(r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    if let Body::Object(o) = &r.body {
        if forms(&o.selector) != 1 || o.selector.wire.is_none() {
            refuse("an object body selects its verb by wire".into());
        }
    }
}

/// A settings body has no command and no args; a row with a command names a command of the
/// variant and declares neither answer nor internal; a row without one follows
/// `commandless_rules`; a binary body is refused.
///
/// v2: rows without a command.
/// @version 2
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-003
fn body_rules(tables: &Tables, r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    let known = |c: &String| tables.commands.iter().any(|x| &x.path == c);
    let outcome = r.answer.is_some() || r.internal.is_some();
    match (&r.body, &r.command) {
        (Body::Kind(BodyKind::Binary), _) => {
            refuse("no library build accepts a binary body; send base64 text in a field".into())
        }
        (Body::Kind(BodyKind::Settings), c) if c.is_some() || !r.args.is_empty() || outcome => {
            refuse("a settings body has no command and no args".into())
        }
        (Body::Kind(BodyKind::Settings), _) => {}
        (_, Some(_)) if outcome => {
            refuse("a row with a command declares neither answer nor internal".into())
        }
        (_, Some(c)) if known(c) => {}
        (_, Some(c)) => refuse(format!("command {c} is not a command of this variant")),
        (_, None) => commandless_rules(r, refuse),
    }
}

/// An internal row's id is 1..=65535 and only an internal row has one.
///
/// @version 1
/// @req REQ-ING-CMD-006
fn internal_id_rule(r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    match (r.internal_id, &r.internal) {
        (Some(_), None) => refuse("internal_id is for internal rows".into()),
        (Some(0), Some(_)) => refuse("internal_id is 1..=65535".into()),
        _ => {}
    }
}

/// A row without a command answers (`answer`), is handled by the library (`internal`, with a
/// reason), or writes STATE keys through its args (checked by `arg_key`); an answer or internal
/// row has no args, and declares one of the two.
///
/// @version 1
/// @req REQ-ING-CMD-003
fn commandless_rules(r: &ShadowCommand, refuse: &mut impl FnMut(String)) {
    let outcome = r.answer.is_some() || r.internal.is_some();
    let why = if r.answer.is_some() && r.internal.is_some() {
        Some("a row declares answer or internal, not both")
    } else if outcome && !r.args.is_empty() {
        Some("an answer or internal row has no args")
    } else if r.internal.as_ref().is_some_and(|w| w.trim().is_empty()) {
        Some("internal needs a reason")
    } else if !outcome && r.args.is_empty() {
        Some("command (none) is not a command of this variant")
    } else {
        None
    };
    if let Some(why) = why {
        refuse(why.into());
    }
}

/// The key `path` names when an argument of a step submitting `command` may write it: a plain
/// key of that command's class, or a STATE key.
///
/// v2: a STATE key.
/// v3: the step's command, not the row's.
/// @version 3
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-002
/// @req REQ-ING-CMD-004
fn arg_key<'a>(tables: &'a Tables, command: Option<&String>, path: &str) -> Option<&'a Key> {
    let key = &tables.keys[tables.key_index(path)?];
    let class = command.and_then(|c| c.rsplit_once('.'));
    let in_class =
        class.is_some_and(|(cls, _)| path.rsplit_once('.').is_some_and(|(c, _)| c == cls));
    let plain = key.command.is_none() && (in_class || key.role == Role::State);
    plain.then_some(key)
}

/// The name an argument is reported under: its field, or its key for a constant.
///
/// @version 1
/// @req REQ-ING-CMD-002
fn arg_name(a: &Arg) -> &str {
    a.field.as_deref().unwrap_or(&a.key)
}

/// An argument takes a field or a constant; a constant has no required rule, ec, absent rule,
/// encoding or element; a field argument follows `field_rule`.
///
/// @version 1
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-002
fn presence_rule(a: &Arg) -> Option<&'static str> {
    let constant = a.value.is_some();
    let declared = a.required != Required::Flag(false) || a.ec.is_some() || a.absent.is_some();
    let extra = a.omit_when_absent || a.encoding.is_some() || a.element.is_some();
    if a.field.is_some() == constant {
        Some("an argument takes a field or a constant value, not both and not neither")
    } else if constant && (declared || extra) {
        Some("a constant has no required rule, ec, absent rule, encoding or element")
    } else if constant {
        None
    } else {
        field_rule(a)
    }
}

/// The required and absent rules of a field argument: a required one has its result code and
/// no absent rule; an optional one has exactly one absent rule and no code.
///
/// @version 1
/// @req REQ-ING-CMD-001
fn field_rule(a: &Arg) -> Option<&'static str> {
    match a.required {
        Required::Unresolved(_) => Some("required = per_handler must be resolved from the handler"),
        Required::Flag(true) if a.ec.is_none() || a.absent.is_some() || a.omit_when_absent => {
            Some("a required argument has its ec and no absent rule")
        }
        Required::Flag(false) if a.ec.is_some() || a.absent.is_some() == a.omit_when_absent => {
            Some("an optional argument has an absent value or omit_when_absent, and no ec")
        }
        _ => None,
    }
}

/// `v` for key `key`: a value name of the key's enum becomes its number.
///
/// v2: shared with the shadow tables, which carry the number.
/// @version 2
/// @req REQ-ING-CMD-002
/// @req REQ-ING-DSH-003
pub(super) fn by_name(key: &Key, v: &toml::Value) -> Result<toml::Value, String> {
    match (v.as_str(), &key.enum_values) {
        (Some(name), Some(e)) => e
            .get(name)
            .map(|m| toml::Value::Integer(m.value))
            .ok_or_else(|| format!("{name} is not a value name of the key's enum")),
        _ => Ok(v.clone()),
    }
}

/// Whether encoding `enc` suits a key of type `ty`: base64 into a string (text), binary or struct
/// (decoded) key, decimal text into an integer key.
///
/// v2: a struct key (the decoded bytes, published as a buffer; a large value is a struct key).
/// @version 2
/// @req REQ-ING-CMD-002
fn encoding_rule(enc: Encoding, ty: KeyType) -> Option<&'static str> {
    match enc {
        Encoding::Base64 if !matches!(ty, KeyType::String | KeyType::Binary | KeyType::Struct) => {
            Some("base64 text goes into a string, binary or struct key")
        }
        Encoding::DecimalText if int_range(ty).is_none() => {
            Some("decimal_text goes into an integer key")
        }
        _ => None,
    }
}

/// An absent or constant value of an argument (an enum key's by value name too) suits the key.
/// The all-variants view checks none: every row is checked in each of its variants' passes,
/// against that variant's enums (a variant may name values the base does not have).
///
/// @version 1
/// @req REQ-ING-CMD-002
/// @req REQ-ING-SHD-004
fn value_rule(tables: &Tables, key: &Key, a: &Arg) -> Option<String> {
    a.absent
        .iter()
        .chain(&a.value)
        .filter(|_| !tables.all_variants_view())
        .find_map(|v| {
            by_name(key, v)
                .and_then(|v| encode_default(key.ty, &v, key.max_size, key.enum_values.as_ref()))
                .err()
        })
}

/// An argument writes a key `arg_key` allows; its presence rules hold; its values suit the key
/// (`value_rule`); its encoding suits the key's type.
///
/// v2: constants, value names, STATE keys, decimal text and base64 into a binary key.
/// v3: the value check moved to `value_rule`; the key is checked against the step's command.
/// @version 3
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-002
/// @req REQ-ING-CMD-004
fn arg_rules(tables: &Tables, command: Option<&String>, a: &Arg, refuse: &mut impl FnMut(String)) {
    let name = arg_name(a);
    let Some(key) = arg_key(tables, command, &a.key) else {
        return refuse(format!(
            "arg {name}: {} is neither a plain key of the command's class nor a STATE key",
            a.key
        ));
    };
    if let Some(why) = presence_rule(a) {
        refuse(format!("arg {name}: {why}"));
    }
    if let Some(e) = value_rule(tables, key, a) {
        refuse(format!("arg {name}: {e}"));
    }
    if let Some(why) = a.encoding.and_then(|enc| encoding_rule(enc, key.ty)) {
        refuse(format!("arg {name}: {why}"));
    }
}

/// No two rows select the same request on one variant.
///
/// v2: the variant overlap is the shared `variants_overlap`.
/// @version 2
/// @req REQ-ING-CMD-001
fn duplicates(rows: &[ShadowCommand], errors: &mut Errors) {
    for (i, a) in rows.iter().enumerate() {
        let twice = rows[i + 1..].iter().any(|b| {
            label(a) == label(b) && variants_overlap(a.variants.as_deref(), b.variants.as_deref())
        });
        if twice {
            errors.push(format!(
                "{}: the request is selected twice on one variant",
                label(a)
            ));
        }
    }
}

/// When any row exists, every command of the variant has a row or is internal with a
/// reason, never both (a further command of a row is on the wire too).
///
/// v2: further commands.
/// @version 2
/// @req REQ-ING-CMD-001
/// @req REQ-ING-CMD-004
fn enumerated(tables: &Tables, errors: &mut Errors) {
    let rows = &tables.shadow.commands;
    if rows.is_empty() {
        return;
    }
    for c in &tables.commands {
        let on_wire = rows.iter().any(|r| r.submits(&c.path));
        let why = match (on_wire, c.internal.as_deref()) {
            (false, None) => Some(
                "no shadow_commands row enumerates it (declare one, or internal = \"<reason>\")",
            ),
            (true, Some(_)) => Some("an internal command has a shadow_commands row"),
            (_, Some(reason)) if reason.trim().is_empty() => Some("internal needs a reason"),
            _ => None,
        };
        if let Some(why) = why {
            errors.push(format!("{}: {why}", c.path));
        }
    }
}
