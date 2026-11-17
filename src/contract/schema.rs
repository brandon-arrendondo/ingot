//! Contract-mode input: `contract.toml` and the namespace files it names (see
//! docs/contract-mode.rst). Unknown fields are errors; every index is explicit.

pub use super::schema_log::*;
pub use super::schema_shadow::*;
use serde::Deserialize;
use std::collections::BTreeMap;

/// `contract.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractFile {
    pub contract: ContractMeta,
    pub bounds: Bounds,
    #[serde(default)]
    pub reserved: Vec<Reserved>,
    #[serde(default)]
    pub principals: Vec<PrincipalDef>,
    /// The store core's own log stream, when it has one.
    pub core_log: Option<LogDef>,
    #[serde(default)]
    pub uid_caps: Vec<UidCapsDef>,
    #[serde(default)]
    pub error_groups: Vec<ErrorGroupDef>,
    #[serde(default)]
    pub apply_groups: Vec<ApplyGroup>,
    #[serde(default)]
    pub shadow_echo: Vec<Echo>,
    #[serde(default)]
    pub shadow_constant: Vec<Constant>,
    pub shadow_rejected: Option<Rejected>,
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub topic_payload: Vec<PayloadRow>,
    /// Named values a topic template may use, which the consumer is configured with.
    #[serde(default)]
    pub placeholders: Vec<TopicVar>,
}

/// `[contract]`: name, version and where the namespace files are.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractMeta {
    pub name: String,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub namespace_dir: String,
    /// The product variants (SKUs); when declared, --variant must name one.
    #[serde(default)]
    pub variants: Vec<String>,
    /// The unit vocabulary file, relative to contract.toml; when given, every unit is a token of it.
    pub units: Option<String>,
}

/// The unit vocabulary file: token to its quantity and meaning.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnitsFile {
    pub units: BTreeMap<String, UnitDef>,
}

/// One unit token.
#[derive(Debug, Deserialize, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UnitDef {
    pub quantity: String,
    pub meaning: String,
}

/// `[bounds]`: model-level inputs to the queue bounds and budgets.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub headroom_pct: u32,
    pub min_queue: u32,
    pub default_rate_burst: u32,
    pub staging_total_bytes: u32,
    #[serde(default)]
    pub inproc_queue_bytes: u32,
    #[serde(default)]
    pub inproc_snapshot_bytes: u32,
    /// Upper bound of the log RAM with every stream enabled; required with any log stream.
    pub log_ram_budget_bytes: Option<u32>,
    /// Upper bound of the log RAM of the streams enabled at boot.
    pub log_ram_default_budget_bytes: Option<u32>,
    /// Upper bound of the flash segments of the streams that may write flash.
    pub log_flash_bytes: Option<u32>,
    /// Upper bound of their flash byte rates per day.
    pub log_flash_bytes_per_day: Option<u32>,
}

/// One `[[reserved]]` key ID.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reserved {
    pub id: u32,
}

/// Principal kind.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Unix,
    Inproc,
    Uart,
}

/// Principal flag.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalFlag {
    WatchdogRequired,
    TraceReader,
    TokenIssuer,
}

/// Access a grant confers.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    None,
    R,
    W,
    Rw,
}

/// One grant: a whole namespace, one (namespace, class), or one key (`ns.class.key`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub namespace: Option<String>,
    pub class: Option<String>,
    pub key: Option<String>,
    pub access: Access,
}

/// Per-(uid, token) caps.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Caps {
    #[serde(default)]
    pub inflight_fd: u32,
    #[serde(default)]
    pub inflight_bytes: u32,
    #[serde(default)]
    pub open_fds: u32,
    #[serde(default)]
    pub trace_share_pct: u16,
}

