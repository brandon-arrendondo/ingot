use serde::Deserialize;
use std::collections::BTreeMap;

fn default_true() -> bool {
    true
}

/// Top-level data model specification (one file per namespace).
///
/// ```toml
/// [meta]
/// id = "battery"
/// version = "0.0.1"
///
/// [enums.level]
/// doc = "Battery level"
/// [enums.level.values]
/// unknown = 0
/// critical = 1
///
/// [[classes]]
/// id = "status"
/// [[classes.keys]]
/// id = "voltage"
/// type = "uint16"
/// default = 0
/// ```
#[derive(Debug, Deserialize)]
pub struct DataModel {
    pub meta: Meta,
    #[serde(default)]
    pub enums: BTreeMap<String, EnumDef>,
    #[serde(default)]
    pub classes: Vec<Class>,
}

/// Model metadata.
#[derive(Debug, Deserialize)]
pub struct Meta {
    pub id: String,
    pub version: String,
    pub doc: Option<String>,
    /// Optional 10-bit namespace identifier (0–1023) for key encoding.
    /// When set, this value occupies bits 31–22 of every generated key.
    /// Namespaces without an explicit ID get one assigned at build time.
    pub namespace_id: Option<u16>,
}

/// A named enum definition with integer values.
#[derive(Debug, Deserialize)]
pub struct EnumDef {
    pub doc: Option<String>,
    /// Default value set: name -> integer.
    pub values: BTreeMap<String, i64>,
    /// Per-variant overrides: variant_name -> (name -> integer).
    #[serde(default)]
    pub variants: BTreeMap<String, BTreeMap<String, i64>>,
}

/// A class groups related keys within a namespace.
#[derive(Debug, Deserialize)]
pub struct Class {
    pub id: String,
    pub doc: Option<String>,
    /// Optional 5-bit class index (0–31) for key encoding.
    /// When set, occupies bits 21–17 of every key in this class.
    pub class_index: Option<u8>,
    #[serde(default)]
    pub keys: Vec<KeyDef>,
}

/// Definition of a single key-value pair.
#[derive(Debug, Deserialize)]
pub struct KeyDef {
    pub id: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
    pub doc: Option<String>,
    pub unit: Option<String>,

    /// Optional 10-bit key index (0–1023) for key encoding.
    /// When set, occupies bits 16–7 of this key's encoded value.
    pub key_index: Option<u16>,

    /// Simple default value (used when all variants share the same default).
    pub default: Option<toml::Value>,
    /// Per-variant default overrides (variant_name -> value).
    /// When present, `default` is the base default and entries here override
    /// it for specific product variants.
    #[serde(default)]
    pub defaults: BTreeMap<String, toml::Value>,

    /// Name of an enum defined in the `[enums]` section.
    #[serde(rename = "enum")]
    pub enum_ref: Option<String>,

    /// Maximum size in bytes (required for string and binary types).
    pub max_size: Option<usize>,

    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub thread_safe: bool,
    #[serde(default)]
    pub persistent: bool,
    #[serde(default)]
    pub event: bool,
    #[serde(default = "default_true")]
    pub helpers: bool,
}

/// Supported data types for storage.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Bool,
    Uint8,
    Int8,
    Uint16,
    Int16,
    Uint32,
    Int32,
    String,
    Binary,
}

impl DataType {
    /// Type code used in the 4-bit key encoding field.
    pub fn type_code(self) -> u8 {
        match self {
            DataType::Bool => 0,
            DataType::Uint8 => 1,
            DataType::Int8 => 2,
            DataType::Uint16 => 3,
            DataType::Int16 => 4,
            DataType::Uint32 => 5,
            DataType::Int32 => 6,
            DataType::String => 7,
            DataType::Binary => 8,
        }
    }

    /// C type name for code generation.
    pub fn c_type(self) -> &'static str {
        match self {
            DataType::Bool => "bool",
            DataType::Uint8 => "uint8_t",
            DataType::Int8 => "int8_t",
            DataType::Uint16 => "uint16_t",
            DataType::Int16 => "int16_t",
            DataType::Uint32 => "uint32_t",
            DataType::Int32 => "int32_t",
            DataType::String => "char *",
            DataType::Binary => "uint8_t *",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_example() {
        let toml_str = include_str!("../../examples/minimal.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();

        assert_eq!(model.meta.id, "example");
        assert_eq!(model.meta.version, "1.0.0");
        assert_eq!(model.enums.len(), 1);
        assert!(model.enums.contains_key("device_mode"));
        assert_eq!(model.enums["device_mode"].values["off"], 0);
        assert_eq!(model.enums["device_mode"].values["active"], 2);
        assert_eq!(model.classes.len(), 2);

        let status = &model.classes[0];
        assert_eq!(status.id, "status");
        assert_eq!(status.keys.len(), 3);

        let temp = &status.keys[0];
        assert_eq!(temp.id, "temperature");
        assert_eq!(temp.data_type, DataType::Uint16);
        assert_eq!(temp.unit.as_deref(), Some("0.1K"));
        assert!(temp.thread_safe);
        assert!(temp.helpers);
        assert!(!temp.event);

        let mode = &status.keys[1];
        assert_eq!(mode.enum_ref.as_deref(), Some("device_mode"));
        assert!(mode.event);

        let config = &model.classes[1];
        assert_eq!(config.id, "config");
        let name_key = &config.keys[0];
        assert_eq!(name_key.data_type, DataType::String);
        assert_eq!(name_key.max_size, Some(32));
    }

    #[test]
    fn parse_battery_example() {
        let toml_str = include_str!("../../examples/battery.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();

        assert_eq!(model.meta.id, "battery");

        // Enum with variants
        let state_enum = &model.enums["state"];
        assert_eq!(state_enum.values["disable_charging"], 0);
        assert_eq!(state_enum.values["enable_charging"], 1);
        assert!(state_enum.variants.contains_key("ascent"));
        assert_eq!(state_enum.variants["ascent"]["error"], 254);
        assert_eq!(state_enum.variants["ascent"]["initial"], 255);

        // Classes
        assert_eq!(model.classes.len(), 2);
        let status = &model.classes[1];
        assert_eq!(status.id, "status");

        // Key with per-variant defaults
        let soc = &status.keys[0];
        assert_eq!(soc.id, "state_of_charge");
        assert!(!soc.defaults.is_empty());
    }

    #[test]
    fn data_type_codes_unique() {
        let types = [
            DataType::Bool,
            DataType::Uint8,
            DataType::Int8,
            DataType::Uint16,
            DataType::Int16,
            DataType::Uint32,
            DataType::Int32,
            DataType::String,
            DataType::Binary,
        ];
        let codes: Vec<u8> = types.iter().map(|t| t.type_code()).collect();
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j], "type codes must be unique");
            }
        }
        // All must fit in 4 bits
        assert!(codes.iter().all(|&c| c < 16));
    }

    #[test]
    fn data_type_c_names() {
        assert_eq!(DataType::Uint8.c_type(), "uint8_t");
        assert_eq!(DataType::Bool.c_type(), "bool");
        assert_eq!(DataType::String.c_type(), "char *");
    }
}
