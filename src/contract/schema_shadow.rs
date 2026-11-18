//! Contract-mode input of the shadow wire mapping: the per-key attributes' types, the
//! apply groups, echo-only desired paths and constant reported rows of `contract.toml`. The same
//! types are written into contract.json and the keyspace files.

use serde::{Deserialize, Serialize};

/// How a path carries an enum key's value, or a bool key's on a reported path.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireForm {
    /// The value's wire string (inbound: a wire string or an accept alias).
    Enum,
    /// An enum code as a JSON integer, inbound and outbound; a bool as 0 or 1, reported only.
    Integer,
}

/// The reported documents a path is in.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The full document only.
    Full,
    /// The on-change delta only.
    Delta,
    /// Both.
    Both,
}

/// One shadow path of a key, with its declared wire form and scope (absent: the defaults).
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PathForm {
    pub path: String,
    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub form: Option<WireForm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    /// The variants the path exists on (absent: every variant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<String>>,
}

impl PathForm {
    /// Whether the path carries the code as an integer.
    ///
    /// @version 1
    /// @req REQ-ING-SHD-003
    pub fn integer(&self) -> bool {
        self.form == Some(WireForm::Integer)
    }

    /// Whether the path is in the full document.
    ///
    /// v2: through `in_full`, shared with echo rows.
    /// @version 2
    /// @req REQ-ING-SHD-003
    pub fn in_full(&self) -> bool {
        in_full(self.scope)
    }

    /// Whether the path is in the delta.
    ///
    /// v2: through `in_delta`, shared with echo rows.
    /// @version 2
    /// @req REQ-ING-SHD-003
    pub fn in_delta(&self) -> bool {
        in_delta(self.scope)
    }

    /// Whether the path declares no form, scope or variants.
    ///
    /// v2: variants.
    /// @version 2
    /// @req REQ-ING-SHD-003
    /// @req REQ-ING-SHD-004
    pub fn plain(&self) -> bool {
        self.form.is_none() && self.scope.is_none() && self.variants.is_none()
    }
}

/// One entry of a key's `shadow`: a bare path, or a path with its form and scope.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum PathDecl {
    Bare(String),
    Form(PathForm),
}

impl PathDecl {
    /// The entry as a path with its declared form, scope and variants.
    ///
    /// v2: variants.
    /// @version 2
    /// @req REQ-ING-SHD-004
    /// @req REQ-ING-SHD-003
    fn form(&self) -> PathForm {
        match self {
            PathDecl::Bare(path) => PathForm {
                path: path.clone(),
                form: None,
                scope: None,
                variants: None,
            },
            PathDecl::Form(f) => f.clone(),
        }
    }
}

/// A key's `shadow`: one entry or several.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Paths {
    One(PathDecl),
    Many(Vec<PathDecl>),
}

impl Paths {
    /// The paths with their forms and scopes, in declaration order.
    ///
    /// v2: entries may declare a wire form and a scope.
    /// @version 2
    /// @req REQ-ING-SHD-001
    /// @req REQ-ING-SHD-003
    pub fn list(&self) -> Vec<PathForm> {
        match self {
            Paths::One(p) => vec![p.form()],
            Paths::Many(list) => list.iter().map(PathDecl::form).collect(),
        }
    }
}

/// What the binder reports for a rejected desired value.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnReject {
    /// The default: the value is reported back as received, with the outcome.
    Echo,
    /// `desired.<path> = null` plus the outcome (a wire change).
    ClearDesired,
}

/// `implies = [{ key, value }]`: a coupled write in the same batch.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Implies {
    pub key: String,
    pub value: toml::Value,
}

/// How a reported key's value is rendered.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderKind {
    /// A comma-separated index string reported as the array of the indexed enum's wire strings.
    CsvIndexList,
}

/// `render = { kind, enum, max_items }`.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Render {
    pub kind: RenderKind,
    /// The enum (of the key's namespace) whose wire strings the indices select.
    #[serde(rename = "enum")]
    pub enum_ref: String,
    /// The most items the list holds.
    pub max_items: u16,
}

/// `[[apply_groups]]`: SETTINGS keys one desired update writes as one store batch.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ApplyGroup {
    pub id: String,
    pub doc: Option<String>,
}

/// The JSON type of an echo-only path.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EchoType {
    Bool,
    Int,
    Float,
    String,
}

