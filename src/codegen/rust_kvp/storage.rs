use crate::hash::{self, PerfectHash};
use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType, KeyDef};
use serde::Serialize;

use crate::codegen::{resolve_class_idx, resolve_ns_id};

/// One group of same-typed integer/bool keys sharing a perfect hash table.
#[derive(Debug, Serialize)]
pub struct RustScalarGroup {
    /// Rust scalar type for the group (e.g. "u8", "bool").
    pub rust_type: String,
    /// Struct field name backing this group (e.g. "u8_values").
    pub field_name: String,
    pub num_keys: usize,
    pub seed1: u32,
    pub seed2: u32,
    pub g_size: usize,
    /// G table values, comma-joined (no surrounding brackets).
    pub g_table_str: String,
    /// Default values in hash order, comma-joined (no surrounding brackets).
    pub default_table_str: String,
    /// Encoded keys in hash order, comma-joined — used to validate the
    /// generic by-key lookup (perfect hashes only guarantee correctness for
    /// keys that were part of the original set).
    pub keys_table_str: String,
}

/// A named accessor backed by a scalar group (int or bool).
#[derive(Debug, Clone, Serialize)]
pub struct RustScalarAccessor {
    pub method_name: String,
    pub const_name: String,
    pub hex_value: String,
    pub field_name: String,
    pub idx: usize,
    pub rust_type: String,
    pub read_only: bool,
    pub persistent: bool,
}

/// A named accessor backed by its own fixed-size `[u8; N]` field (string/binary).
#[derive(Debug, Clone, Serialize)]
pub struct RustBytesAccessor {
    pub method_name: String,
    pub const_name: String,
    pub hex_value: String,
    pub field_name: String,
    pub max_size: usize,
    pub is_binary: bool,
    pub read_only: bool,
    pub persistent: bool,
    /// Default value as a Rust byte-string literal (e.g. `b"ingot-device"`).
    pub default_literal: String,
}

const INT_TYPES: &[DataType] = &[
    DataType::Uint8,
    DataType::Int8,
    DataType::Uint16,
    DataType::Int16,
    DataType::Uint32,
    DataType::Int32,
];

fn field_name_for(data_type: DataType) -> &'static str {
    match data_type {
        DataType::Bool => "bool_values",
        DataType::Uint8 => "u8_values",
        DataType::Int8 => "i8_values",
        DataType::Uint16 => "u16_values",
        DataType::Int16 => "i16_values",
        DataType::Uint32 => "u32_values",
        DataType::Int32 => "i32_values",
        DataType::String | DataType::Binary => unreachable!("byte types have no shared field"),
    }
}

/// A key together with its resolved names/metadata, independent of storage kind.
struct ResolvedKey<'a> {
    encoded: u32,
    key: &'a KeyDef,
    method_name: String,
    const_name: String,
}

/// Walk every class/key in the model, resolving names and the 32-bit encoding.
fn resolve_all_keys(model: &DataModel, ns_id: u16) -> Vec<ResolvedKey<'_>> {
    let mut out = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_id = resolve_ns_id(class, ns_id);
        let c_idx = resolve_class_idx(class, pos);
        let ns_name = class
            .namespace_name
            .as_deref()
            .unwrap_or(&model.meta.id)
            .to_string();

        for (key_pos, key) in class.keys.iter().enumerate() {
            let encoding = KeyEncoding {
                namespace: c_ns_id,
                class: c_idx,
                id: key.key_index.unwrap_or(key_pos as u16),
                data_type: key.data_type.type_code(),
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };
            let encoded = encoding.encode();

            let key_name = key.id.to_lowercase().replace([' ', '-'], "_");
            let ns_lower = ns_name.to_lowercase();
            let class_lower = class.id.to_lowercase();
            let method_name = format!("{ns_lower}_{class_lower}_{key_name}");

            let ns_upper = ns_name.to_uppercase();
            let class_upper = class.id.to_uppercase();
            let key_upper = key.id.to_uppercase().replace(' ', "_");
            let const_name = format!("DM_KEY_{ns_upper}_{class_upper}_{key_upper}");

            out.push(ResolvedKey {
                encoded,
                key,
                method_name,
                const_name,
            });
        }
    }

    out
}

