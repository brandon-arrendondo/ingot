use super::schema::{DataModel, DataType};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("class count ({count}) exceeds maximum (31) in namespace '{namespace}'")]
    TooManyClasses { namespace: String, count: usize },

    #[error(
        "key count ({count}) in class '{class}' exceeds maximum (1023) in namespace '{namespace}'"
    )]
    TooManyKeys {
        namespace: String,
        class: String,
        count: usize,
    },

    #[error("duplicate class id '{id}' in namespace '{namespace}'")]
    DuplicateClassId { namespace: String, id: String },

    #[error("duplicate key id '{key}' in {namespace}.{class}")]
    DuplicateKeyId {
        namespace: String,
        class: String,
        key: String,
    },

    #[error("string key '{key}' in {namespace}.{class} missing max_size")]
    StringMissingMaxSize {
        namespace: String,
        class: String,
        key: String,
    },

    #[error("binary key '{key}' in {namespace}.{class} missing max_size")]
    BinaryMissingMaxSize {
        namespace: String,
        class: String,
        key: String,
    },

    #[error("key '{key}' in {namespace}.{class} references undefined enum '{enum_name}'")]
    UndefinedEnum {
        namespace: String,
        class: String,
        key: String,
        enum_name: String,
    },

    #[error("key '{key}' in {namespace}.{class} is read-only and cannot be persistent")]
    ReadOnlyPersistent {
        namespace: String,
        class: String,
        key: String,
    },

    #[error("namespace_id {id} in '{namespace}' exceeds maximum (1023)")]
    NamespaceIdOutOfRange { namespace: String, id: u16 },

    #[error("class_index {index} for class '{class}' in '{namespace}' exceeds maximum (31)")]
    ClassIndexOutOfRange {
        namespace: String,
        class: String,
        index: u8,
    },

    #[error("duplicate class_index {index} in namespace '{namespace}'")]
    DuplicateClassIndex { namespace: String, index: u8 },

    #[error("key_index {index} for key '{key}' in {namespace}.{class} exceeds maximum (1023)")]
    KeyIndexOutOfRange {
        namespace: String,
        class: String,
        key: String,
        index: u16,
    },

    #[error("duplicate key_index {index} in {namespace}.{class}")]
    DuplicateKeyIndex {
        namespace: String,
        class: String,
        index: u16,
    },
}

pub fn validate(model: &DataModel) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let ns = &model.meta.id;

    // Namespace ID range check (10-bit field = max 1023)
    if let Some(ns_id) = model.meta.namespace_id {
        if ns_id > 1023 {
            errors.push(ValidationError::NamespaceIdOutOfRange {
                namespace: ns.clone(),
                id: ns_id,
            });
        }
    }

    // Class count limit (5-bit field = max 31, but index 0 is often reserved)
    if model.classes.len() > 31 {
        errors.push(ValidationError::TooManyClasses {
            namespace: ns.clone(),
            count: model.classes.len(),
        });
    }

    // Duplicate class IDs and class_index checks
    let mut class_ids = HashSet::new();
    let mut class_indices = HashSet::new();
    for class in &model.classes {
        if !class_ids.insert(&class.id) {
            errors.push(ValidationError::DuplicateClassId {
                namespace: ns.clone(),
                id: class.id.clone(),
            });
        }

        if let Some(ci) = class.class_index {
            if ci > 31 {
                errors.push(ValidationError::ClassIndexOutOfRange {
                    namespace: ns.clone(),
                    class: class.id.clone(),
                    index: ci,
                });
            } else if !class_indices.insert(ci) {
                errors.push(ValidationError::DuplicateClassIndex {
                    namespace: ns.clone(),
                    index: ci,
                });
            }
        }

        // Key count limit (10-bit field = max 1023)
        if class.keys.len() > 1023 {
            errors.push(ValidationError::TooManyKeys {
                namespace: ns.clone(),
                class: class.id.clone(),
                count: class.keys.len(),
            });
        }

        // Duplicate key IDs and key_index checks within class
        let mut key_ids = HashSet::new();
        let mut key_indices = HashSet::new();
        for key in &class.keys {
            if !key_ids.insert(&key.id) {
                errors.push(ValidationError::DuplicateKeyId {
                    namespace: ns.clone(),
                    class: class.id.clone(),
                    key: key.id.clone(),
                });
            }

            if let Some(ki) = key.key_index {
                if ki > 1023 {
                    errors.push(ValidationError::KeyIndexOutOfRange {
                        namespace: ns.clone(),
                        class: class.id.clone(),
                        key: key.id.clone(),
                        index: ki,
                    });
                } else if !key_indices.insert(ki) {
                    errors.push(ValidationError::DuplicateKeyIndex {
                        namespace: ns.clone(),
                        class: class.id.clone(),
                        index: ki,
                    });
                }
            }

            // String/binary require max_size
            if key.data_type == DataType::String && key.max_size.is_none() {
                errors.push(ValidationError::StringMissingMaxSize {
                    namespace: ns.clone(),
                    class: class.id.clone(),
                    key: key.id.clone(),
                });
            }
            if key.data_type == DataType::Binary && key.max_size.is_none() {
                errors.push(ValidationError::BinaryMissingMaxSize {
                    namespace: ns.clone(),
                    class: class.id.clone(),
                    key: key.id.clone(),
                });
            }

            // Read-only keys cannot be persistent
            if key.read_only && key.persistent {
                errors.push(ValidationError::ReadOnlyPersistent {
                    namespace: ns.clone(),
                    class: class.id.clone(),
                    key: key.id.clone(),
                });
            }

            // Enum references must exist
            if let Some(ref enum_name) = key.enum_ref {
                if !model.enums.contains_key(enum_name) {
                    errors.push(ValidationError::UndefinedEnum {
                        namespace: ns.clone(),
                        class: class.id.clone(),
                        key: key.id.clone(),
                        enum_name: enum_name.clone(),
                    });
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_model(toml_str: &str) -> DataModel {
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn valid_minimal_example() {
        let model = parse_model(include_str!("../../examples/minimal.toml"));
        assert!(validate(&model).is_ok());
    }

    #[test]
    fn valid_battery_example() {
        let model = parse_model(include_str!("../../examples/battery.toml"));
        assert!(validate(&model).is_ok());
    }

    #[test]
    fn detect_duplicate_class_id() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "status"

[[classes]]
id = "status"
"#,
        );
        let errs = validate(&model).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::DuplicateClassId { .. })));
    }

    #[test]
    fn detect_duplicate_key_id() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "status"

[[classes.keys]]
id = "foo"
type = "uint8"

[[classes.keys]]
id = "foo"
type = "uint16"
"#,
        );
        let errs = validate(&model).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::DuplicateKeyId { .. })));
    }

    #[test]
    fn detect_string_missing_max_size() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "cfg"

[[classes.keys]]
id = "name"
type = "string"
"#,
        );
        let errs = validate(&model).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::StringMissingMaxSize { .. })));
    }

    #[test]
    fn detect_read_only_persistent() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "cfg"

[[classes.keys]]
id = "version"
type = "uint32"
read_only = true
persistent = true
"#,
        );
        let errs = validate(&model).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::ReadOnlyPersistent { .. })));
    }

    #[test]
    fn detect_undefined_enum() {
        let model = parse_model(
            r#"
[meta]
id = "test"
version = "1.0.0"

[[classes]]
id = "status"

[[classes.keys]]
id = "mode"
type = "uint8"
enum = "nonexistent"
"#,
        );
        let errs = validate(&model).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::UndefinedEnum { .. })));
    }
}
