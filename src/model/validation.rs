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

    #[error("instance '{id}' in namespace '{namespace}' has empty expression")]
    EmptyExpression { namespace: String, id: String },
}

pub fn validate(model: &DataModel) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let ns = &model.meta.id;

    // Class count limit (5-bit field = max 31, but index 0 is often reserved)
    if model.classes.len() > 31 {
        errors.push(ValidationError::TooManyClasses {
            namespace: ns.clone(),
            count: model.classes.len(),
        });
    }

    // Duplicate class IDs
    let mut class_ids = HashSet::new();
    for class in &model.classes {
        if !class_ids.insert(&class.id) {
            errors.push(ValidationError::DuplicateClassId {
                namespace: ns.clone(),
                id: class.id.clone(),
            });
        }

        // Key count limit (10-bit field = max 1023)
        if class.keys.len() > 1023 {
            errors.push(ValidationError::TooManyKeys {
                namespace: ns.clone(),
                class: class.id.clone(),
                count: class.keys.len(),
            });
        }

        // Duplicate key IDs within class
        let mut key_ids = HashSet::new();
        for key in &class.keys {
            if !key_ids.insert(&key.id) {
                errors.push(ValidationError::DuplicateKeyId {
                    namespace: ns.clone(),
                    class: class.id.clone(),
                    key: key.id.clone(),
                });
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

    // Instance validation
    for inst in &model.instances {
        if inst.expr.trim().is_empty() {
            errors.push(ValidationError::EmptyExpression {
                namespace: ns.clone(),
                id: inst.id.clone(),
            });
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