/// Collect one perfect-hash-backed group per integer type present in the model.
pub fn collect_int_groups(
    model: &DataModel,
    ns_id: u16,
) -> Result<(Vec<RustScalarGroup>, Vec<RustScalarAccessor>), String> {
    let resolved = resolve_all_keys(model, ns_id);
    let mut groups = Vec::new();
    let mut accessors = Vec::new();

    for &data_type in INT_TYPES {
        let keys: Vec<&ResolvedKey> = resolved
            .iter()
            .filter(|k| k.key.data_type == data_type)
            .collect();
        if keys.is_empty() {
            continue;
        }

        let encoded_keys: Vec<u32> = keys.iter().map(|k| k.encoded).collect();
        let ph = hash::generate_deterministic(&encoded_keys, 100)
            .ok_or_else(|| format!("Failed to generate perfect hash for {data_type:?} keys"))?;

        let field_name = field_name_for(data_type).to_string();
        let (group, mut group_accessors) =
            build_scalar_group(data_type.rust_type(), &field_name, &keys, &ph, |v| {
                format_int_default(v)
            });
        groups.push(group);
        accessors.append(&mut group_accessors);
    }

    Ok((groups, accessors))
}

/// Collect the single bool group, if any bool keys exist.
pub fn collect_bool_group(
    model: &DataModel,
    ns_id: u16,
) -> Result<(Option<RustScalarGroup>, Vec<RustScalarAccessor>), String> {
    let resolved = resolve_all_keys(model, ns_id);
    let keys: Vec<&ResolvedKey> = resolved
        .iter()
        .filter(|k| k.key.data_type == DataType::Bool)
        .collect();
    if keys.is_empty() {
        return Ok((None, Vec::new()));
    }

    let encoded_keys: Vec<u32> = keys.iter().map(|k| k.encoded).collect();
    let ph = hash::generate_deterministic(&encoded_keys, 100)
        .ok_or("Failed to generate perfect hash for bool keys")?;

    let (group, accessors) =
        build_scalar_group("bool", "bool_values", &keys, &ph, format_bool_default);
    Ok((Some(group), accessors))
}

fn build_scalar_group(
    rust_type: &str,
    field_name: &str,
    keys: &[&ResolvedKey],
    ph: &PerfectHash,
    format_default: impl Fn(&Option<toml::Value>) -> String,
) -> (RustScalarGroup, Vec<RustScalarAccessor>) {
    let num_keys = keys.len();
    let mut defaults: Vec<String> = vec![String::new(); num_keys];
    let mut key_order: Vec<u32> = vec![0; num_keys];
    let mut accessors = Vec::with_capacity(num_keys);

    for rk in keys {
        let idx = ph.lookup(rk.encoded);
        defaults[idx] = format_default(&rk.key.default);
        key_order[idx] = rk.encoded;

        accessors.push(RustScalarAccessor {
            method_name: rk.method_name.clone(),
            const_name: rk.const_name.clone(),
            hex_value: format!("{:#010X}", rk.encoded),
            field_name: field_name.to_string(),
            idx,
            rust_type: rust_type.to_string(),
            read_only: rk.key.read_only,
            persistent: rk.key.persistent,
        });
    }

    let group = RustScalarGroup {
        rust_type: rust_type.to_string(),
        field_name: field_name.to_string(),
        num_keys,
        seed1: ph.seed1,
        seed2: ph.seed2,
        g_size: ph.g_table.len(),
        g_table_str: join_i32(&ph.g_table),
        default_table_str: defaults.join(", "),
        keys_table_str: key_order
            .iter()
            .map(|k| format!("{k:#010X}"))
            .collect::<Vec<_>>()
            .join(", "),
    };

    (group, accessors)
}