/// One `[[principals]]` entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalDef {
    pub name: String,
    pub uid: u32,
    #[serde(default)]
    pub token: u16,
    pub kind: Kind,
    #[serde(default)]
    pub flags: Vec<PrincipalFlag>,
    pub heartbeat: Option<String>,
    #[serde(default)]
    pub heartbeat_window_ms: u32,
    pub latency_budget_ms: u32,
    #[serde(default)]
    pub caps: Caps,
    pub resync_k: u16,
    pub resync_t_ms: u32,
    pub staging_bound_bytes: u32,
    pub introduced_in: String,
    #[serde(default)]
    pub grants: Vec<Grant>,
    /// The principal's log stream, when it has one.
    pub log: Option<LogDef>,
}

/// One `[[uid_caps]]` entry: totals for a UNIX uid.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UidCapsDef {
    pub uid: u32,
    pub inflight_fd: u32,
    pub inflight_bytes: u32,
    pub open_fds: u32,
}

/// A namespace file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceFile {
    pub meta: NamespaceMeta,
    #[serde(default)]
    pub enums: BTreeMap<String, EnumDef>,
    #[serde(default)]
    pub classes: Vec<ClassDef>,
    /// How requests reach commands.
    #[serde(default)]
    pub shadow_commands: Vec<ShadowCommand>,
}

/// `[meta]` of a namespace file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceMeta {
    pub id: String,
    pub namespace_id: u16,
    #[serde(default)]
    pub mfg: bool,
    /// Documentation only; accepted, not generated.
    #[serde(rename = "doc")]
    pub _doc: Option<String>,
}

/// A named enum: member name to value (and wire string), and its per-variant changes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumDef {
    /// Documentation only; accepted, not generated.
    #[serde(rename = "doc")]
    pub _doc: Option<String>,
    pub values: BTreeMap<String, EnumValueDef>,
    #[serde(default)]
    pub variants: BTreeMap<String, EnumVariantDef>,
}

/// One enum value: a bare number (its wire string is its name) or a number with a wire string.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum EnumValueDef {
    Number(i64),
    Wire(WireValue),
}

/// `{ value = n, wire = "...", accept = [...] }`: the outbound wire string and the other inbound
/// strings that also mean this value; or `report_as` (another member, by name or wire string)
/// instead of a wire string: the code reports that member's wire string.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct WireValue {
    pub value: i64,
    pub wire: Option<String>,
    pub report_as: Option<String>,
    #[serde(default)]
    pub accept: Vec<String>,
}

/// A variant's changes to an enum: values merged over the base by number, base names removed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumVariantDef {
    #[serde(default)]
    pub values: BTreeMap<String, EnumValueDef>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Class role.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Settings,
    State,
    Command,
    Config,
    Diag,
}

/// Persistence class.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Persist {
    #[default]
    None,
    Deferred,
    Durable,
}

/// Value type of a key (and of a struct value header's field, in contract.json).
#[derive(Debug, Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    Bool,
    Uint8,
    Int8,
    Uint16,
    Int16,
    Uint32,
    Int32,
    Float32,
    String,
    Binary,
    Struct,
}

/// A class of a namespace.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassDef {
    pub id: String,
    pub class_index: u8,
    pub role: Role,
    #[serde(default)]
    pub persistence: Persist,
    #[serde(default)]
    pub deferred_ms: u32,
    /// Documentation only; accepted, not generated.
    #[serde(rename = "doc")]
    pub _doc: Option<String>,
    #[serde(default)]
    pub keys: Vec<KeyDef>,
    #[serde(default)]
    pub commands: Vec<CommandDef>,
}

