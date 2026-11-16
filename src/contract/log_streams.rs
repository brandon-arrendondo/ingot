//! Per-principal log streams: resolution for a variant, the `dmt_log_streams` rows and the
//! contract.json entries. The table is runtime configuration: no digest, keyspace file or
//! previous-contract check reads it.

use super::emit::c_string;
use super::keys::Errors;
use super::model::Tables;
use super::schema::{
    level_code, mask, trigger_code, Backing, FlushTrigger, Level, LogDef, PrincipalDef, BACKINGS,
};
use super::types::Version;
use serde::{Deserialize, Serialize};

/// A principal's log stream, its name resolved and its version parsed.
#[derive(Debug)]
pub struct LogStream {
    pub name: String,
    pub def: LogDef,
    pub introduced: Version,
}

/// The stream name of `def` on principal `principal`: the declared one, else the principal's
/// name with each dot an underscore.
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the name.
pub fn stream_name(principal: &str, def: &LogDef) -> String {
    def.name
        .clone()
        .unwrap_or_else(|| principal.replace('.', "_"))
}

/// Resolve principal `principal`'s log declaration; a malformed introduced_in is refused.
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @req REQ-ING-LOG-004
/// @return the stream.
pub fn resolve(principal: &str, def: LogDef, errors: &mut Errors) -> LogStream {
    let name = stream_name(principal, &def);
    let introduced = Version::parse(&def.introduced_in).unwrap_or_else(|e| {
        errors.push(format!("log stream {name}: introduced_in {e}"));
        Version { major: 0, minor: 0 }
    });
    LogStream {
        name,
        def,
        introduced,
    }
}

/// Every (where, variant) the log declarations name, for the declared-variant check.
///
/// @version 1
/// @req REQ-ING-LOG-004
/// @return the pairs.
pub fn variant_uses(principals: &[PrincipalDef]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in principals {
        let Some(def) = &p.log else { continue };
        let at = format!("log stream {}", stream_name(&p.name, def));
        let named = def
            .backing_by_variant
            .keys()
            .chain(def.allowed_backings_by_variant.keys());
        out.extend(named.map(|v| (at.clone(), v.clone())));
    }
    out
}

impl LogStream {
    /// The boot backing on `variant` (`""`: the base).
    ///
    /// @version 1
    /// @req REQ-ING-LOG-001
    /// @return the backing.
    pub fn backing(&self, variant: &str) -> Backing {
        let by = self.def.backing_by_variant.get(variant);
        by.copied().unwrap_or(self.def.backing)
    }

    /// The allowed-backing mask on `variant` (`1 << code` each; off always set).
    ///
    /// @version 1
    /// @req REQ-ING-LOG-001
    /// @return the mask.
    pub fn allowed(&self, variant: &str) -> u8 {
        let declared = self
            .def
            .allowed_backings_by_variant
            .get(variant)
            .or(self.def.allowed_backings.as_ref());
        let listed = match declared {
            Some(list) => mask(list, Backing::code),
            None => mask(&[self.backing(variant)], Backing::code),
        };
        (listed | mask(&[Backing::Off], Backing::code)) as u8
    }

    /// Whether `variant` allows a backing other than off.
    ///
    /// @version 1
    /// @req REQ-ING-LOG-002
    /// @return true when the stream may be enabled.
    pub fn may_enable(&self, variant: &str) -> bool {
        self.allowed(variant) & !(1 << Backing::Off.code()) != 0
    }

    /// Whether `variant` allows a flash backing.
    ///
    /// @version 1
    /// @req REQ-ING-LOG-003
    /// @return true when the stream may write flash.
    pub fn may_flash(&self, variant: &str) -> bool {
        let allowed = self.allowed(variant);
        BACKINGS
            .iter()
            .any(|b| b.flash() && allowed & (1 << b.code()) != 0)
    }

    /// The level masks (RAM, file).
    ///
    /// @version 1
    /// @req REQ-ING-LOG-001
    /// @return (ram, file).
    pub fn levels(&self) -> (u16, u16) {
        (
            mask(&self.def.default_level_ram, level_code),
            mask(&self.def.default_level_file, level_code),
        )
    }
}

/// Every log stream of `tables`, with its principal's index, in principal table order.
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the (principal index, stream) pairs.
pub fn streams(tables: &Tables) -> impl Iterator<Item = (usize, &LogStream)> {
    tables
        .principals
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.log.as_ref().map(|l| (i, l)))
}

