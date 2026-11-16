use serde::Deserialize;
use std::collections::BTreeMap;

/// Serde default for boolean fields that default to true (`helpers`).
///
/// @version 1
/// @req REQ-ING-MOD-001
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

impl DataModel {
    /// Parse one namespace file and stamp every class and key without an explicit index with
    /// its declaration position, so encoded IDs survive key-list filtering. This is the only
    /// place positions are assigned.
    ///
    /// @version 1
    /// @req REQ-ING-KEY-002
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        let mut model: Self = toml::from_str(text)?;
        for (i, class) in model.classes.iter_mut().enumerate() {
            class.class_index.get_or_insert(i as u8);
            for (k, key) in class.keys.iter_mut().enumerate() {
                key.key_index.get_or_insert(k as u16);
            }
        }
        Ok(model)
    }
}

/// Message of the stamped-position invariant (DataModel::from_toml).
pub const STAMPED: &str = "class and key positions are stamped by DataModel::from_toml";

/// Model metadata.
#[derive(Debug, Deserialize)]
pub struct Meta {
    pub id: String,
    pub version: String,
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
    /// Optional original-case display name (e.g. "deviceConfig").
    ///
    /// A DM catalog pairs a lowercase `id` (which drives key encoding and the
    /// uppercase C surface) with a mixed-case display name. `--dm-compat`
    /// uses this spelling for the class segment of the lowercase
    /// `DataModel_{Get,Set}<path>` aliases. Falls back to `id` when absent.
    pub name: Option<String>,
    /// Optional 5-bit class index (0–31) for key encoding.
    /// When set, occupies bits 21–17 of every key in this class.
    pub class_index: Option<u8>,
    #[serde(default)]
    pub keys: Vec<KeyDef>,

    /// Namespace name for this class (set during multi-file merge).
    #[serde(skip)]
    pub namespace_name: Option<String>,
    /// Namespace ID for this class (set during multi-file merge).
    #[serde(skip)]
    pub namespace_id: Option<u16>,
}

/// Definition of a single key-value pair.
#[derive(Debug, Deserialize)]
pub struct KeyDef {
    pub id: String,
    /// Optional original-case display name (e.g. "versionNumber").
    ///
    /// Parallels `Class::name`: `--dm-compat` uses this spelling for the key
    /// segment of the lowercase `DataModel_{Get,Set}<path>` aliases. Falls
    /// back to `id` when absent.
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub data_type: DataType,
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
    ///
    /// Ordering matches gen_dm_code: unsigned widths first, then signed.
    /// @version 1
    /// @req REQ-ING-MOD-002
    pub fn type_code(self) -> u8 {
        match self {
            DataType::Bool => 0,
            DataType::Uint8 => 1,
            DataType::Uint16 => 2,
            DataType::Uint32 => 3,
            DataType::Int8 => 4,
            DataType::Int16 => 5,
            DataType::Int32 => 6,
            // 7 = float (reserved, not used)
            DataType::String => 8,
            DataType::Binary => 9,
        }
    }

