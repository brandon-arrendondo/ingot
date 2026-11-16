//! Resolve declared keys and command types into the rows of `dmt_keys`.

use super::enums::ResolvedEnum;
use super::header::{self, HeaderField};
use super::ranges::{self, Range, Units};
use super::schema::{
    ClassDef, CommandDef, ErrorRef, KeyDef, KeyType, NamespaceFile, Payload, Persist, Role,
};
use super::types::{
    wk_index, Version, CMD_HEADER_BYTES, DEDUPE_ENTRY_BYTES, KF_ALIAS, KF_CMD, KF_CMD_DEDUPE,
    KF_CMD_RESULT, KF_ENUM, KF_MFG, KF_SECRET, KF_SURVIVES_RESET, RESULT_HEADER_BYTES, X_INPROC,
    X_UART, X_UNIX,
};
use super::value::{encode_default, parse_rate};
use super::wire::{self, KeyWire};
use crate::model::key::KeyEncoding;
use std::collections::BTreeMap;

/// Part a key plays in a command type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdPart {
    Command,
    Result,
    Dedupe,
}

/// One resolved key: a row of `dmt_keys` plus what the checks and contract.json need.
#[derive(Debug, Clone)]
pub struct Key {
    pub id: u32,
    /// `namespace.class.key`
    pub path: String,
    pub ty: KeyType,
    pub max_size: u32,
    pub rate_per_day: u32,
    pub rate_burst: u32,
    pub persist: Persist,
    pub deferred_ms: u32,
    pub alias_of: Option<String>,
    pub default: Vec<u8>,
    pub introduced: Version,
    pub role: Role,
    pub secret: bool,
    pub readers: Vec<String>,
    pub survives_reset: bool,
    pub mfg: bool,
    /// The resolved enum of the key's variant, and its name (`namespace::enum`).
    pub enum_values: Option<ResolvedEnum>,
    pub enum_name: Option<String>,
    pub wk: Option<usize>,
    pub command: Option<(String, CmdPart)>,
    pub declared_read_only: bool,
    pub declared_thread_safe: bool,
    /// Reference documentation, carried into contract.json.
    pub doc: Option<String>,
    pub unit: Option<String>,
    /// Range, allowed set, sentinel default and scale; tooling and binder inputs.
    pub range: Option<Range>,
    pub allowed: Option<Vec<i64>>,
    pub unset: Option<i64>,
    pub scale: f64,
    /// The error group and bit of an error key.
    pub error: Option<ErrorRef>,
    /// The shadow wire attributes.
    pub wire: KeyWire,
    /// The fixed header a struct value starts with, when declared.
    pub header: Option<Vec<HeaderField>>,
}

impl Key {
    /// `DMT_KF_*` flags of this key.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn flags(&self) -> u16 {
        let part = match self.command.as_ref().map(|(_, part)| *part) {
            Some(CmdPart::Command) => KF_CMD,
            Some(CmdPart::Result) => KF_CMD_RESULT,
            Some(CmdPart::Dedupe) => KF_CMD_DEDUPE,
            None => 0,
        };
        let bit = |on: bool, flag: u16| if on { flag } else { 0 };
        part | bit(self.secret, KF_SECRET)
            | bit(self.survives_reset, KF_SURVIVES_RESET)
            | bit(self.alias_of.is_some(), KF_ALIAS)
            | bit(self.enum_values.is_some(), KF_ENUM)
            | bit(self.mfg, KF_MFG)
    }

    /// `DMT_X_*` mask: struct keys need the socket's fd passing; secrets never cross UART.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn xport(&self) -> u8 {
        match (self.ty, self.secret) {
            (KeyType::Struct, _) => X_UNIX,
            (_, true) => X_UNIX | X_INPROC,
            _ => X_UNIX | X_INPROC | X_UART,
        }
    }

    /// The key's deferred interval, 0 unless it is a deferred key (a class default interval
    /// does not apply to the class's non-deferred keys).
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn effective_deferred_ms(&self) -> u32 {
        if self.persist == Persist::Deferred {
            self.deferred_ms
        } else {
            0
        }
    }
}

