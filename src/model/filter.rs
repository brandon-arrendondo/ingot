use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::schema::DataModel;

/// A parsed key list entry — either a define name or a hex key value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ListEntry {
    Name(String),
    HexValue(u32),
}

/// Parsed key list supporting both name and hex-value entries.
///
/// Names are stored in two forms: original (for exact matching) and
/// normalized (underscores removed, for fuzzy matching against TOML
/// snake_case IDs that differ from the original YAML naming).
#[derive(Debug, Default)]
pub struct KeyList {
    names: HashSet<String>,
    /// Normalized names (uppercased, underscores stripped) for fuzzy matching.
    normalized: HashSet<String>,
    values: HashSet<u32>,
}

impl KeyList {
    pub fn len(&self) -> usize {
        self.names.len() + self.values.len()
    }
}

/// Normalize a define name for fuzzy matching: uppercase + strip underscores.
fn normalize(s: &str) -> String {
    s.to_uppercase().replace('_', "")
}

/// Load a YAML list file (array of strings/ints) into a KeyList.
/// Entries can be define names (`BATTERY_STATUS_VOLTAGE`) or
/// hex key values (`0x05C00041` or integer literals).
pub fn load_key_list(path: &Path) -> Result<KeyList, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let items: Vec<serde_yaml::Value> = serde_yaml::from_str(&content)?;
    let mut list = KeyList::default();
    for item in items {
        match item {
            serde_yaml::Value::String(s) => {
                if let Some(hex) = parse_hex(&s) {
                    list.values.insert(hex);
                } else {
                    list.normalized.insert(normalize(&s));
                    list.names.insert(s);
                }
            }
            serde_yaml::Value::Number(n) => {
                if let Some(v) = n.as_u64() {
                    list.values.insert(v as u32);
                }
            }
            _ => {}
        }
    }
    Ok(list)
}

/// Try to parse a "0x..." hex string into a u32.
fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        None
    }
}

/// Construct the bare define name for a key: `{NAMESPACE}_{CLASS}_{KEY}`.
fn define_name(ns: &str, class: &str, key: &str) -> String {
    format!(
        "{}_{}_{}",
        ns.to_uppercase(),
        class.to_uppercase().replace(' ', "_"),
        key.to_uppercase().replace(' ', "_"),
    )
}

/// Returns true if a key matches the list by name, normalized name, or encoded value.
///
/// Normalized matching strips underscores so that TOML snake_case IDs
/// (e.g. `STATE_OF_CHARGE`) match YAML-era names (`STATEOFCHARGE`).
fn matches(list: &KeyList, name: &str, encoded: Option<u32>) -> bool {
    list.names.contains(name)
        || list.names.contains(&format!("DM_KEY_{name}"))
        || list.normalized.contains(&normalize(name))
        || encoded.is_some_and(|v| list.values.contains(&v))
}

/// Filter the model to only include keys present in the include list.
pub fn apply_include_list(model: &mut DataModel, include: &KeyList) {
    let fallback_ns = model.meta.id.clone();
    for class in &mut model.classes {
        let ns = class.namespace_name.as_deref().unwrap_or(&fallback_ns);
        class.keys.retain(|key| {
            let dn = define_name(ns, &class.id, &key.id);
            matches(include, &dn, None)
        });
    }
}

/// Remove keys present in the exclude list from the model.
pub fn apply_exclude_list(model: &mut DataModel, exclude: &KeyList) {
    let fallback_ns = model.meta.id.clone();
    for class in &mut model.classes {
        let ns = class.namespace_name.as_deref().unwrap_or(&fallback_ns);
        class.keys.retain(|key| {
            let dn = define_name(ns, &class.id, &key.id);
            !matches(exclude, &dn, None)
        });
    }
}

