//! `contract.json`: the machine-readable contract of one generated table set. It is read back
//! as `--previous-contract` and carries the CI inputs (rates, persistence, sizes, bounds).

use super::bounds::queue_bound;
use super::digest::digests;
use super::enums::ResolvedEnum;
use super::error_groups::ErrorGroup;
use super::header::HeaderJson;
use super::model::hex;
use super::model::Tables;
use super::ranges::Range;
use super::schema::{
    Constant, Echo, ErrorRef, Kind, PayloadRow, Rejected, ShadowCommand, Topic, TopicVar, UnitDef,
};
use super::types::WK_RULES;
use super::wire::KeyWire;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The whole contract of one table set.
#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tiv: String,
    pub contract: ContractJson,
    pub variant: String,
    /// The declared variants; absent in snapshots from before they existed.
    #[serde(default)]
    pub variants: Vec<String>,
    /// The unit vocabulary; absent in snapshots from before it existed.
    #[serde(default)]
    pub units: BTreeMap<String, UnitDef>,
    /// The error groups as this variant resolves them; absent in older snapshots.
    #[serde(default)]
    pub error_groups: Vec<ErrorGroup>,
    pub model_hash: String,
    pub ingot: IngotJson,
    pub keys: Vec<KeyJson>,
    pub reserved: Vec<u32>,
    pub principals: Vec<PrincipalJson>,
    /// principal -> key path -> "r" | "w" | "rw" (absent: none)
    pub rights: BTreeMap<String, BTreeMap<String, String>>,
    pub commands: Vec<CommandJson>,
    /// Name to code of each enum a topic var names, by `namespace::enum`; absent when none does.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub var_enums: BTreeMap<String, BTreeMap<String, i64>>,
    /// The shadow wire mapping's rows and sizes; absent when the model maps nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<serde_json::Value>,
}

/// The contract-level rows of contract.json's `shadow` section, read back typed for the
/// previous-contract checks (the sizes are not frozen and are not read).
#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ShadowRows {
    pub echo: Vec<Echo>,
    pub constants: Vec<Constant>,
    pub topics: Vec<Topic>,
    pub commands: Vec<ShadowCommand>,
    pub payload: Vec<PayloadRow>,
    /// The rejected-settings list; absent in snapshots that declare none.
    pub rejected: Option<Rejected>,
    /// The declared topic placeholders; absent in snapshots that declare none.
    pub placeholders: Vec<TopicVar>,
}

impl Snapshot {
    /// The typed contract-level shadow rows (none when the snapshot maps nothing); a section
    /// that does not parse as the rows is an error.
    ///
    /// @version 1
    /// @req REQ-ING-SHD-006
    pub fn shadow_rows(&self) -> Result<ShadowRows, String> {
        match &self.shadow {
            None => Ok(ShadowRows::default()),
            Some(v) => {
                serde_json::from_value(v.clone()).map_err(|e| format!("contract.json shadow: {e}"))
            }
        }
    }
}

/// Contract name and version.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContractJson {
    pub name: String,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

/// The ingot build that generated the set.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngotJson {
    pub version: String,
    pub commit: String,
}

/// One key.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct KeyJson {
    pub path: String,
    pub id: u32,
    #[serde(rename = "type")]
    pub ty: String,
    pub introduced_in: String,
    pub persist: String,
    pub deferred_ms: u32,
    pub rate_per_day: u32,
    pub rate_burst: u32,
    pub max_size: u32,
    pub secret: bool,
    pub survives_reset: bool,
    pub mfg: bool,
    pub alias_of: Option<String>,
    #[serde(rename = "enum")]
    pub enum_values: Option<BTreeMap<String, i64>>,
    /// Number to wire string of the resolved enum; absent in snapshots from before wire strings.
    #[serde(default)]
    pub enum_wire: Option<BTreeMap<i64, String>>,
    pub command: Option<String>,
    pub wk: Option<String>,
    pub doc: Option<String>,
    pub unit: Option<String>,
    /// The struct value header's fields and byte count; absent without a header.
    #[serde(flatten)]
    pub header: HeaderJson,
    /// Range, allowed set, sentinel default, scale and error bit; absent in older snapshots.
    #[serde(flatten)]
    pub limits: Limits,
}

/// The shadow wire mapping of a key in contract.json; read back for the
/// previous-contract checks (absent in older snapshots: no mapping).
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Mapping {
    /// Number to the accept aliases of the values that have any.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "accepts_by_code"
    )]
    pub enum_accept: Option<BTreeMap<i64, Vec<String>>>,
    #[serde(flatten)]
    pub wire: KeyWire,
}

/// The accept aliases by code, read through a flattened key whose JSON object keys arrive as
/// strings (serde does not convert map keys inside `flatten`).
///
/// @version 1
/// @req REQ-ING-SHD-006
fn accepts_by_code<'de, D>(d: D) -> Result<Option<BTreeMap<i64, Vec<String>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<BTreeMap<String, Vec<String>>> = Option::deserialize(d)?;
    raw.map(|m| {
        m.into_iter()
            .map(|(k, v)| k.parse().map(|c| (c, v)).map_err(serde::de::Error::custom))
            .collect()
    })
    .transpose()
}

