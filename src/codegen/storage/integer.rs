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
pub fn collect_integer_storage(
    model: &DataModel,
    ns_id: u16,
) -> Result<Vec<IntTypeStorage>, String> {
    use crate::model::key::KeyEncoding;

    let mut result = Vec::new();

    for desc in INT_TYPES {
        // Collect keys of this type along with their encoded 32-bit key value and default
        let mut keys_with_meta: Vec<(u32, &KeyDef)> = Vec::new();

        for (pos, class) in model.classes.iter().enumerate() {
            let c_ns_id = class.namespace_id.unwrap_or(ns_id);
            let c_idx = class.class_index.unwrap_or(pos as u8);
            let mut type_id_counter: u16 = 0;
            for key in &class.keys {
                if key.data_type == desc.data_type {
                    let encoding = KeyEncoding {
                        namespace: c_ns_id,
                        class: c_idx,
                        id: type_id_counter,
                        data_type: key.data_type.type_code(),
                        thread_safe: key.thread_safe,
                        derived: false,
                        read_only: key.read_only,
                    };
                    keys_with_meta.push((encoding.encode(), key));
                    type_id_counter += 1;
                }
            }
        }

        if keys_with_meta.is_empty() {
            continue;
        }

        let encoded_keys: Vec<u32> = keys_with_meta.iter().map(|(k, _)| *k).collect();
        let ph = hash::generate(&encoded_keys, 100)
            .ok_or_else(|| format!("Failed to generate perfect hash for {} keys", desc.label))?;

        let storage = build_int_type_storage(desc, &keys_with_meta, &ph);
        result.push(storage);
    }

    Ok(result)
}

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
        g_table_str: format_i32_array(&ph.g_table),
        init_table_str: init_values.join(", "),
    }
}

/// Pick the smallest signed C integer type that can hold all G table values.
fn select_g_table_type(g: &[i32]) -> &'static str {
    let min = g.iter().copied().min().unwrap_or(0);
    let max = g.iter().copied().max().unwrap_or(0);

    if min >= i8::MIN as i32 && max <= i8::MAX as i32 {
        "int8_t"
    } else if min >= i16::MIN as i32 && max <= i16::MAX as i32 {
        "int16_t"
    } else {
        "int32_t"
    }
}

fn format_i32_array(values: &[i32]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

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
    fn select_g_type_fits_i8() {
        assert_eq!(select_g_table_type(&[0, 1, -1, 127, -128]), "int8_t");
    }

    #[test]
    fn select_g_type_fits_i16() {
        assert_eq!(select_g_table_type(&[0, 128, -129]), "int16_t");
    }

    #[test]
    fn select_g_type_needs_i32() {
        assert_eq!(select_g_table_type(&[0, 40000, -40000]), "int32_t");
    }

    #[test]
    fn collect_from_minimal_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/minimal.toml")).unwrap();
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
