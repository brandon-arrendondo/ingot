use crate::codegen::{c_default_literal, key_stem};
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
///
/// v2: defaults come from the shared codegen::c_default_literal; an unused KeyEncoding (and the
/// namespace ID it needed) is no longer built.
/// v3: C storage type, dm_val_t field and buffer flag come from DataType.
/// v4: the define name comes from the shared codegen::key_stem.
/// @version 4
/// @req REQ-ING-GEN-007
pub fn collect_persistence_storage(model: &DataModel) -> Option<PersistenceStorage> {
    let mut entries = Vec::new();

    for class in &model.classes {
        for key in &class.keys {
            if !key.persistent {
                continue;
            }

            let define_name = format!("DM_KEY_{}", key_stem(class, model, key));
            let field_name = format!(
                "{}_{}",
                class.id.to_lowercase(),
                key.id.to_lowercase().replace(' ', "_")
            );

            let is_string = key.data_type.is_buffer();
            let max_size = if is_string {
                key.max_size.unwrap_or(0)
            } else {
                0
            };

            let default_literal = c_default_literal(&key.default, key.data_type);

            entries.push(PersistenceEntry {
                field_name,
                define_name,
                c_type: key.data_type.c_storage_type().to_string(),
                is_string,
                max_size,
                default_literal,
                val_field: key.data_type.val_field().to_string(),
                is_bool: key.data_type == DataType::Bool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::DataModel;

    fn parse_model(toml_str: &str) -> DataModel {
        DataModel::from_toml(toml_str).unwrap()
    }

    #[test]
    fn minimal_has_one_persistent_key() {
        let model = parse_model(include_str!("../../../examples/minimal.toml"));
        let ps = collect_persistence_storage(&model).unwrap();
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
        assert!(collect_persistence_storage(&model).is_none());
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
        let ps = collect_persistence_storage(&model).unwrap();
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
        let ps = collect_persistence_storage(&model).unwrap();
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
        let ps = collect_persistence_storage(&model).unwrap();
        assert_eq!(ps.entries[0].default_literal, r#""/sdcard/\"data\".bin""#);
    }
}
