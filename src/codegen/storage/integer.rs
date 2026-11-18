use super::select_g_table_type;
use crate::codegen::{encoded_keys, join_list};
use crate::hash::{self, PerfectHash};
use crate::model::schema::{DataModel, DataType, KeyDef};
use serde::Serialize;

/// Collected keys of one integer type, with their perfect hash and defaults.
#[derive(Debug, Serialize)]
pub struct IntTypeStorage {
    /// Display label (e.g. "uint8_t")
    pub label: String,
    /// C type name (e.g. "uint8_t")
    pub c_type: String,
    /// Prefix for variable names (e.g. "u8")
    pub prefix: String,
    /// Prefix for macro names (e.g. "U8")
    #[serde(rename = "PREFIX")]
    pub prefix_upper: String,
    /// Function name suffix (e.g. "UINT8")
    pub suffix: String,
    /// Number of keys
    pub num_keys: usize,
    /// Perfect hash seed 1
    pub seed1: u32,
    /// Perfect hash seed 2
    pub seed2: u32,
    /// G table size
    pub g_size: usize,
    /// C type for G table elements
    pub g_c_type: String,
    /// G table values formatted as C initializer list
    pub g_table_str: String,
    /// Default values formatted as C initializer list (in hash order)
    pub init_table_str: String,
}

/// Descriptor for how to handle each integer data type.
struct TypeDesc {
    data_type: DataType,
    label: &'static str,
    c_type: &'static str,
    prefix: &'static str,
    prefix_upper: &'static str,
    suffix: &'static str,
}

const INT_TYPES: &[TypeDesc] = &[
    TypeDesc {
        data_type: DataType::Uint8,
        label: "uint8_t",
        c_type: "uint8_t",
        prefix: "u8",
        prefix_upper: "U8",
        suffix: "UINT8",
    },
    TypeDesc {
        data_type: DataType::Int8,
        label: "int8_t",
        c_type: "int8_t",
        prefix: "s8",
        prefix_upper: "S8",
        suffix: "SINT8",
    },
    TypeDesc {
        data_type: DataType::Uint16,
        label: "uint16_t",
        c_type: "uint16_t",
        prefix: "u16",
        prefix_upper: "U16",
        suffix: "UINT16",
    },
    TypeDesc {
        data_type: DataType::Int16,
        label: "int16_t",
        c_type: "int16_t",
        prefix: "s16",
        prefix_upper: "S16",
        suffix: "SINT16",
    },
    TypeDesc {
        data_type: DataType::Uint32,
        label: "uint32_t",
        c_type: "uint32_t",
        prefix: "u32",
        prefix_upper: "U32",
        suffix: "UINT32",
    },
    TypeDesc {
        data_type: DataType::Int32,
        label: "int32_t",
        c_type: "int32_t",
        prefix: "s32",
        prefix_upper: "S32",
        suffix: "SINT32",
    },
];

/// Collect all integer keys from the model, grouped by type, with perfect hashes.
///
/// v2: keys and their IDs come from the shared codegen::encoded_keys.
/// @version 2
/// @req REQ-ING-GEN-005
pub fn collect_integer_storage(
    model: &DataModel,
    ns_id: u16,
) -> Result<Vec<IntTypeStorage>, String> {
    let mut result = Vec::new();

    for desc in INT_TYPES {
        let keys_with_meta = encoded_keys(model, ns_id, |k| k.data_type == desc.data_type);
        if keys_with_meta.is_empty() {
            continue;
        }

        let ids: Vec<u32> = keys_with_meta.iter().map(|(k, _)| *k).collect();
        let ph = hash::generate_deterministic(&ids, 100)
            .ok_or_else(|| format!("Failed to generate perfect hash for {} keys", desc.label))?;

        let storage = build_int_type_storage(desc, &keys_with_meta, &ph);
        result.push(storage);
    }

    Ok(result)
}

/// Build the storage group of one integer type in perfect-hash slot order.
///
/// v2: the G table is joined by the shared codegen::join_list.
/// @version 2
/// @req REQ-ING-GEN-005
fn build_int_type_storage(
    desc: &TypeDesc,
    keys_with_meta: &[(u32, &KeyDef)],
    ph: &PerfectHash,
) -> IntTypeStorage {
    let num_keys = keys_with_meta.len();

    // Build init table in hash order (index -> default value)
    let mut init_values: Vec<String> = vec!["0".to_string(); num_keys];
    for (encoded_key, key_def) in keys_with_meta {
        let idx = ph.lookup(*encoded_key);
        let default_str = key_def
            .default
            .as_ref()
            .map(|v| format_c_value(v, desc.data_type))
            .unwrap_or_else(|| "0".to_string());
        init_values[idx] = default_str;
    }

    // Select smallest C type for G table
    let g_c_type = select_g_table_type(&ph.g_table);

    IntTypeStorage {
        label: desc.label.to_string(),
        c_type: desc.c_type.to_string(),
        prefix: desc.prefix.to_string(),
        prefix_upper: desc.prefix_upper.to_string(),
        suffix: desc.suffix.to_string(),
        num_keys,
        seed1: ph.seed1,
        seed2: ph.seed2,
        g_size: ph.g_table.len(),
        g_c_type: g_c_type.to_string(),
        g_table_str: join_list(&ph.g_table),
        init_table_str: init_values.join(", "),
    }
}

/// C literal of an integer key's default (booleans as 1 or 0).
///
/// @version 1
/// @req REQ-ING-GEN-005
fn format_c_value(val: &toml::Value, _data_type: DataType) -> String {
    match val {
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Boolean(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        toml::Value::Float(f) => format!("{f}"),
        toml::Value::String(s) => s.clone(),
        _ => "0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_from_minimal_model() {
        let model: DataModel =
            DataModel::from_toml(include_str!("../../../examples/minimal.toml")).unwrap();
        let storages = collect_integer_storage(&model, 0).unwrap();

        // minimal.toml has: temperature(uint16), mode(uint8), update_interval_ms(uint32)
        // is_connected is bool (not integer), device_name is string
        // mode=uint8(1 key), temperature=uint16(1 key), update_interval_ms=uint32(1 key)
        assert_eq!(storages.len(), 3);
        for s in &storages {
            assert!(s.num_keys > 0);
            assert!(s.g_size > 0);
        }
    }
}