impl Mapping {
    /// The mapping of key `k`.
    ///
    /// @version 1
    /// @req REQ-ING-SHD-001
    fn of(k: &super::keys::Key) -> Self {
        Mapping {
            enum_accept: k.enum_values.as_ref().and_then(enum_accepts),
            wire: k.wire.clone(),
        }
    }
}

/// The limits and shadow wire mapping of a key in contract.json (written flat, beside the
/// other key fields).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Limits {
    /// The shadow wire mapping, only where declared; the previous-contract checks read it back.
    #[serde(flatten)]
    pub mapping: Mapping,
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub allowed: Option<Vec<i64>>,
    #[serde(default)]
    pub unset: Option<i64>,
    #[serde(default = "one")]
    pub scale: f64,
    /// The error group and bit.
    #[serde(default)]
    pub error: Option<ErrorRef>,
}

impl Limits {
    /// The limits and mapping of key `k`.
    ///
    /// v2: the shadow wire mapping.
    /// @version 2
    /// @req REQ-ING-RNG-001
    /// @req REQ-ING-ERR-001
    /// @req REQ-ING-SHD-001
    fn of(k: &super::keys::Key) -> Self {
        Limits {
            mapping: Mapping::of(k),
            range: k.range,
            allowed: k.allowed.clone(),
            unset: k.unset,
            scale: k.scale,
            error: k.error.clone(),
        }
    }
}

/// Serde default of a scale: 1.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn one() -> f64 {
    1.0
}

/// One principal.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PrincipalJson {
    pub name: String,
    pub uid: u32,
    pub token: u16,
    pub kind: String,
    pub flags: u8,
    pub introduced_in: String,
    pub queue_bound: u32,
    pub latency_budget_ms: u32,
    /// KSET v1 digest, lower-case hex, for each minor 0..=contract minor.
    pub digests: Vec<String>,
}

/// One command type.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CommandJson {
    pub path: String,
    pub persist: String,
    pub owner: String,
    /// Why the command is not on the wire; absent unless internal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal: Option<String>,
    /// Name to code of the outcome enum; absent unless the command names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<BTreeMap<String, i64>>,
}

/// Name of a principal kind: `unix`, `inproc` or `uart` (contract.json and the SDK context).
///
/// v2: shared with the SDK context.
/// @version 2
/// @req REQ-ING-TIV-009
/// @req REQ-ING-SDK-001
pub(super) fn kind_name(k: Kind) -> String {
    match k {
        Kind::Unix => "unix",
        Kind::Inproc => "inproc",
        Kind::Uart => "uart",
    }
    .to_string()
}

/// Name of a rights cell in contract.json, `None` for no rights.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn access_name(bits: u8) -> Option<String> {
    match bits & 3 {
        1 => Some("r".to_string()),
        2 => Some("w".to_string()),
        3 => Some("rw".to_string()),
        _ => None,
    }
}

/// The snapshot of `tables`, generated by ingot `commit`.
///
/// v2: the declared variants.
/// v3: the unit vocabulary; principals from principals_json.
/// v4: the error groups.
/// v5: the shadow wire mapping.
/// v6: the codes of the enums topic vars name.
/// @version 6
/// @req REQ-ING-TOP-008
/// @req REQ-ING-SHD-002
/// @req REQ-ING-ERR-001
/// @req REQ-ING-TIV-015
/// @req REQ-ING-RNG-001
/// @req REQ-ING-TIV-009
pub fn snapshot(tables: &Tables, commit: &str) -> Snapshot {
    Snapshot {
        tiv: "1.0".to_string(),
        contract: contract_json(tables),
        variant: tables.variant.clone(),
        variants: tables.variants.clone(),
        units: tables.units.clone().unwrap_or_default(),
        error_groups: tables.error_groups.clone(),
        model_hash: tables.model_hash.clone(),
        ingot: IngotJson {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: commit.to_string(),
        },
        keys: tables.keys.iter().map(key_json).collect(),
        reserved: tables.reserved.clone(),
        principals: principals_json(tables),
        rights: rights_json(tables),
        commands: commands_json(tables),
        var_enums: var_enums_json(tables),
        shadow: super::shadow::json(tables),
    }
}

/// Name to code of each enum a topic var of `tables` names, by `namespace::enum`.
///
/// @version 1
/// @req REQ-ING-TOP-008
fn var_enums_json(tables: &Tables) -> BTreeMap<String, BTreeMap<String, i64>> {
    super::constants::var_enums(tables)
        .into_iter()
        .map(|(r, e)| (r.to_string(), enum_numbers(e)))
        .collect()
}

/// Contract name and version of the snapshot.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn contract_json(tables: &Tables) -> ContractJson {
    ContractJson {
        name: tables.name.clone(),
        major: tables.version.major,
        minor: tables.version.minor,
        patch: tables.patch,
    }
}

