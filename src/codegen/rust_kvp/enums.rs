use crate::model::schema::DataModel;
use serde::Serialize;

/// A single named value within a generated Rust `#[repr]` enum.
#[derive(Debug, Serialize)]
pub struct RustEnumMember {
    pub name: String,
    pub value: i64,
}

/// A named enum ready for `dm_rust.rs` template rendering.
#[derive(Debug, Serialize)]
pub struct RustEnumRenderable {
    pub type_name: String,
    /// Rust primitive backing type, e.g. "u8" — smallest type that can
    /// represent every member value.
    pub repr: String,
    pub doc: Option<String>,
    pub members: Vec<RustEnumMember>,
    /// Variant `from_raw` falls back to for a raw value matching no member
    /// (the lowest-valued member, mirroring the C backend's "index 0 wins").
    pub fallback_name: String,
}

/// Split a (possibly namespace-qualified) enum key into `(namespace, name)`.
///
/// Multi-file merges qualify enum keys as `"{namespace}::{enum_name}"` (see
/// `main::merge_models`); single-file models leave the key bare, in which
/// case the model's own `meta.id` is the namespace.
fn split_enum_key<'a>(raw: &'a str, model_id: &'a str) -> (&'a str, &'a str) {
    match raw.split_once("::") {
        Some((ns, name)) => (ns, name),
        None => (model_id, raw),
    }
}

/// Convert a `snake_case`/`kebab-case` identifier into `PascalCase`.
fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Rust type name for an enum, namespace-qualified to avoid collisions
/// between namespaces that happen to define an enum with the same name.
fn enum_type_name(raw: &str, model_id: &str) -> String {
    let (ns, name) = split_enum_key(raw, model_id);
    format!("{}{}", to_pascal_case(ns), to_pascal_case(name))
}

/// Smallest Rust primitive integer type that can represent every value.
fn select_repr(values: impl Iterator<Item = i64>) -> &'static str {
    let mut min = 0i64;
    let mut max = 0i64;
    let mut any = false;
    for v in values {
        if !any {
            min = v;
            max = v;
            any = true;
        } else {
            min = min.min(v);
            max = max.max(v);
        }
    }

    if min < 0 {
        if min >= i64::from(i8::MIN) && max <= i64::from(i8::MAX) {
            "i8"
        } else if min >= i64::from(i16::MIN) && max <= i64::from(i16::MAX) {
            "i16"
        } else {
            "i32"
        }
    } else if max <= i64::from(u8::MAX) {
        "u8"
    } else if max <= i64::from(u16::MAX) {
        "u16"
    } else {
        "u32"
    }
}

/// Resolve the generated Rust enum type name + repr for a key's
/// `enum = "..."` reference by looking the definition up in the model.
///
/// Returns `None` if `raw_ref` doesn't name a known enum — callers only
/// reach this after model validation, so that shouldn't happen in practice.
pub fn resolve_enum_ref(model: &DataModel, raw_ref: &str) -> Option<(String, String)> {
    let def = model.enums.get(raw_ref)?;
    let type_name = enum_type_name(raw_ref, &model.meta.id);
    let repr = select_repr(def.values.values().copied()).to_string();
    Some((type_name, repr))
}

/// Collect every named enum in the model as a renderable Rust `#[repr]` enum.
pub fn collect_rust_enums(model: &DataModel) -> Vec<RustEnumRenderable> {
    model
        .enums
        .iter()
        .map(|(raw_name, def)| {
            let type_name = enum_type_name(raw_name, &model.meta.id);
            let repr = select_repr(def.values.values().copied()).to_string();

            let mut members: Vec<(String, i64)> = def
                .values
                .iter()
                .map(|(name, &value)| (to_pascal_case(name), value))
                .collect();
            members.sort_by_key(|(_, value)| *value);

            let fallback_name = members
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            RustEnumRenderable {
                type_name,
                repr,
                doc: def.doc.clone(),
                members: members
                    .into_iter()
                    .map(|(name, value)| RustEnumMember { name, value })
                    .collect(),
                fallback_name,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_from_snake_case() {
        assert_eq!(to_pascal_case("operating_mode"), "OperatingMode");
        assert_eq!(to_pascal_case("one_bar"), "OneBar");
        assert_eq!(to_pascal_case("battery"), "Battery");
    }

    #[test]
    fn type_name_is_namespace_qualified() {
        assert_eq!(enum_type_name("level", "battery"), "BatteryLevel");
        assert_eq!(enum_type_name("battery::level", "unified"), "BatteryLevel");
    }

    #[test]
    fn repr_picks_smallest_unsigned_type() {
        assert_eq!(select_repr([0, 1, 6].into_iter()), "u8");
        assert_eq!(select_repr([0, 1, 254, 255].into_iter()), "u8");
        assert_eq!(select_repr([0, 300].into_iter()), "u16");
        assert_eq!(select_repr([0, 70000].into_iter()), "u32");
    }

    #[test]
    fn repr_picks_smallest_signed_type_when_negative() {
        assert_eq!(select_repr([-1, 0, 1].into_iter()), "i8");
        assert_eq!(select_repr([-200, 0].into_iter()), "i16");
        assert_eq!(select_repr([-70000, 0].into_iter()), "i32");
    }

    #[test]
    fn collect_rust_enums_for_battery_example() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/battery.toml")).unwrap();
        let enums = collect_rust_enums(&model);
        assert_eq!(enums.len(), 2);

        let level = enums
            .iter()
            .find(|e| e.type_name == "BatteryLevel")
            .unwrap();
        assert_eq!(level.repr, "u8");
        assert_eq!(level.members.len(), 7);
        assert_eq!(level.members[0].name, "Unknown");
        assert_eq!(level.fallback_name, "Unknown");
        assert_eq!(level.members.last().unwrap().name, "Full");
    }

    #[test]
    fn resolve_enum_ref_matches_collect_rust_enums() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/battery.toml")).unwrap();
        let (type_name, repr) = resolve_enum_ref(&model, "level").unwrap();
        assert_eq!(type_name, "BatteryLevel");
        assert_eq!(repr, "u8");
    }

    #[test]
    fn resolve_enum_ref_unknown_returns_none() {
        let model: DataModel =
            toml::from_str(include_str!("../../../examples/battery.toml")).unwrap();
        assert!(resolve_enum_ref(&model, "nonexistent").is_none());
    }
}