/// The `dmt_log_streams` rows (one zero row when there are none, so the array is never empty).
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the C initialisers.
pub fn rows(tables: &Tables) -> Vec<String> {
    let rows: Vec<String> = streams(tables).map(|(p, s)| row(tables, p, s)).collect();
    if rows.is_empty() {
        vec![
            "{ 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0x0000u, 0u, 0u, 0x00u, 0x00u, 0x00u, 0u }"
                .into(),
        ]
    } else {
        rows
    }
}

/// One `dmt_log_streams` row of stream `s` of principal `p`, for the tables' variant.
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the C initialiser.
fn row(tables: &Tables, p: usize, s: &LogStream) -> String {
    let d = &s.def;
    let id = |path: &str| format!("{:#010X}U", tables.id_of(Some(path)));
    let (ram, file) = s.levels();
    let v = &tables.variant;
    format!(
        "{{ {}u, {}u, {}u, {}u, {}u, {}u, {}, {}, {p}u, {:#06x}u, {}u, {}u, {:#04x}u, {:#04x}u, {:#04x}u, 0u }}",
        d.ram_cap_bytes, d.max_line_bytes, d.flash_seg_bytes, d.flash_seg_count,
        d.flash_rate_bytes_per_day, d.flash_rate_burst_bytes, id(&d.ctl_key),
        id(&d.drop_counter_key), mask(&d.flush, trigger_code),
        s.introduced.minor_in(tables.version.major), s.backing(v).code(), s.allowed(v), ram, file
    )
}

/// `dmt_log_stream_names`: each stream's name in row order (`NULL` when there are none).
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the C string literals.
pub fn names(tables: &Tables) -> Vec<String> {
    let names: Vec<String> = streams(tables)
        .map(|(_, s)| c_string(Some(&s.name)))
        .collect();
    if names.is_empty() {
        vec![c_string(None)]
    } else {
        names
    }
}

/// One log stream in contract.json, as the variant resolves it.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct LogStreamJson {
    pub name: String,
    pub principal: String,
    pub backing: Backing,
    pub allowed_backings: Vec<Backing>,
    pub ram_cap_bytes: u32,
    pub max_line_bytes: u32,
    pub default_level_ram: Vec<Level>,
    pub default_level_file: Vec<Level>,
    pub flash_seg_bytes: u32,
    pub flash_seg_count: u32,
    pub flash_rate_bytes_per_day: u32,
    pub flash_rate_burst_bytes: u32,
    pub flush: Vec<FlushTrigger>,
    pub ctl_key: String,
    pub drop_counter_key: String,
    pub introduced_in: String,
}

/// Every log stream of `tables` for contract.json.
///
/// @version 1
/// @req REQ-ING-LOG-005
/// @return the entries.
pub fn json(tables: &Tables) -> Vec<LogStreamJson> {
    streams(tables)
        .map(|(p, s)| stream_json(tables, p, s))
        .collect()
}

/// The contract.json entry of stream `s` of principal `p`.
///
/// @version 1
/// @req REQ-ING-LOG-005
/// @return the entry.
fn stream_json(tables: &Tables, p: usize, s: &LogStream) -> LogStreamJson {
    let (d, v) = (&s.def, &tables.variant);
    let allowed = s.allowed(v);
    LogStreamJson {
        name: s.name.clone(),
        principal: tables.principals[p].name.clone(),
        backing: s.backing(v),
        allowed_backings: BACKINGS
            .into_iter()
            .filter(|b| allowed & (1 << b.code()) != 0)
            .collect(),
        ram_cap_bytes: d.ram_cap_bytes,
        max_line_bytes: d.max_line_bytes,
        default_level_ram: d.default_level_ram.clone(),
        default_level_file: d.default_level_file.clone(),
        flash_seg_bytes: d.flash_seg_bytes,
        flash_seg_count: d.flash_seg_count,
        flash_rate_bytes_per_day: d.flash_rate_bytes_per_day,
        flash_rate_burst_bytes: d.flash_rate_burst_bytes,
        flush: d.flush.clone(),
        ctl_key: d.ctl_key.clone(),
        drop_counter_key: d.drop_counter_key.clone(),
        introduced_in: s.introduced.to_string(),
    }
}
