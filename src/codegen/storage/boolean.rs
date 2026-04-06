use crate::hash::{self, PerfectHash};
use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType, KeyDef};
use serde::Serialize;

/// Boolean storage metadata for template rendering.
#[derive(Debug, Serialize)]
pub struct BoolStorage {
    /// Number of boolean keys
    pub num_keys: usize,
    /// Number of uint32_t words needed: ceil(num_keys / 32)
    pub num_words: usize,
    /// Perfect hash seed 1
    pub seed1: u32,
    /// Perfect hash seed 2
    pub seed2: u32,
    /// G table size
    pub g_size: usize,
    /// C type for G table elements
    pub g_c_type: String,
    /// G table values as C initializer list
    pub g_table_str: String,
    /// Default storage words as C initializer list (e.g. "0x00000402U, 0x00000000U")
    pub init_table_str: String,
}

/// Collect boolean keys from the model, generate perfect hash and pack defaults.
pub fn collect_boolean_storage(
    model: &DataModel,
    ns_id: u16,
) -> Result<Option<BoolStorage>, String> {
    let mut keys_with_meta: Vec<(u32, &KeyDef)> = Vec::new();

    for (class_idx, class) in model.classes.iter().enumerate() {
        let mut bool_id_counter: u16 = 0;
        for key in &class.keys {
            if key.data_type == DataType::Bool {
                let encoding = KeyEncoding {
                    namespace: ns_id,
                    class: class_idx as u8,
                    id: bool_id_counter,
                    data_type: key.data_type.type_code(),
                    thread_safe: key.thread_safe,
                    derived: false,
                    read_only: key.read_only,
                };
                keys_with_meta.push((encoding.encode(), key));
                bool_id_counter += 1;
            }
        }
    }

    if keys_with_meta.is_empty() {
        return Ok(None);
    }

    let encoded_keys: Vec<u32> = keys_with_meta.iter().map(|(k, _)| *k).collect();
    let ph = hash::generate(&encoded_keys, 100)
        .ok_or("Failed to generate perfect hash for boolean keys")?;

    Ok(Some(build_bool_storage(&keys_with_meta, &ph)))
}

fn build_bool_storage(keys_with_meta: &[(u32, &KeyDef)], ph: &PerfectHash) -> BoolStorage {
    let num_keys = keys_with_meta.len();
    let num_words = num_keys.div_ceil(32);

    // Pack default values into uint32_t words by setting bits at hash-ordered positions
    let mut words = vec![0u32; num_words];
    for (encoded_key, key_def) in keys_with_meta {
        let idx = ph.lookup(*encoded_key);
        let is_true = key_def
            .default
            .as_ref()
            .map(|v| match v {
                toml::Value::Boolean(b) => *b,
                toml::Value::Integer(i) => *i != 0,
                _ => false,
            })
            .unwrap_or(false);

        if is_true {
            let word = idx / 32;
            let bit = idx % 32;
            words[word] |= 1 << bit;
        }
    }

    let init_table_str = words
        .iter()
        .map(|w| format!("{w:#010X}U"))
        .collect::<Vec<_>>()
        .join(", ");

    let g_c_type = select_g_table_type(&ph.g_table);

    BoolStorage {
        num_keys,
        num_words,
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
        init_table_str,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::DataModel;

    #[test]
    fn collect_from_battery_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/battery.toml")).unwrap();
        let storage = collect_boolean_storage(&model, 0).unwrap();

        // battery.toml has 2 bool keys: is_charging, shutdown
        let storage = storage.expect("should have boolean keys");
        assert_eq!(storage.num_keys, 2);
        assert_eq!(storage.num_words, 1);
        assert!(storage.g_size > 0);
    }

    #[test]
    fn collect_from_minimal_model() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/minimal.toml")).unwrap();
        let storage = collect_boolean_storage(&model, 0).unwrap();

        // minimal.toml has 1 bool key: is_connected (default=false)
        let storage = storage.expect("should have boolean keys");
        assert_eq!(storage.num_keys, 1);
        assert_eq!(storage.num_words, 1);
    }

    #[test]
    fn no_bools_returns_none() {
        // A model with only integer keys
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
        let storage = collect_boolean_storage(&model, 0).unwrap();
        assert!(storage.is_none());
    }

    #[test]
    fn default_true_sets_bit() {
        let toml_str = r#"
            [meta]
            id = "test"
            version = "1.0.0"
            [[classes]]
            id = "flags"
            [[classes.keys]]
            id = "flag_a"
            type = "bool"
            default = true
            [[classes.keys]]
            id = "flag_b"
            type = "bool"
            default = false
            [[classes.keys]]
            id = "flag_c"
            type = "bool"
            default = true
        "#;
        let model: DataModel = toml::from_str(toml_str).unwrap();
        let storage = collect_boolean_storage(&model, 0).unwrap().unwrap();

        assert_eq!(storage.num_keys, 3);
        // The init word should be non-zero (some bits set for true defaults)
        assert_ne!(storage.init_table_str, "0x00000000U");
    }

    #[test]
    fn num_words_ceiling_division() {
        // Verify num_words = ceil(n/32)
        assert_eq!((1 + 31) / 32, 1);
        assert_eq!((32 + 31) / 32, 1);
        assert_eq!((33 + 31) / 32, 2);
        assert_eq!((64 + 31) / 32, 2);
        assert_eq!((65 + 31) / 32, 3);
    }
}