/// `[[shadow_echo]]`: a desired path reported back as received, with no key.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Echo {
    pub path: String,
    #[serde(rename = "type")]
    pub ty: EchoType,
    /// Longest text of a string path, in bytes.
    pub max_size: Option<u32>,
    /// The SETTINGS key command bodies apply this field to (the delta path only echoes it).
    pub command_key: Option<String>,
    /// The variants the path is echoed on (absent: every variant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<String>>,
    /// The reported documents the echo is in (absent: both).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
}

/// Whether a row of `scope` (absent: both) is in the full document.
///
/// @version 1
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-007
pub fn in_full(scope: Option<Scope>) -> bool {
    scope != Some(Scope::Delta)
}

/// Whether a row of `scope` (absent: both) is in the delta.
///
/// @version 1
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-007
pub fn in_delta(scope: Option<Scope>) -> bool {
    scope != Some(Scope::Full)
}

/// Whether rows of scopes `a` and `b` (absent: both) meet in a document.
///
/// @version 1
/// @req REQ-ING-SHD-010
pub fn scopes_meet(a: Option<Scope>, b: Option<Scope>) -> bool {
    (in_full(a) && in_full(b)) || (in_delta(a) && in_delta(b))
}

/// `[shadow_rejected]`: the reported path where the binder lists the desired members it rejected
/// in the update it answers (in the delta only; the list is transient and has no key).
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rejected {
    pub path: String,
    /// The variants the list is reported on (absent: every variant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<String>>,
}

/// A constant whose value the image supplies.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Image {
    /// `"M.m.p"` of the contract, filled by ingot.
    ContractVersion,
    /// The 64-hex SHA-256 of the resolved keyspace body, filled when the image is built.
    ContractHash,
}

/// `[[shadow_constant]]`: a reported path with a literal or image value and no key.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Constant {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<toml::Value>,
    pub image: Option<Image>,
    /// Re-emitted on every delta.
    #[serde(default)]
    pub forced: bool,
}

/// `[[topic_payload]]`: one member of a pub topic's payload, from a key or a constant.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PayloadRow {
    pub topic: String,
    /// Dotted, each dot one nested object of the payload.
    pub path: String,
    /// The key whose value the member carries (any key with a JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// A literal value instead of a key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<toml::Value>,
    /// An image value instead of a key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,
    /// How an enum key's value is carried (default its wire string).
    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub form: Option<WireForm>,
    /// What an unknown code reports, on an enum key's wire-string member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbound_unknown: Option<String>,
    /// The variants the member exists on (absent: every variant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<String>>,
}

/// Direction of a topic.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Pub,
    Sub,
}

/// The connection a topic uses when the model names none.
///
/// @version 1
/// @req REQ-ING-TOP-001
fn main_connection() -> String {
    "main".to_string()
}

/// `[[topics]]`: one MQTT topic of the device.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    pub id: String,
    /// Over `{stage}`, `{duid}`, `{model}`, `{thing}` and `{shadow_name}`, each repeatable.
    pub template: String,
    pub direction: Direction,
    #[serde(default = "main_connection")]
    pub connection: String,
    pub subscribe_order: Option<u16>,
    /// The key of the engineering `/eng/cmd` wrapper that feeds this topic's handler.
    pub local_wrapper: Option<String>,
    /// The pub topic responses go to.
    pub ack: Option<String>,
    pub envelope: Option<Envelope>,
    /// Every body on this topic also runs the settings parsers on its top-level fields.
    #[serde(default)]
    pub settings_in_every_body: bool,
    /// The shadow name is a literal here although another topic uses `{shadow_name}` (kept).
    #[serde(default)]
    pub literal_shadow_name: bool,
    /// Placeholders of a pub topic that the publisher fills per message, each `{name}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<TopicVar>,
    /// The variants the topic exists on (absent: every variant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<String>>,
}

/// One per-message placeholder of a pub topic: its name, its longest value in bytes and the
/// enum whose code it carries, when it names one.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TopicVar {
    pub name: String,
    pub max_len: u16,
    /// `namespace::enum`: the var carries a code of that enum as decimal text (a topic var only).
    #[serde(rename = "enum", default, skip_serializing_if = "Option::is_none")]
    pub enum_ref: Option<String>,
}

/// A command envelope: the id and the source of a request.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: Option<EnvelopeId>,
    pub source: Option<EnvelopeSource>,
}

/// The envelope id: required, or optional with a declared absent value.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeId {
    pub field: String,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absent: Option<toml::Value>,
}