fn join_i32(values: &[i32]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_int_default(default: &Option<toml::Value>) -> String {
    match default {
        Some(toml::Value::Integer(i)) => i.to_string(),
        Some(toml::Value::Boolean(b)) => if *b { "1" } else { "0" }.to_string(),
        _ => "0".to_string(),
    }
}

fn format_bool_default(default: &Option<toml::Value>) -> String {
    let is_true = matches!(default, Some(toml::Value::Boolean(true)))
        || matches!(default, Some(toml::Value::Integer(i)) if *i != 0);
    if is_true {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

/// Collect string/binary keys, each backed by its own fixed-size field
/// (max_size varies per key, so these can't share a uniform array the way
/// scalar types do — no perfect hash needed since access is name-only).
pub fn collect_bytes_accessors(model: &DataModel, ns_id: u16) -> Vec<RustBytesAccessor> {
    let resolved = resolve_all_keys(model, ns_id);

    resolved
        .iter()
        .filter(|k| matches!(k.key.data_type, DataType::String | DataType::Binary))
        .map(|rk| {
            let max_size = rk.key.max_size.unwrap_or(0);
            let is_binary = rk.key.data_type == DataType::Binary;
            let default_literal = format_bytes_default(&rk.key.default);

            RustBytesAccessor {
                method_name: rk.method_name.clone(),
                const_name: rk.const_name.clone(),
                hex_value: format!("{:#010X}", rk.encoded),
                field_name: rk.method_name.clone(),
                max_size,
                is_binary,
                read_only: rk.key.read_only,
                persistent: rk.key.persistent,
                default_literal,
            }
        })
        .collect()
}

/// Format a default string value as a Rust byte-string literal, escaping any
/// bytes that aren't printable ASCII or the standard escape set.
fn format_bytes_default(default: &Option<toml::Value>) -> String {
    let s = match default {
        Some(toml::Value::String(s)) => s.clone(),
        _ => String::new(),
    };

    let mut out = String::with_capacity(s.len() + 3);
    out.push_str("b\"");
    for byte in s.bytes() {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7E => out.push(byte as char),
            _ => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_int_groups_from_minimal_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/minimal.toml")).unwrap();
        let (groups, accessors) = collect_int_groups(&model, 0).unwrap();

        // uint16 (temperature), uint8 (mode), uint32 (update_interval_ms)
        assert_eq!(groups.len(), 3);
        assert_eq!(accessors.len(), 3);
        for g in &groups {
            assert!(g.num_keys > 0);
            assert!(g.g_size > 0);
        }
    }

    #[test]
    fn collect_bool_group_from_minimal_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/minimal.toml")).unwrap();
        let (group, accessors) = collect_bool_group(&model, 0).unwrap();
        let group = group.expect("should have a bool group");
        assert_eq!(group.num_keys, 1);
        assert_eq!(accessors.len(), 1);
        assert_eq!(accessors[0].method_name, "example_status_is_connected");
    }

    #[test]
    fn collect_bytes_accessors_from_minimal_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/minimal.toml")).unwrap();
        let accessors = collect_bytes_accessors(&model, 0);
        assert_eq!(accessors.len(), 1);
        assert_eq!(accessors[0].method_name, "example_config_device_name");
        assert_eq!(accessors[0].max_size, 32);
        assert!(!accessors[0].read_only);
        assert_eq!(accessors[0].default_literal, "b\"ingot-device\"");
    }

    #[test]
    fn no_bools_returns_none() {
        let toml_str = r#"
            [meta]
            id = "test"
            version = "1.0.0"
            [[classes]]
            id = "data"
            [[classes.keys]]
            id = "counter"
            type = "uint32"
            default = 0
        "#;
        let model: DataModel = toml::from_str(toml_str).unwrap();
        let (group, accessors) = collect_bool_group(&model, 0).unwrap();
        assert!(group.is_none());
        assert!(accessors.is_empty());
    }
}
