use crate::model::schema::DataModel;
use serde::Serialize;

/// A single named value within a generated C enum.
#[derive(Debug, Serialize)]
pub struct EnumMemberRenderable {
    pub name: String,
    pub value: i64,
}

/// A named enum ready for `dm_enums.h` template rendering.
#[derive(Debug, Serialize)]
pub struct EnumRenderable {
    pub type_name: String,
    pub doc: Option<String>,
    pub members: Vec<EnumMemberRenderable>,
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

/// C identifier stem shared by an enum's typedef name and its members.
fn enum_stem(raw: &str, model_id: &str) -> String {
    let (ns, name) = split_enum_key(raw, model_id);
    format!("{}_{}", ns.to_uppercase(), name.to_uppercase())
}

/// Resolve the generated C typedef name for a key's `enum = "..."` reference.
pub fn enum_type_name(raw: &str, model_id: &str) -> String {
    format!("{}_T", enum_stem(raw, model_id))
}

/// Collect every named enum in the model as a renderable C typedef enum.
///
/// Per-variant overrides are expected to already be resolved (see
/// `main::resolve_variant`) — this just renders whatever `values` map is
/// currently on each `EnumDef`.
pub fn collect_enums(model: &DataModel) -> Vec<EnumRenderable> {
    model
        .enums
        .iter()
        .map(|(raw_name, def)| {
            let stem = enum_stem(raw_name, &model.meta.id);

            let mut members: Vec<(String, i64)> = def
                .values
                .iter()
                .map(|(name, &value)| {
                    let member_part = name.to_uppercase().replace(' ', "_");
                    (format!("{stem}_{member_part}"), value)
                })
                .collect();
            members.sort_by_key(|(_, value)| *value);

            EnumRenderable {
                type_name: format!("{stem}_T"),
                doc: def.doc.clone(),
                members: members
                    .into_iter()
                    .map(|(name, value)| EnumMemberRenderable { name, value })
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_uses_model_id_when_unqualified() {
        assert_eq!(enum_type_name("level", "battery"), "BATTERY_LEVEL_T");
    }

    #[test]
    fn type_name_uses_qualified_namespace_when_present() {
        assert_eq!(
            enum_type_name("battery::level", "unified"),
            "BATTERY_LEVEL_T"
        );
    }

    #[test]
    fn collect_enums_for_battery_example() {
        let toml_str = include_str!("../../examples/battery.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();

        let enums = collect_enums(&model);
        assert_eq!(enums.len(), 2);

        let level = enums
            .iter()
            .find(|e| e.type_name == "BATTERY_LEVEL_T")
            .unwrap();
        assert_eq!(level.members.len(), 7);
        assert_eq!(level.members[0].name, "BATTERY_LEVEL_UNKNOWN");
        assert_eq!(level.members[0].value, 0);
        assert_eq!(level.members.last().unwrap().name, "BATTERY_LEVEL_FULL");
        assert_eq!(level.members.last().unwrap().value, 6);

        let state = enums
            .iter()
            .find(|e| e.type_name == "BATTERY_STATE_T")
            .unwrap();
        assert_eq!(state.members.len(), 2);
    }

    #[test]
    fn members_are_sorted_by_value() {
        let toml_str = include_str!("../../examples/battery.toml");
        let model: DataModel = toml::from_str(toml_str).unwrap();
        let enums = collect_enums(&model);

        let level = enums
            .iter()
            .find(|e| e.type_name == "BATTERY_LEVEL_T")
            .unwrap();
        let values: Vec<i64> = level.members.iter().map(|m| m.value).collect();
        let mut sorted = values.clone();
        sorted.sort();
        assert_eq!(values, sorted);
    }
}