/// Mark keys present in the persistent list as persistent.
pub fn apply_persistent_keys(model: &mut DataModel, persistent: &KeyList) {
    let fallback_ns = model.meta.id.clone();
    for class in &mut model.classes {
        let ns = class.namespace_name.as_deref().unwrap_or(&fallback_ns);
        for key in &mut class.keys {
            let dn = define_name(ns, &class.id, &key.id);
            if matches(persistent, &dn, None) {
                key.persistent = true;
            }
        }
    }
}

/// Per-key property overrides keyed by normalized define name.
#[derive(Debug, Default)]
pub struct PropertyOverrides {
    /// Map from normalized define name → default value override.
    overrides: HashMap<String, toml::Value>,
}

impl PropertyOverrides {
    pub fn len(&self) -> usize {
        self.overrides.len()
    }
}

/// Load a property override YAML file.
///
/// Expected format:
/// ```yaml
/// KEY_DEFINE_NAME:
///   default_value: <value>
/// ```
pub fn load_property_overrides(
    path: &Path,
) -> Result<PropertyOverrides, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let map: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&content)?;
    let mut overrides = PropertyOverrides::default();

    for (key_name, props) in map {
        if let Some(default_val) = props.get("default_value") {
            let toml_val = yaml_value_to_toml(default_val);
            overrides.overrides.insert(normalize(&key_name), toml_val);
        }
    }

    Ok(overrides)
}

/// Convert a serde_yaml::Value to a toml::Value for use as key default.
fn yaml_value_to_toml(v: &serde_yaml::Value) -> toml::Value {
    match v {
        serde_yaml::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::Integer(0)
            }
        }
        serde_yaml::Value::String(s) => toml::Value::String(s.clone()),
        _ => toml::Value::String(format!("{v:?}")),
    }
}