    /// C type of a value as the generated API passes it: the scalar type, or a const
    /// pointer for string and binary.
    ///
    /// @version 1
    /// @req REQ-ING-MOD-002
    pub fn c_value_type(self) -> &'static str {
        match self {
            DataType::String => "const char *",
            DataType::Binary => "const uint8_t *",
            scalar => scalar.c_storage_type(),
        }
    }

    /// C type a value is stored as: the scalar type, or the element type of the
    /// fixed-size buffer for string (`char`) and binary (`uint8_t`).
    ///
    /// @version 1
    /// @req REQ-ING-MOD-002
    pub fn c_storage_type(self) -> &'static str {
        match self {
            DataType::Bool => "bool",
            DataType::Uint8 | DataType::Binary => "uint8_t",
            DataType::Int8 => "int8_t",
            DataType::Uint16 => "uint16_t",
            DataType::Int16 => "int16_t",
            DataType::Uint32 => "uint32_t",
            DataType::Int32 => "int32_t",
            DataType::String => "char",
        }
    }

    /// Field of the generated `dm_val_t` union that carries a scalar of this type;
    /// empty for string and binary, which are passed as buffers.
    ///
    /// @version 1
    /// @req REQ-ING-MOD-002
    pub fn val_field(self) -> &'static str {
        match self {
            DataType::Bool => "bval",
            DataType::Uint8 => "u8val",
            DataType::Int8 => "s8val",
            DataType::Uint16 => "u16val",
            DataType::Int16 => "s16val",
            DataType::Uint32 => "u32val",
            DataType::Int32 => "s32val",
            DataType::String | DataType::Binary => "",
        }
    }

    /// Whether values of this type are byte buffers (string and binary).
    ///
    /// @version 1
    /// @req REQ-ING-MOD-002
    pub fn is_buffer(self) -> bool {
        matches!(self, DataType::String | DataType::Binary)
    }

    /// Rust type name for code generation.
    ///
    /// `String`/`Binary` have no single scalar Rust type (they're generated
    /// as fixed-size `[u8; N]` byte arrays) — this returns a byte-slice
    /// placeholder for those two variants.
    /// @version 1
    /// @req REQ-ING-MOD-002
    pub fn rust_type(self) -> &'static str {
        match self {
            DataType::Bool => "bool",
            DataType::Uint8 => "u8",
            DataType::Int8 => "i8",
            DataType::Uint16 => "u16",
            DataType::Int16 => "i16",
            DataType::Uint32 => "u32",
            DataType::Int32 => "i32",
            DataType::String | DataType::Binary => "&[u8]",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_toml_stamps_positions_and_keeps_explicit_indices() {
        let model = DataModel::from_toml(
            "[meta]\nid = \"a\"\nversion = \"1\"\n\n[[classes]]\nid = \"x\"\n[[classes.keys]]\nid = \"p\"\ntype = \"bool\"\n[[classes.keys]]\nid = \"q\"\ntype = \"bool\"\nkey_index = 7\n\n[[classes]]\nid = \"y\"\nclass_index = 5\n[[classes]]\nid = \"z\"\n",
        )
        .unwrap();
        let classes: Vec<Option<u8>> = model.classes.iter().map(|c| c.class_index).collect();
        assert_eq!(classes, [Some(0), Some(5), Some(2)]);
        let keys: Vec<Option<u16>> = model.classes[0].keys.iter().map(|k| k.key_index).collect();
        assert_eq!(keys, [Some(0), Some(7)]);
    }

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
    fn data_type_rust_names() {
        assert_eq!(DataType::Uint8.rust_type(), "u8");
        assert_eq!(DataType::Int32.rust_type(), "i32");
        assert_eq!(DataType::Bool.rust_type(), "bool");
        assert_eq!(DataType::String.rust_type(), "&[u8]");
        assert_eq!(DataType::Binary.rust_type(), "&[u8]");
    }

    #[test]
    fn data_type_c_facts() {
        // (type, value type, storage type, dm_val_t field, buffer)
        let table = [
            (DataType::Bool, "bool", "bool", "bval", false),
            (DataType::Uint8, "uint8_t", "uint8_t", "u8val", false),
            (DataType::Int8, "int8_t", "int8_t", "s8val", false),
            (DataType::Uint16, "uint16_t", "uint16_t", "u16val", false),
            (DataType::Int16, "int16_t", "int16_t", "s16val", false),
            (DataType::Uint32, "uint32_t", "uint32_t", "u32val", false),
            (DataType::Int32, "int32_t", "int32_t", "s32val", false),
            (DataType::String, "const char *", "char", "", true),
            (DataType::Binary, "const uint8_t *", "uint8_t", "", true),
        ];
        for (t, value, storage, field, buffer) in table {
            assert_eq!(t.c_value_type(), value, "{t:?} value type");
            assert_eq!(t.c_storage_type(), storage, "{t:?} storage type");
            assert_eq!(t.val_field(), field, "{t:?} dm_val_t field");
            assert_eq!(t.is_buffer(), buffer, "{t:?} buffer");
        }
    }
}
