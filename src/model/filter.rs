use std::collections::HashSet;
use std::path::Path;

use super::schema::DataModel;

/// A parsed key list entry — either a define name or a hex key value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ListEntry {
    Name(String),
    HexValue(u32),
}

/// Parsed key list supporting both name and hex-value entries.
#[derive(Debug, Default)]
pub struct KeyList {
    names: HashSet<String>,
    values: HashSet<u32>,
}

impl KeyList {
    pub fn len(&self) -> usize {
        self.names.len() + self.values.len()
    }
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

/// Returns true if a key matches the list by name or encoded value.
fn matches(list: &KeyList, name: &str, encoded: Option<u32>) -> bool {
    list.names.contains(name)
        || list.names.contains(&format!("DM_KEY_{name}"))
        || encoded.is_some_and(|v| list.values.contains(&v))
}

/// Filter the model to only include keys present in the include list.
pub fn apply_include_list(model: &mut DataModel, include: &KeyList) {
    let ns = model.meta.id.clone();
    for class in &mut model.classes {
        class.keys.retain(|key| {
            let dn = define_name(&ns, &class.id, &key.id);
            matches(include, &dn, None)
        });
    }
}

/// Remove keys present in the exclude list from the model.
pub fn apply_exclude_list(model: &mut DataModel, exclude: &KeyList) {
    let ns = model.meta.id.clone();
    for class in &mut model.classes {
        class.keys.retain(|key| {
            let dn = define_name(&ns, &class.id, &key.id);
            !matches(exclude, &dn, None)
        });
    }
}

/// Mark keys present in the persistent list as persistent.
pub fn apply_persistent_keys(model: &mut DataModel, persistent: &KeyList) {
    let ns = model.meta.id.clone();
    for class in &mut model.classes {
        for key in &mut class.keys {
            let dn = define_name(&ns, &class.id, &key.id);
            if matches(persistent, &dn, None) {
                key.persistent = true;
            }
        }
    }
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
            names: names.iter().map(|s| s.to_string()).collect(),
            values: HashSet::new(),
        }
    }

    fn key_list_from_values(vals: &[u32]) -> KeyList {
        KeyList {
            names: HashSet::new(),
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