/// The envelope source: the field it is read from (none: never read) and the origin enum
/// member it takes when absent.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeSource {
    pub field: Option<String>,
    /// `namespace::enum`
    #[serde(rename = "enum")]
    pub enum_ref: String,
    pub absent: String,
}

/// How a topic's requests are processed.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// One request, one command.
    #[default]
    Single,
    /// An array of command names drained one at a time, one response at the end.
    CommandArray,
}

/// A selector's wire value: a string or a bool.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum WireLit {
    Flag(bool),
    Text(String),
}

/// Which requests a row matches: `field` equal to `wire`, holding `element` in its array, or
/// being an object.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub field: String,
    pub wire: Option<WireLit>,
    pub element: Option<String>,
    #[serde(default)]
    pub object: bool,
}

/// The kind of a request body.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    /// The arguments are fields of the request.
    Fields,
    /// The request is a settings body (the desired tree without `settings.`).
    Settings,
    /// A raw binary body: no library build accepts one (refused).
    Binary,
}

/// `body = { object, selector }`: the arguments are fields of one object holding the verb.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectBody {
    pub object: String,
    pub selector: Selector,
}

/// A row's body.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Body {
    Kind(BodyKind),
    Object(ObjectBody),
}

impl Default for Body {
    /// Fields of the request.
    ///
    /// @version 1
    /// @req REQ-ING-CMD-001
    fn default() -> Self {
        Body::Kind(BodyKind::Fields)
    }
}

/// `required = "per_handler"`: not yet read from the handler (refused).
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Unresolved {
    PerHandler,
}

/// Whether an argument must be present.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(untagged)]
pub enum Required {
    Flag(bool),
    Unresolved(Unresolved),
}

impl Default for Required {
    /// Optional.
    ///
    /// @version 1
    /// @req REQ-ING-CMD-001
    fn default() -> Self {
        Required::Flag(false)
    }
}

/// How a text argument is encoded on the wire.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Base64 text: passed through as text into a string key, decoded into a binary key.
    Base64,
    /// A JSON string holding a decimal integer, into an integer key.
    DecimalText,
}

/// One argument of a row: the request field (or a constant value) and the key it is written to.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Arg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The element of an array field the argument takes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<u16>,
    /// A constant written into the key when the row is selected (instead of a field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<toml::Value>,
    pub key: String,
    #[serde(default)]
    pub required: Required,
    /// The result code a missing required argument answers.
    pub ec: Option<u8>,
    /// The value written when the field is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absent: Option<toml::Value>,
    /// Nothing is written when the field is absent.
    #[serde(default)]
    pub omit_when_absent: bool,
    pub encoding: Option<Encoding>,
}

/// `[[shadow_commands]]`: one way a request reaches a command, or a settings body.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShadowCommand {
    pub topic: String,
    /// The variants the row exists on (absent: every one), and the library build it is grounded in.
    pub variants: Option<Vec<String>>,
    pub grounded_in: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    pub selector: Selector,
    #[serde(default)]
    pub body: Body,
    /// The command type's path (`namespace.class.command`); none for a settings body, an
    /// answer, an internal verb or a row that writes STATE keys.
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<Arg>,
    /// The result code the verb is answered with, and nothing else is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<u8>,
    /// Why the library handles the verb itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal: Option<String>,
    /// The model's stable id of an internal row's action, which the library dispatches on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_id: Option<u16>,
    /// Further commands the row submits after `command`, in order, each with its arguments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Step>,
}

/// One further command of a row (`also`), with its arguments.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub command: String,
    #[serde(default)]
    pub args: Vec<Arg>,
}

impl ShadowCommand {
    /// The row's steps in order: its command (none: STATE writes only) with its arguments, then
    /// each `also` command with its own.
    ///
    /// @version 1
    /// @req REQ-ING-CMD-004
    pub fn steps(&self) -> Vec<(Option<&String>, &[Arg])> {
        let first = (self.command.as_ref(), self.args.as_slice());
        let rest = self
            .also
            .iter()
            .map(|s| (Some(&s.command), s.args.as_slice()));
        std::iter::once(first).chain(rest).collect()
    }

    /// Whether any step of the row submits command `path`.
    ///
    /// @version 1
    /// @req REQ-ING-CMD-004
    pub fn submits(&self, path: &str) -> bool {
        self.steps()
            .iter()
            .any(|(c, _)| c.is_some_and(|c| c == path))
    }
}