/// Where a key is declared, and the inputs its resolution shares.
pub struct ClassCtx<'a> {
    pub ns: &'a NamespaceFile,
    pub class: &'a ClassDef,
    /// The namespace's enums, resolved for the variant.
    pub enums: &'a BTreeMap<String, ResolvedEnum>,
    /// The unit vocabulary, when the model declares one.
    pub units: Option<&'a Units>,
    pub variant: Option<&'a str>,
    pub default_burst: u32,
}

impl ClassCtx<'_> {
    /// Encoded ID of index `index` and type `ty` in this class, flag bits zero.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    fn id(&self, index: u16, ty: KeyType) -> u32 {
        KeyEncoding {
            namespace: self.ns.meta.namespace_id,
            class: self.class.class_index,
            id: index,
            data_type: ty.code(),
            thread_safe: false,
            derived: false,
            read_only: false,
        }
        .encode()
    }

    /// `namespace.class.name`
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    fn path(&self, name: &str) -> String {
        format!("{}.{}.{name}", self.ns.meta.id, self.class.id)
    }

    /// A key of this class with every field at its class default.
    ///
    /// v2: no range, allowed set or sentinel, scale 1.
    /// v3: no enum name.
    /// v4: no error group.
    /// v5: no shadow wire attributes.
    /// v6: no header.
    /// @version 6
    /// @req REQ-ING-TIV-003
    /// @req REQ-ING-HDR-001
    fn base(&self, name: &str, index: u16, ty: KeyType, introduced: Version) -> Key {
        Key {
            id: self.id(index, ty),
            path: self.path(name),
            ty,
            max_size: ty.scalar_size().unwrap_or(0),
            rate_per_day: 0,
            rate_burst: 0,
            persist: self.class.persistence,
            deferred_ms: self.class.deferred_ms,
            alias_of: None,
            default: Vec::new(),
            introduced,
            role: self.class.role,
            secret: false,
            readers: Vec::new(),
            survives_reset: self.ns.meta.mfg,
            mfg: self.ns.meta.mfg,
            enum_values: None,
            enum_name: None,
            wk: None,
            command: None,
            declared_read_only: false,
            declared_thread_safe: false,
            doc: None,
            unit: None,
            range: None,
            allowed: None,
            unset: None,
            scale: 1.0,
            error: None,
            wire: KeyWire::default(),
            header: None,
        }
    }
}

/// Errors found while resolving, prefixed with where they are.
pub type Errors = Vec<String>;

/// Largest key index the 10-bit field holds.
const MAX_KEY_INDEX: u16 = 1023;

/// Record an error when `index` does not fit the key-index field.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn check_index(ctx: &ClassCtx, name: &str, index: u16, errors: &mut Errors) {
    if index > MAX_KEY_INDEX {
        errors.push(format!(
            "{}: key_index {index} exceeds {MAX_KEY_INDEX}",
            ctx.path(name)
        ));
    }
}

/// Resolve one declared key; problems go to `errors`.
///
/// v2: units, ranges, allowed sets, sentinels and scale.
/// v3: the enum name, for the keyspace.
/// v4: the shadow wire attributes.
/// v5: the struct value header.
/// @version 5
/// @req REQ-ING-TIV-003
/// @req REQ-ING-RNG-001
/// @req REQ-ING-SHD-001
/// @req REQ-ING-HDR-001
pub fn declared_key(ctx: &ClassCtx, def: &KeyDef, errors: &mut Errors) -> Key {
    check_index(ctx, &def.id, def.key_index, errors);
    let mut key = ctx.base(
        &def.id,
        def.key_index,
        def.ty,
        version(ctx, &def.introduced_in, errors),
    );
    copy_attributes(&mut key, def);
    key.wk = def
        .wk
        .as_deref()
        .and_then(|name| role(&key.path, name, errors));
    key.enum_values = def
        .enum_ref
        .as_ref()
        .and_then(|e| enum_of(ctx, &key.path, e, errors));
    key.enum_name = def
        .enum_ref
        .as_ref()
        .map(|e| format!("{}::{e}", ctx.ns.meta.id));
    layout_key(&mut key, def, errors);
    rate_key(
        &mut key,
        def.write_rate.as_deref(),
        def.rate_burst,
        ctx.default_burst,
        errors,
    );
    default_key(&mut key, ctx.variant, def, errors);
    ranges::apply(&mut key, def, ctx.units, errors);
    key.wire = wire::of(ctx, &key.path, def, errors);
    key
}