/// Command types of the snapshot.
///
/// v2: persistence names from Persist::name.
/// v3: the internal reason.
/// v4: the outcome names and codes.
/// @version 4
/// @req REQ-ING-TIV-009
/// @req REQ-ING-CMD-001
/// @req REQ-ING-TIV-018
fn commands_json(tables: &Tables) -> Vec<CommandJson> {
    tables
        .commands
        .iter()
        .map(|c| CommandJson {
            path: c.path.clone(),
            persist: c.persist.name().to_string(),
            owner: c.owner.clone(),
            internal: c.internal.clone(),
            outcome: c.outcome.as_ref().map(enum_numbers),
        })
        .collect()
}

/// One key of the snapshot.
///
/// v2: deferred_ms is the effective interval (0 for non-deferred keys).
/// v3: the enum is the variant-resolved one, with its wire strings.
/// v4: persistence names from Persist::name.
/// v5: range, allowed set, sentinel and scale.
/// v6: the error group and bit.
/// v7: the struct value header; the enum maps from enum_maps.
/// @version 7
/// @req REQ-ING-HDR-001
/// @req REQ-ING-ERR-001
/// @req REQ-ING-TIV-009
/// @req REQ-ING-TIV-016
/// @req REQ-ING-RNG-001
fn key_json(k: &super::keys::Key) -> KeyJson {
    let (enum_values, enum_wire) = enum_maps(k);
    KeyJson {
        path: k.path.clone(),
        id: k.id,
        ty: k.ty.name().to_string(),
        introduced_in: k.introduced.to_string(),
        persist: k.persist.name().to_string(),
        deferred_ms: k.effective_deferred_ms(),
        rate_per_day: k.rate_per_day,
        rate_burst: k.rate_burst,
        max_size: k.max_size,
        secret: k.secret,
        survives_reset: k.survives_reset,
        mfg: k.mfg,
        alias_of: k.alias_of.clone(),
        enum_values,
        enum_wire,
        command: k.command.as_ref().map(|(path, _)| path.clone()),
        wk: k.wk.map(|w| WK_RULES[w].name.to_string()),
        doc: k.doc.clone(),
        unit: k.unit.clone(),
        header: HeaderJson::of(k),
        limits: Limits::of(k),
    }
}

/// Name to number and number to wire string of key `k`'s resolved enum; both `None` without one.
///
/// @version 1
/// @req REQ-ING-TIV-016
fn enum_maps(k: &super::keys::Key) -> EnumMaps {
    let e = k.enum_values.as_ref();
    (e.map(enum_numbers), e.map(enum_wires))
}

/// A key's enum in contract.json: name to number, and number to wire string.
type EnumMaps = (Option<BTreeMap<String, i64>>, Option<BTreeMap<i64, String>>);

/// Number to accept aliases of the values of a resolved enum that have any; `None` when none do.
///
/// @version 1
/// @req REQ-ING-SHD-001
fn enum_accepts(e: &ResolvedEnum) -> Option<BTreeMap<i64, Vec<String>>> {
    let map: BTreeMap<i64, Vec<String>> = e
        .values()
        .filter(|m| !m.accept.is_empty())
        .map(|m| (m.value, m.accept.clone()))
        .collect();
    (!map.is_empty()).then_some(map)
}

/// Name to number of a resolved enum.
///
/// @version 1
/// @req REQ-ING-TIV-016
fn enum_numbers(e: &ResolvedEnum) -> BTreeMap<String, i64> {
    e.iter().map(|(n, m)| (n.clone(), m.value)).collect()
}

/// Number to wire string of a resolved enum.
///
/// @version 1
/// @req REQ-ING-TIV-016
fn enum_wires(e: &ResolvedEnum) -> BTreeMap<i64, String> {
    e.values().map(|m| (m.value, m.wire.clone())).collect()
}

/// Every principal of the snapshot, with its digests.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn principals_json(tables: &Tables) -> Vec<PrincipalJson> {
    digests(tables)
        .iter()
        .enumerate()
        .map(|(p, d)| principal_json(tables, p, d))
        .collect()
}

/// One principal of the snapshot.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn principal_json(tables: &Tables, p: usize, digests: &[[u8; 32]]) -> PrincipalJson {
    let principal = &tables.principals[p];
    PrincipalJson {
        name: principal.name.clone(),
        uid: principal.uid,
        token: principal.token,
        kind: kind_name(principal.kind),
        flags: principal.flags,
        introduced_in: principal.introduced.to_string(),
        queue_bound: queue_bound(tables, p),
        latency_budget_ms: principal.latency_budget_ms,
        digests: digests.iter().map(|d| hex(d)).collect(),
    }
}

/// The non-empty rights cells, by principal and key path.
///
/// @version 1
/// @req REQ-ING-TIV-009
fn rights_json(tables: &Tables) -> BTreeMap<String, BTreeMap<String, String>> {
    tables
        .principals
        .iter()
        .zip(&tables.rights)
        .map(|(p, row)| {
            let cells = tables
                .keys
                .iter()
                .zip(row)
                .filter_map(|(k, bits)| access_name(*bits).map(|a| (k.path.clone(), a)))
                .collect();
            (p.name.clone(), cells)
        })
        .collect()
}