/// Apply property overrides to the data model.
///
/// For each key whose normalized define name matches an override entry,
/// replace the key's default value with the override value.
/// Returns the number of overrides applied.
pub fn apply_property_overrides(model: &mut DataModel, overrides: &PropertyOverrides) -> usize {
    let fallback_ns = model.meta.id.clone();
    let mut count = 0;
    for class in &mut model.classes {
        let ns = class.namespace_name.as_deref().unwrap_or(&fallback_ns);
        for key in &mut class.keys {
            let dn = define_name(ns, &class.id, &key.id);
            let norm = normalize(&dn);
            if let Some(val) = overrides.overrides.get(&norm) {
                key.default = Some(val.clone());
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_model(toml_str: &str) -> DataModel {
        toml::from_str(toml_str).unwrap()
    }

    fn test_model() -> DataModel {
        parse_model(
            r#"
[meta]
id = "battery"
version = "1.0.0"

[[classes]]
id = "status"

[[classes.keys]]
id = "voltage"
type = "uint16"

[[classes.keys]]
id = "current"
type = "int16"

[[classes.keys]]
id = "level"
type = "uint8"
"#,
        )
    }

    fn key_list_from_names(names: &[&str]) -> KeyList {
        KeyList {
            normalized: names.iter().map(|s| normalize(s)).collect(),
            names: names.iter().map(|s| s.to_string()).collect(),
            values: HashSet::new(),
        }
    }

    fn key_list_from_values(vals: &[u32]) -> KeyList {
        KeyList {
            names: HashSet::new(),
            normalized: HashSet::new(),
            values: vals.iter().copied().collect(),
        }
    }

    #[test]
    fn include_list_keeps_only_matching() {
        let mut model = test_model();
        let include = key_list_from_names(&["BATTERY_STATUS_VOLTAGE"]);
        apply_include_list(&mut model, &include);
        assert_eq!(model.classes[0].keys.len(), 1);
        assert_eq!(model.classes[0].keys[0].id, "voltage");
    }

    #[test]
    fn include_list_with_dm_key_prefix() {
        let mut model = test_model();
        let include = key_list_from_names(&["DM_KEY_BATTERY_STATUS_CURRENT"]);
        apply_include_list(&mut model, &include);
        assert_eq!(model.classes[0].keys.len(), 1);
        assert_eq!(model.classes[0].keys[0].id, "current");
    }

    #[test]
    fn exclude_list_removes_matching() {
        let mut model = test_model();
        let exclude = key_list_from_names(&["BATTERY_STATUS_LEVEL"]);
        apply_exclude_list(&mut model, &exclude);
        assert_eq!(model.classes[0].keys.len(), 2);
        assert!(model.classes[0].keys.iter().all(|k| k.id != "level"));
    }

    #[test]
    fn persistent_keys_sets_flag() {
        let mut model = test_model();
        assert!(!model.classes[0].keys[0].persistent);
        let persistent = key_list_from_names(&["BATTERY_STATUS_VOLTAGE"]);
        apply_persistent_keys(&mut model, &persistent);
        assert!(model.classes[0].keys[0].persistent);
        assert!(!model.classes[0].keys[1].persistent);
    }

    #[test]
    fn parse_hex_values() {
        assert_eq!(parse_hex("0x05C00041"), Some(0x05C00041));
        assert_eq!(parse_hex("0X00040388"), Some(0x00040388));
        assert_eq!(parse_hex("NOT_HEX"), None);
    }

    #[test]
    fn property_overrides_apply() {
        let mut model = test_model();
        // voltage default is None initially
        assert!(model.classes[0].keys[0].default.is_none());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("overrides.yaml");
        std::fs::write(
            &path,
            "---\nBATTERY_STATUS_VOLTAGE:\n  default_value: 3700\nBATTERY_STATUS_LEVEL:\n  default_value: 5\n",
        )
        .unwrap();
        let overrides = load_property_overrides(&path).unwrap();
        assert_eq!(overrides.len(), 2);

        let applied = apply_property_overrides(&mut model, &overrides);
        assert_eq!(applied, 2);
        assert_eq!(
            model.classes[0].keys[0].default,
            Some(toml::Value::Integer(3700))
        );
        assert_eq!(
            model.classes[0].keys[2].default,
            Some(toml::Value::Integer(5))
        );
    }

    #[test]
    fn property_overrides_normalized_matching() {
        let mut model = test_model();
        // Override uses YAML-era name without underscores
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("overrides.yaml");
        std::fs::write(&path, "---\nBATTERYSTATUSVOLTAGE:\n  default_value: 4200\n").unwrap();
        let overrides = load_property_overrides(&path).unwrap();
        let applied = apply_property_overrides(&mut model, &overrides);
        assert_eq!(applied, 1);
        assert_eq!(
            model.classes[0].keys[0].default,
            Some(toml::Value::Integer(4200))
        );
    }

    #[test]
    fn property_overrides_string_and_bool() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("overrides.yaml");
        std::fs::write(
            &path,
            "---\nSOME_KEY:\n  default_value: hello\nOTHER_KEY:\n  default_value: true\n",
        )
        .unwrap();
        let overrides = load_property_overrides(&path).unwrap();
        assert_eq!(overrides.len(), 2);
        // Verify parsed types
        assert_eq!(
            overrides.overrides.get(&normalize("SOME_KEY")),
            Some(&toml::Value::String("hello".into()))
        );
        assert_eq!(
            overrides.overrides.get(&normalize("OTHER_KEY")),
            Some(&toml::Value::Boolean(true))
        );
    }

    #[test]
    fn load_mixed_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.yaml");
        std::fs::write(
            &path,
            "---\n- BATTERY_STATUS_VOLTAGE\n- 0x05C00041\n- 12345\n",
        )
        .unwrap();
        let list = load_key_list(&path).unwrap();
        assert_eq!(list.names.len(), 1);
        assert!(list.names.contains("BATTERY_STATUS_VOLTAGE"));
        assert_eq!(list.values.len(), 2);
        assert!(list.values.contains(&0x05C00041));
        assert!(list.values.contains(&12345));
    }
}
