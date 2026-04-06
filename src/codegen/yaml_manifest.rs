use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Top-level dm_full.yaml manifest consumed by downstream tools.
#[derive(Debug, Serialize)]
struct DmFullManifest {
    metadata: Metadata,
    namespaces: Vec<NamespaceEntry>,
}

#[derive(Debug, Serialize)]
struct Metadata {
    ingot_version: String,
}

#[derive(Debug, Serialize)]
struct NamespaceEntry {
    classes: Vec<ClassEntry>,
    name: String,
}

#[derive(Debug, Serialize)]
struct ClassEntry {
    data: Vec<KeyEntry>,
    name: String,
}

#[derive(Debug, Serialize)]
struct KeyEntry {
    define_name: String,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    enum_values: Option<BTreeMap<i64, String>>,
    generate_helpers: bool,
    id: u32,
    max_size: usize,
    name: String,
    #[serde(rename = "type")]
    key_type: KeyTypeEntry,
}

#[derive(Debug, Serialize)]
struct KeyTypeEntry {
    default_value: serde_yaml::Value,
    mem: String,
}

/// Generate the dm_full.yaml manifest for downstream tool consumption.
pub fn generate_yaml_manifest(
    model: &DataModel,
    ns_id: u16,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let ns_name = model.meta.id.to_uppercase();
    let mut classes = Vec::new();

    for (class_idx, class) in model.classes.iter().enumerate() {
        let class_name = class.id.to_uppercase();
        let mut type_counters: [u16; 16] = [0; 16];
        let mut data = Vec::new();

        for key in &class.keys {
            let type_code = key.data_type.type_code();
            let per_type_id = type_counters[type_code as usize];
            type_counters[type_code as usize] += 1;

            let encoding = KeyEncoding {
                namespace: ns_id,
                class: class_idx as u8,
                id: per_type_id,
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };
            let encoded = encoding.encode();

            let define_name = format!(
                "{}_{}_{}",
                ns_name,
                class_name,
                key.id.to_uppercase().replace(' ', "_")
            );

            // Inline enum values: DEFINE_NAME_VALUENAME
            let enum_values = key.enum_ref.as_ref().and_then(|enum_name| {
                model.enums.get(enum_name).map(|enum_def| {
                    enum_def
                        .values
                        .iter()
                        .map(|(name, &val)| {
                            let full_name = format!("{}_{}", define_name, name.to_uppercase());
                            (val, full_name)
                        })
                        .collect::<BTreeMap<i64, String>>()
                })
            });

            let default_value = convert_default(&key.default, key.data_type);
            let max_size = key.max_size.unwrap_or(0);

            data.push(KeyEntry {
                define_name,
                enum_values,
                generate_helpers: key.helpers,
                id: encoded,
                max_size,
                name: key.id.clone(),
                key_type: KeyTypeEntry {
                    default_value,
                    mem: format!("{:?}", key.data_type).to_lowercase(),
                },
            });
        }

        classes.push(ClassEntry {
            data,
            name: class.id.clone(),
        });
    }

    let manifest = DmFullManifest {
        metadata: Metadata {
            ingot_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        namespaces: vec![NamespaceEntry {
            classes,
            name: model.meta.id.clone(),
        }],
    };

    let yaml = serde_yaml::to_string(&manifest)?;
    std::fs::write(output_dir.join("dm_full.yaml"), yaml)?;
    log::info!("Generated dm_full.yaml");

    Ok(())
}

fn convert_default(default: &Option<toml::Value>, data_type: DataType) -> serde_yaml::Value {
    match default {
        Some(toml::Value::Boolean(b)) => serde_yaml::Value::Bool(*b),
        Some(toml::Value::Integer(i)) => serde_yaml::Value::Number(serde_yaml::Number::from(*i)),
        Some(toml::Value::String(s)) => serde_yaml::Value::String(s.clone()),
        Some(toml::Value::Float(f)) => {
            serde_yaml::Value::Number(serde_yaml::Number::from(*f as i64))
        }
        _ => match data_type {
            DataType::Bool => serde_yaml::Value::Bool(false),
            DataType::String | DataType::Binary => serde_yaml::Value::String(String::new()),
            _ => serde_yaml::Value::Number(serde_yaml::Number::from(0)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::DataModel;

    #[test]
    fn generate_manifest_for_battery() {
        let toml_str = include_str!("../../examples/battery.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();

        let dir = tempfile::tempdir().unwrap();
        generate_yaml_manifest(&model, 0, dir.path()).unwrap();

        let yaml = std::fs::read_to_string(dir.path().join("dm_full.yaml")).unwrap();
        assert!(yaml.contains("name: battery"));
        assert!(yaml.contains("name: namespace"));
        assert!(yaml.contains("name: status"));
        assert!(yaml.contains("define_name: BATTERY_STATUS_VOLTAGE"));
        assert!(yaml.contains("mem: uint16"));
        // Enum values should be inlined
        assert!(yaml.contains("BATTERY_STATUS_LEVEL_UNKNOWN"));
        assert!(yaml.contains("BATTERY_STATUS_LEVEL_FULL"));
        assert!(yaml.contains("BATTERY_STATUS_STATE_ENABLE_CHARGING"));
    }

    #[test]
    fn generate_manifest_for_minimal() {
        let toml_str = include_str!("../../examples/minimal.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();

        let dir = tempfile::tempdir().unwrap();
        generate_yaml_manifest(&model, 0, dir.path()).unwrap();

        let yaml = std::fs::read_to_string(dir.path().join("dm_full.yaml")).unwrap();
        assert!(yaml.contains("name: example"));
        assert!(yaml.contains("define_name: EXAMPLE_STATUS_TEMPERATURE"));
        assert!(yaml.contains("mem: uint16"));
        assert!(yaml.contains("mem: string"));
        // device_mode enum should be inlined for the mode key
        assert!(yaml.contains("EXAMPLE_STATUS_MODE_OFF"));
        assert!(yaml.contains("EXAMPLE_STATUS_MODE_ACTIVE"));
    }

    #[test]
    fn manifest_key_ids_are_encoded() {
        let toml_str = include_str!("../../examples/minimal.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();

        let dir = tempfile::tempdir().unwrap();
        generate_yaml_manifest(&model, 0, dir.path()).unwrap();

        let yaml = std::fs::read_to_string(dir.path().join("dm_full.yaml")).unwrap();
        // Key IDs should be non-zero encoded 32-bit values
        assert!(yaml.contains("id: "));
        // Parse the YAML back and check IDs are non-zero
        let value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let namespaces = value["namespaces"].as_sequence().unwrap();
        let classes = namespaces[0]["classes"].as_sequence().unwrap();
        let data = classes[0]["data"].as_sequence().unwrap();
        for key_entry in data {
            let id = key_entry["id"].as_u64().unwrap();
            assert!(id > 0, "encoded key ID should be non-zero");
        }
    }

    #[test]
    fn default_values_match_types() {
        assert_eq!(
            convert_default(&Some(toml::Value::Boolean(true)), DataType::Bool),
            serde_yaml::Value::Bool(true)
        );
        assert_eq!(
            convert_default(&Some(toml::Value::Integer(42)), DataType::Uint16),
            serde_yaml::Value::Number(serde_yaml::Number::from(42))
        );
        assert_eq!(
            convert_default(&Some(toml::Value::String("hello".into())), DataType::String),
            serde_yaml::Value::String("hello".into())
        );
        // No default → type-appropriate zero
        assert_eq!(
            convert_default(&None, DataType::Bool),
            serde_yaml::Value::Bool(false)
        );
        assert_eq!(
            convert_default(&None, DataType::Uint32),
            serde_yaml::Value::Number(serde_yaml::Number::from(0))
        );
        assert_eq!(
            convert_default(&None, DataType::String),
            serde_yaml::Value::String(String::new())
        );
    }
}
