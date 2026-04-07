use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType};
use serde::Serialize;

/// A single persistent key entry for template rendering.
#[derive(Debug, Serialize)]
pub struct PersistenceEntry {
    /// Struct field name (lowercase, e.g. "config_update_interval_ms")
    pub field_name: String,
    /// Key #define name (e.g. "DM_KEY_EXAMPLE_CONFIG_UPDATE_INTERVAL_MS")
    pub define_name: String,
    /// C type for the struct field ("uint32_t", "bool", "char", etc.)
    pub c_type: String,
    /// True for string-type keys (rendered as char[max_size])
    pub is_string: bool,
    /// Array size for strings (max_size from schema)
    pub max_size: usize,
    /// C literal for default value
    pub default_literal: String,
    /// dm_val_t union field for integral types ("u32val", "bval", etc.)
    pub val_field: String,
    /// True for boolean keys
    pub is_bool: bool,
}

/// Template context for persistence storage generation.
#[derive(Debug, Serialize)]
pub struct PersistenceStorage {
    pub num_keys: usize,
    pub entries: Vec<PersistenceEntry>,
}

/// Collect all persistent keys from the model and build template context.
///
/// Returns `None` if no keys are marked persistent.
pub fn collect_persistence_storage(model: &DataModel, ns_id: u16) -> Option<PersistenceStorage> {
    let mut entries = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_id = class.namespace_id.unwrap_or(ns_id);
        let c_ns_name = class
            .namespace_name
            .as_deref()
            .unwrap_or(&model.meta.id)
            .to_uppercase();
        let c_idx = class.class_index.unwrap_or(pos as u8);
        let class_name = class.id.to_uppercase();
        for (key_pos, key) in class.keys.iter().enumerate() {
            if !key.persistent {
                continue;
            }

            let type_code = key.data_type.type_code();
            let encoding = KeyEncoding {
                namespace: c_ns_id,
                class: c_idx,
                id: key.key_index.unwrap_or(key_pos as u16),
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };
            // Validate encoding (should not fail after validation pass)
            let _ = encoding.encode();

            let key_name = key.id.to_uppercase().replace(' ', "_");
            let define_name = format!("DM_KEY_{c_ns_name}_{class_name}_{key_name}");
            let field_name = format!(
                "{}_{}",
                class.id.to_lowercase(),
                key.id.to_lowercase().replace(' ', "_")
            );

            let (c_type, val_field, is_string, is_bool) = match key.data_type {
                DataType::Bool => ("bool".to_string(), "bval".to_string(), false, true),
                DataType::Uint8 => ("uint8_t".to_string(), "u8val".to_string(), false, false),
                DataType::Int8 => ("int8_t".to_string(), "s8val".to_string(), false, false),
                DataType::Uint16 => ("uint16_t".to_string(), "u16val".to_string(), false, false),
                DataType::Int16 => ("int16_t".to_string(), "s16val".to_string(), false, false),
                DataType::Uint32 => ("uint32_t".to_string(), "u32val".to_string(), false, false),
                DataType::Int32 => ("int32_t".to_string(), "s32val".to_string(), false, false),
                DataType::String => ("char".to_string(), String::new(), true, false),
                DataType::Binary => ("uint8_t".to_string(), String::new(), true, false),
            };

            let max_size = if is_string {
                key.max_size.unwrap_or(0)
            } else {
                0
            };

            let default_literal = format_default(&key.default, key.data_type);

            entries.push(PersistenceEntry {
                field_name,
                define_name,
                c_type,
                is_string,
                max_size,
                default_literal,
                val_field,
                is_bool,
            });
        }
    }

    if entries.is_empty() {
        None
    } else {
        Some(PersistenceStorage {
            num_keys: entries.len(),
            entries,
        })
    }
}

/// Format a default value as a C literal for the persistence struct initializer.
fn format_default(default: &Option<toml::Value>, data_type: DataType) -> String {
    match default {
        Some(toml::Value::Boolean(b)) => if *b { "true" } else { "false" }.to_string(),
        Some(toml::Value::Integer(i)) => i.to_string(),
        Some(toml::Value::String(s)) => format!("\"{}\"", s.replace('"', "\\\"")),
        _ => match data_type {
            DataType::Bool => "false".to_string(),
            DataType::String | DataType::Binary => "\"\"".to_string(),
            _ => "0".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::DataModel;

    fn parse_model(toml_str: &str) -> DataModel {
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn minimal_has_one_persistent_key() {
        let model = parse_model(include_str!("../../../examples/minimal.toml"));
        let ps = collect_persistence_storage(&model, 0).unwrap();
        assert_eq!(ps.num_keys, 1);
        let entry = &ps.entries[0];
        assert_eq!(entry.field_name, "config_update_interval_ms");
        assert!(entry.define_name.contains("UPDATE_INTERVAL_MS"));
        assert_eq!(entry.c_type, "uint32_t");
        assert_eq!(entry.default_literal, "1000");
        assert!(!entry.is_string);
        assert!(!entry.is_bool);
        assert_eq!(entry.val_field, "u32val");
    }

    #[test]
    fn no_persistent_keys_returns_none() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "status"

[[classes.keys]]
id = "temp"
type = "uint16"
default = 0
"#,
        );
        assert!(collect_persistence_storage(&model, 0).is_none());
    }

    #[test]
    fn mixed_persistent_types() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "cfg"

[[classes.keys]]
id = "enabled"
type = "bool"
default = true
persistent = true

[[classes.keys]]
id = "count"
type = "uint16"
default = 42
persistent = true

[[classes.keys]]
id = "name"
type = "string"
max_size = 32
default = "hello"
persistent = true

[[classes.keys]]
id = "not_persistent"
type = "uint8"
default = 0
"#,
        );
        let ps = collect_persistence_storage(&model, 0).unwrap();
        assert_eq!(ps.num_keys, 3);

        assert_eq!(ps.entries[0].c_type, "bool");
        assert!(ps.entries[0].is_bool);
        assert_eq!(ps.entries[0].default_literal, "true");

        assert_eq!(ps.entries[1].c_type, "uint16_t");
        assert_eq!(ps.entries[1].default_literal, "42");

        assert!(ps.entries[2].is_string);
        assert_eq!(ps.entries[2].c_type, "char");
        assert_eq!(ps.entries[2].max_size, 32);
        assert_eq!(ps.entries[2].default_literal, "\"hello\"");
    }

    #[test]
    fn battery_has_two_persistent_keys() {
        let model = parse_model(include_str!("../../../examples/battery.toml"));
        let ps = collect_persistence_storage(&model, 0).unwrap();
        assert_eq!(ps.num_keys, 2);

        assert_eq!(ps.entries[0].field_name, "status_soc_in_use_threshold");
        assert_eq!(ps.entries[0].c_type, "uint8_t");
        assert_eq!(ps.entries[0].default_literal, "7");

        assert_eq!(ps.entries[1].field_name, "status_shutdown_delay_sec");
        assert_eq!(ps.entries[1].c_type, "uint8_t");
        assert_eq!(ps.entries[1].default_literal, "0");
    }

    #[test]
    fn string_escape_in_default() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "cfg"

[[classes.keys]]
id = "path"
type = "string"
max_size = 64
default = '/sdcard/"data".bin'
persistent = true
"#,
        );
        let ps = collect_persistence_storage(&model, 0).unwrap();
        assert_eq!(ps.entries[0].default_literal, r#""/sdcard/\"data\".bin""#);
    }
}
