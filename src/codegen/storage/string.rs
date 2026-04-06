use crate::hash;
use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType, KeyDef};
use serde::Serialize;

/// Template context for string storage generation.
#[derive(Debug, Serialize)]
pub struct StringStorage {
    /// Total string keys
    pub total_keys: usize,
    /// Read-only string group (if any)
    pub ro: Option<StringGroup>,
    /// Read-write string group (if any)
    pub rw: Option<StringGroup>,
}

/// One group (RO or RW) of string keys with its perfect hash.
#[derive(Debug, Serialize)]
pub struct StringGroup {
    pub num_keys: usize,
    pub seed1: u32,
    pub seed2: u32,
    pub g_size: usize,
    pub g_c_type: String,
    pub g_table_str: String,
    /// Entries in hash order
    pub entries: Vec<StringEntry>,
}

/// A single string key's storage metadata (in hash order).
#[derive(Debug, Clone, Serialize)]
pub struct StringEntry {
    pub idx: usize,
    pub max_size: usize,
    /// C string literal for default value (e.g. `"hello"`)
    pub default_literal: String,
}

/// Collect string keys from the model, split into RO and RW groups.
pub fn collect_string_storage(
    model: &DataModel,
    ns_id: u16,
) -> Result<Option<StringStorage>, String> {
    let mut ro_keys: Vec<(u32, &KeyDef)> = Vec::new();
    let mut rw_keys: Vec<(u32, &KeyDef)> = Vec::new();

    for (class_idx, class) in model.classes.iter().enumerate() {
        let mut string_id_counter: u16 = 0;
        for key in &class.keys {
            if key.data_type == DataType::String {
                let encoding = KeyEncoding {
                    namespace: ns_id,
                    class: class_idx as u8,
                    id: string_id_counter,
                    data_type: key.data_type.type_code(),
                    thread_safe: key.thread_safe,
                    derived: false,
                    read_only: key.read_only,
                };
                let encoded = encoding.encode();
                if key.read_only {
                    ro_keys.push((encoded, key));
                } else {
                    rw_keys.push((encoded, key));
                }
                string_id_counter += 1;
            }
        }
    }

    if ro_keys.is_empty() && rw_keys.is_empty() {
        return Ok(None);
    }

    let ro = if ro_keys.is_empty() {
        None
    } else {
        Some(build_string_group(&ro_keys)?)
    };

    let rw = if rw_keys.is_empty() {
        None
    } else {
        Some(build_string_group(&rw_keys)?)
    };

    let total_keys = ro_keys.len() + rw_keys.len();
    Ok(Some(StringStorage { total_keys, ro, rw }))
}

fn build_string_group(keys: &[(u32, &KeyDef)]) -> Result<StringGroup, String> {
    let encoded_keys: Vec<u32> = keys.iter().map(|(k, _)| *k).collect();
    let ph = hash::generate(&encoded_keys, 100)
        .ok_or("Failed to generate perfect hash for string keys")?;

    let num_keys = keys.len();
    let mut entries = vec![None; num_keys];

    for (encoded_key, key_def) in keys {
        let idx = ph.lookup(*encoded_key);
        let max_size = key_def.max_size.unwrap_or(0);
        let default_str = key_def
            .default
            .as_ref()
            .map(|v| match v {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();

        entries[idx] = Some(StringEntry {
            idx,
            max_size,
            default_literal: escape_c_string(&default_str),
        });
    }

    let entries: Vec<StringEntry> = entries.into_iter().flatten().collect();
    assert_eq!(entries.len(), num_keys);

    let g_c_type = select_g_table_type(&ph.g_table);

    Ok(StringGroup {
        num_keys,
        seed1: ph.seed1,
        seed2: ph.seed2,
        g_size: ph.g_table.len(),
        g_c_type: g_c_type.to_string(),
        g_table_str: ph
            .g_table
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        entries,
    })
}

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

/// Escape a string for use as a C string literal (with quotes).
fn escape_c_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::DataModel;

    #[test]
    fn collect_from_battery_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/battery.toml")).unwrap();
        let storage = collect_string_storage(&model, 0).unwrap();

        // battery.toml has 1 string key: version (read_only, max_size=12, default="0.0.1")
        let storage = storage.expect("should have string keys");
        assert_eq!(storage.total_keys, 1);
        assert!(storage.ro.is_some());
        assert!(storage.rw.is_none());

        let ro = storage.ro.unwrap();
        assert_eq!(ro.num_keys, 1);
        assert_eq!(ro.entries[0].max_size, 12);
        assert_eq!(ro.entries[0].default_literal, "\"0.0.1\"");
    }

    #[test]
    fn collect_from_minimal_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/minimal.toml")).unwrap();
        let storage = collect_string_storage(&model, 0).unwrap();

        // minimal.toml has 1 string key: device_name (not read_only, max_size=32, default="device")
        let storage = storage.expect("should have string keys");
        assert_eq!(storage.total_keys, 1);
        assert!(storage.ro.is_none());
        assert!(storage.rw.is_some());

        let rw = storage.rw.unwrap();
        assert_eq!(rw.num_keys, 1);
        assert_eq!(rw.entries[0].max_size, 32);
        assert_eq!(rw.entries[0].default_literal, "\"ingot-device\"");
    }

    #[test]
    fn no_strings_returns_none() {
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
        let storage = collect_string_storage(&model, 0).unwrap();
        assert!(storage.is_none());
    }

    #[test]
    fn mixed_ro_rw_strings() {
        let toml_str = r#"
            [meta]
            id = "test"
            version = "1.0.0"
            [[classes]]
            id = "info"
            [[classes.keys]]
            id = "version"
            type = "string"
            max_size = 16
            default = "1.0.0"
            read_only = true
            [[classes.keys]]
            id = "name"
            type = "string"
            max_size = 32
            default = "widget"
        "#;
        let model: DataModel = toml::from_str(toml_str).unwrap();
        let storage = collect_string_storage(&model, 0).unwrap().unwrap();
        assert_eq!(storage.total_keys, 2);
        assert!(storage.ro.is_some());
        assert!(storage.rw.is_some());
        assert_eq!(storage.ro.unwrap().num_keys, 1);
        assert_eq!(storage.rw.unwrap().num_keys, 1);
    }

    #[test]
    fn escape_c_string_special_chars() {
        assert_eq!(escape_c_string("hello"), "\"hello\"");
        assert_eq!(escape_c_string(""), "\"\"");
        assert_eq!(escape_c_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(escape_c_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(escape_c_string("line\nnext"), "\"line\\nnext\"");
    }
}