/// Copy the declared attributes that need no resolution onto `key`.
///
/// v2: the error group and bit.
/// @version 2
/// @req REQ-ING-ERR-001
/// @req REQ-ING-TIV-003
fn copy_attributes(key: &mut Key, def: &KeyDef) {
    key.persist = def.persistence.unwrap_or(key.persist);
    key.deferred_ms = def.deferred_ms.unwrap_or(key.deferred_ms);
    key.secret = def.secret;
    key.readers.clone_from(&def.readers);
    key.survives_reset |= def.survives_reset;
    key.alias_of.clone_from(&def.alias_of);
    key.declared_read_only = def.read_only;
    key.declared_thread_safe = def.thread_safe;
    key.doc.clone_from(&def.doc);
    key.unit.clone_from(&def.unit);
    key.error.clone_from(&def.error);
}

/// One generated key of a command type: name, index, type, part, max_size.
type CommandPart = (String, u16, KeyType, CmdPart, u32);

/// The ids of a command type's command, result and dedupe keys, within its class.
///
/// @version 1
/// @req REQ-ING-TIV-005
pub fn command_key_names(def: &CommandDef) -> [String; 3] {
    [
        def.id.clone(),
        format!("{}_result", def.id),
        format!("{}_dedupe", def.id),
    ]
}

/// The command, result and dedupe keys a command type declares.
///
/// v2: key names from command_key_names.
/// @version 2
/// @req REQ-ING-TIV-005
fn command_parts(def: &CommandDef) -> [CommandPart; 3] {
    let [cmd, result, dedupe] = command_key_names(def);
    let cmd_ty = match def.payload {
        Payload::Binary => KeyType::Binary,
        Payload::Struct => KeyType::Struct,
    };
    [
        (
            cmd,
            def.cmd_index,
            cmd_ty,
            CmdPart::Command,
            CMD_HEADER_BYTES + def.payload_max,
        ),
        (
            result,
            def.result_index,
            KeyType::Binary,
            CmdPart::Result,
            RESULT_HEADER_BYTES + def.result_max,
        ),
        (
            dedupe,
            def.dedupe_index,
            KeyType::Binary,
            CmdPart::Dedupe,
            u32::from(def.dedupe_ring) * DEDUPE_ENTRY_BYTES,
        ),
    ]
}

/// The three keys of one command type.
///
/// v2: the command's write rate and burst, resolved once, on all three keys.
/// @version 2
/// @req REQ-ING-TIV-005
/// @req REQ-ING-TIV-019
pub fn command_keys(ctx: &ClassCtx, def: &CommandDef, errors: &mut Errors) -> [Key; 3] {
    let introduced = version(ctx, &def.introduced_in, errors);
    let rate = rate_of(
        &ctx.path(&def.id),
        def.write_rate.as_deref(),
        def.rate_burst,
        ctx.default_burst,
        errors,
    );
    command_parts(def).map(|(name, index, ty, part, size)| {
        check_index(ctx, &name, index, errors);
        let mut key = ctx.base(&name, index, ty, introduced);
        key.max_size = size;
        key.persist = def.persistence;
        key.command = Some((ctx.path(&def.id), part));
        key.doc.clone_from(&def.doc);
        (key.rate_per_day, key.rate_burst) = rate;
        if part == CmdPart::Command {
            key.wk = def
                .wk
                .as_deref()
                .and_then(|name| role(&key.path, name, errors));
        }
        key
    })
}

/// The namespace enum a command type names as its outcomes, resolved for the variant; an
/// undefined one is recorded against the command's path.
///
/// @version 1
/// @req REQ-ING-TIV-018
pub fn command_outcome(
    ctx: &ClassCtx,
    def: &CommandDef,
    errors: &mut Errors,
) -> Option<ResolvedEnum> {
    let path = ctx.path(&def.id);
    def.outcome
        .as_ref()
        .and_then(|e| enum_of(ctx, &path, e, errors))
}

/// Parse an `introduced_in`, recording an error (and using 0.0) when it is malformed.
///
/// @version 1
/// @req REQ-ING-TIV-002
fn version(ctx: &ClassCtx, text: &str, errors: &mut Errors) -> Version {
    Version::parse(text).unwrap_or_else(|e| {
        errors.push(format!(
            "{}.{}: introduced_in {e}",
            ctx.ns.meta.id, ctx.class.id
        ));
        Version { major: 0, minor: 0 }
    })
}