/// A key of a class.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyDef {
    pub id: String,
    pub key_index: u16,
    #[serde(rename = "type")]
    pub ty: KeyType,
    pub introduced_in: String,
    pub write_rate: Option<String>,
    pub rate_burst: Option<u32>,
    pub persistence: Option<Persist>,
    pub deferred_ms: Option<u32>,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub readers: Vec<String>,
    #[serde(default)]
    pub survives_reset: bool,
    pub alias_of: Option<String>,
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub defaults: BTreeMap<String, toml::Value>,
    #[serde(rename = "enum")]
    pub enum_ref: Option<String>,
    pub max_size: Option<u32>,
    pub wk: Option<String>,
    pub doc: Option<String>,
    /// The displayed unit (display = wire x scale), a token of the vocabulary.
    pub unit: Option<String>,
    /// Inclusive range and step, in wire units (integer keys).
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub step: Option<i64>,
    /// A discrete set of accepted values, instead of min/max/step.
    pub allowed: Option<Vec<i64>>,
    /// A sentinel default meaning "never set", outside the range.
    pub unset: Option<i64>,
    /// Wire to display factor.
    pub scale: Option<f64>,
    /// The variants the key exists on; absent: every variant.
    pub variants: Option<Vec<String>>,
    /// The error group and bit of a one-bool-per-error STATE key.
    pub error: Option<ErrorRef>,
    /// The shadow path or paths (desired for SETTINGS, reported for STATE).
    pub shadow: Option<Paths>,
    /// The SETTINGS key this STATE key reports as applied.
    pub applies: Option<String>,
    pub apply_group: Option<String>,
    #[serde(default)]
    pub implies: Vec<Implies>,
    pub on_reject: Option<OnReject>,
    /// The code an unmatched inbound string becomes (an inbound string), or `reject`.
    pub inbound_unknown: Option<String>,
    /// The string an unknown code reports (a wire string), or `raw`.
    pub outbound_unknown: Option<String>,
    /// Re-emitted on every delta.
    #[serde(default)]
    pub forced: bool,
    pub render: Option<Render>,
    /// The telemetry name.
    pub telem: Option<String>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub thread_safe: bool,
    /// The fixed header a struct value starts with, field by field in wire order.
    pub header: Option<Vec<HeaderFieldDef>>,
}

/// `{ name, type }`: one field of a struct key's header.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderFieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: KeyType,
}

/// Payload kind of a command key.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Payload {
    #[default]
    Binary,
    Struct,
}

/// A command type: expands to its command, result and dedupe keys.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDef {
    pub id: String,
    pub cmd_index: u16,
    pub result_index: u16,
    pub dedupe_index: u16,
    pub introduced_in: String,
    pub persistence: Persist,
    pub owner: String,
    #[serde(default)]
    pub payload: Payload,
    #[serde(default)]
    pub payload_max: u32,
    #[serde(default)]
    pub result_max: u32,
    #[serde(default)]
    pub max_edits: u16,
    pub dedupe_ring: u16,
    pub write_rate: Option<String>,
    /// The token-bucket burst of all three keys; needs `write_rate`; absent: the default burst.
    pub rate_burst: Option<u32>,
    pub wk: Option<String>,
    pub doc: Option<String>,
    /// Not on the wire, with the reason (a model mapping commands enumerates every other one).
    pub internal: Option<String>,
    /// The variants the command exists on; absent: every variant.
    pub variants: Option<Vec<String>>,
    /// The namespace enum naming the codes of the result's outcome field.
    pub outcome: Option<String>,
}

/// `error = { group, bit }` on an error key.
#[derive(Debug, Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ErrorRef {
    pub group: String,
    pub bit: u8,
}

/// `[[error_groups]]`: one composed error word.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ErrorGroupDef {
    pub id: String,
    /// Bits in the composed word: 8, 16, 32 or 64.
    pub width: u8,
    /// Wire id of bit 0 ("e<wire_base + bit>"); `wire_bases` overrides it per variant.
    pub wire_base: u32,
    #[serde(default)]
    pub wire_bases: BTreeMap<String, u32>,
    /// The only principal that may write the group's keys.
    pub writer: String,
    /// Positions never used or retired; never reused.
    #[serde(default)]
    pub reserved_bits: Vec<u8>,
    /// The highest bit the full table publishes (default: width - 1).
    pub publish_max_bit: Option<u8>,
    /// The reported field the reporter composes.
    pub shadow: Option<String>,
}