/// Index of well-known role `name`, recording an error for an unknown one.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn role(path: &str, name: &str, errors: &mut Errors) -> Option<usize> {
    let index = wk_index(name);
    if index.is_none() {
        errors.push(format!("{path}: unknown well-known role '{name}'"));
    }
    index
}

/// The namespace enum `name`, resolved for the variant, recording an error for an undefined one.
///
/// v2: the resolved (variant) enum, with wire strings.
/// @version 2
/// @req REQ-ING-TIV-003
/// @req REQ-ING-TIV-016
fn enum_of(ctx: &ClassCtx, path: &str, name: &str, errors: &mut Errors) -> Option<ResolvedEnum> {
    let values = ctx.enums.get(name).cloned();
    if values.is_none() {
        errors.push(format!("{path}: undefined enum '{name}'"));
    }
    values
}

/// Apply `max_size` to a string, binary or struct key (required there, refused on scalars).
///
/// @version 1
/// @req REQ-ING-TIV-010
fn size_key(key: &mut Key, max_size: Option<u32>, errors: &mut Errors) {
    match (key.ty.scalar_size(), max_size) {
        (None, Some(size)) if size > 0 => key.max_size = size,
        (None, _) => errors.push(format!(
            "{}: {} key needs max_size > 0",
            key.path,
            key.ty.name()
        )),
        (Some(_), Some(_)) => errors.push(format!("{}: max_size on a scalar key", key.path)),
        (Some(_), None) => {}
    }
}

/// Apply a declared key's `max_size`, then the struct value header laid out within it.
///
/// @version 1
/// @req REQ-ING-TIV-010
/// @req REQ-ING-HDR-001
fn layout_key(key: &mut Key, def: &KeyDef, errors: &mut Errors) {
    size_key(key, def.max_size, errors);
    key.header = header::resolve(key, def.header.as_deref(), errors);
}

/// Apply a declared key's write rate and its burst.
///
/// v2: the rate and burst from rate_of (a burst without a rate is refused).
/// @version 2
/// @req REQ-ING-TIV-007
/// @req REQ-ING-TIV-019
fn rate_key(
    key: &mut Key,
    rate: Option<&str>,
    burst: Option<u32>,
    default: u32,
    errors: &mut Errors,
) {
    (key.rate_per_day, key.rate_burst) = rate_of(&key.path, rate, burst, default, errors);
}

/// The writes per day and token-bucket burst of the declaration at `path`: (0, 0) without a
/// write rate, the declared burst or else `default` with one. A burst without a rate, a declared
/// burst of 0 (it would switch the store's rate limit off), and a malformed rate are recorded
/// against `path` and give (0, 0).
///
/// v2: a declared burst of 0 is refused.
/// @version 2
/// @req REQ-ING-TIV-007
/// @req REQ-ING-TIV-019
fn rate_of(
    path: &str,
    rate: Option<&str>,
    burst: Option<u32>,
    default: u32,
    errors: &mut Errors,
) -> (u32, u32) {
    match (rate.map(parse_rate), burst) {
        (Some(Ok(_)), Some(0)) => {
            errors.push(format!(
                "{path}: rate_burst must be at least 1 (0 switches the write-rate limit off; \
                 omit write_rate for no limit)"
            ));
            (0, 0)
        }
        (Some(Ok(per_day)), _) => (per_day, burst.unwrap_or(default)),
        (Some(Err(e)), _) => {
            errors.push(format!("{path}: {e}"));
            (0, 0)
        }
        (None, Some(_)) => {
            errors.push(format!("{path}: rate_burst needs a write_rate"));
            (0, 0)
        }
        (None, None) => (0, 0),
    }
}

/// Encode the key's default for the variant (`defaults.<variant>`, else `default`).
///
/// @version 1
/// @req REQ-ING-TIV-004
fn default_key(key: &mut Key, variant: Option<&str>, def: &KeyDef, errors: &mut Errors) {
    let chosen = variant
        .and_then(|v| def.defaults.get(v))
        .or(def.default.as_ref());
    if let Some(value) = chosen {
        match encode_default(key.ty, value, key.max_size, key.enum_values.as_ref()) {
            Ok(bytes) => key.default = bytes,
            Err(e) => errors.push(format!("{}: {e}", key.path)),
        }
    }
}
