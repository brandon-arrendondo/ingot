pub mod enums;
pub mod rust_kvp;
pub mod storage;
pub mod target;
pub mod yaml_manifest;

use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType};
use serde::Serialize;
use std::path::Path;
use tera::{Context, Tera};

pub(crate) fn write_generated(path: impl AsRef<Path>, content: String) -> std::io::Result<()> {
    let normalized = format!("{}\n", content.trim_end_matches('\n'));
    std::fs::write(path, normalized)
}

/// A key definition ready for template rendering.
#[derive(Debug, Serialize)]
pub struct KeyDefRenderable {
    pub namespace: String,
    pub class: String,
    pub name: String,
    pub define_name: String,
    pub hex_value: String,
    pub type_name: String,
    pub unit: Option<String>,
    pub read_only: bool,
    pub thread_safe: bool,
    pub persistent: bool,
    pub event: bool,
}

/// A single event key ready for the dispatch-by-key switch.
#[derive(Debug, Serialize)]
struct EventKeyRenderable {
    /// The key #define (e.g. "DM_KEY_APPLIANCE_STATUS_MODE") — the switch case.
    define_name: String,
    /// The tinyfsm event struct (e.g. "FSM_EVENT_APPLIANCE_STATUS_MODE").
    fsm_event_name: String,
}

/// Event keys grouped by (namespace, class) for the event-struct header.
#[derive(Debug, Serialize)]
struct EventGroupRenderable {
    namespace: String,
    class: String,
    /// FSM event struct names in this namespace/class, in declaration order.
    events: Vec<String>,
}

/// Integer type descriptor for the API dispatch templates.
#[derive(Debug, Serialize)]
struct DmIntTypeInfo {
    type_enum: String,
    c_type: String,
    val_field: String,
    get_fn: String,
    set_fn: String,
    wrapper_suffix: String,
}

/// Map integer storage suffix to API dispatch info.
fn int_type_info(suffix: &str) -> DmIntTypeInfo {
    let (type_enum, c_type, val_field, wrapper) = match suffix {
        "UINT8" => ("DM_KEY_TYPE_UINT8", "uint8_t", "u8val", "UInt8"),
        "SINT8" => ("DM_KEY_TYPE_INT8", "int8_t", "s8val", "SInt8"),
        "UINT16" => ("DM_KEY_TYPE_UINT16", "uint16_t", "u16val", "UInt16"),
        "SINT16" => ("DM_KEY_TYPE_INT16", "int16_t", "s16val", "SInt16"),
        "UINT32" => ("DM_KEY_TYPE_UINT32", "uint32_t", "u32val", "UInt32"),
        "SINT32" => ("DM_KEY_TYPE_INT32", "int32_t", "s32val", "SInt32"),
        _ => unreachable!("unknown integer suffix: {suffix}"),
    };
    DmIntTypeInfo {
        type_enum: type_enum.to_string(),
        c_type: c_type.to_string(),
        val_field: val_field.to_string(),
        get_fn: format!("IntegerStorage_Get{suffix}Key"),
        set_fn: format!("IntegerStorage_Set{suffix}Key"),
        wrapper_suffix: wrapper.to_string(),
    }
}

/// Generate all C code from a parsed data model.
pub fn generate(
    model: &DataModel,
    ns_id: u16,
    output_dir: &Path,
    template_dir: &Path,
    target: &target::TargetConfig,
    no_events: bool,
    emit_tinyfsm: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;

    let tera = Tera::new(
        template_dir
            .join("*")
            .to_str()
            .ok_or("invalid template path")?,
    )?;

    let version = env!("CARGO_PKG_VERSION");

    // Collect all key definitions
    let key_defs = collect_key_definitions(model, ns_id);

    // Generate key_definitions.h
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("keys", &key_defs);
        let rendered = tera.render("key_definitions.h", &ctx)?;
        write_generated(output_dir.join("key_definitions.h"), rendered)?;
        log::info!("Generated key_definitions.h ({} keys)", key_defs.len());
    }

    // Generate C++/tinyfsm event artifacts (opt-in, additive — see --emit-tinyfsm).
    // Off by default so C99-only consumers and their output are untouched.
    if emit_tinyfsm {
        generate_tinyfsm_events(&tera, version, &key_defs, output_dir)?;
    }

    // Generate jenkins_hash.h and jenkins_hash.c
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        let h = tera.render("jenkins_hash.h", &ctx)?;
        let c = tera.render("jenkins_hash.c", &ctx)?;
        write_generated(output_dir.join("jenkins_hash.h"), h)?;
        write_generated(output_dir.join("jenkins_hash.c"), c)?;
        log::info!("Generated jenkins_hash.h/.c");
    }

    // Generate dm_key.h
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        let h = tera.render("dm_key.h", &ctx)?;
        write_generated(output_dir.join("dm_key.h"), h)?;
        log::info!("Generated dm_key.h");
    }

    // Generate dm_namespace_definitions.h
    {
        let namespaces = collect_namespaces(model, ns_id);
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("namespaces", &namespaces);
        let h = tera.render("dm_namespace_definitions.h", &ctx)?;
        write_generated(output_dir.join("dm_namespace_definitions.h"), h)?;
        log::info!("Generated dm_namespace_definitions.h");
    }

    // Generate dm_enums.h (only when the model defines named enums)
    let enum_defs = enums::collect_enums(model);
    if !enum_defs.is_empty() {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("enums", &enum_defs);
        let h = tera.render("dm_enums.h", &ctx)?;
        write_generated(output_dir.join("dm_enums.h"), h)?;
        log::info!("Generated dm_enums.h ({} enum(s))", enum_defs.len());
    }

    // Generate dm_full.yaml manifest
    yaml_manifest::generate_yaml_manifest(model, ns_id, output_dir)?;

    // --- Collect all storage data ---
    let bool_storage = storage::boolean::collect_boolean_storage(model, ns_id)?;
    let int_storages = storage::integer::collect_integer_storage(model, ns_id)?;
    let str_storage = storage::string::collect_string_storage(model, ns_id)?;
    let persist_storage = storage::persistence::collect_persistence_storage(model, ns_id);

    // Generate boolean_storage.h/.c
    if let Some(ref bs) = bool_storage {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("bool", bs);
        let h = tera.render("boolean_storage.h", &ctx)?;
        let c = tera.render("boolean_storage.c", &ctx)?;
        write_generated(output_dir.join("boolean_storage.h"), h)?;
        write_generated(output_dir.join("boolean_storage.c"), c)?;
        log::info!(
            "Generated boolean_storage.h/.c ({} keys, {} word(s))",
            bs.num_keys,
            bs.num_words
        );
    }

    // Generate integer_storage.h/.c
    if !int_storages.is_empty() {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("types", &int_storages);
        let h = tera.render("integer_storage.h", &ctx)?;
        let c = tera.render("integer_storage.c", &ctx)?;
        write_generated(output_dir.join("integer_storage.h"), h)?;
        write_generated(output_dir.join("integer_storage.c"), c)?;
        log::info!(
            "Generated integer_storage.h/.c ({} type groups)",
            int_storages.len()
        );
    }

    // Generate string_storage.h/.c
    if let Some(ref ss) = str_storage {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("total_keys", &ss.total_keys);
        ctx.insert("ro", &ss.ro);
        ctx.insert("rw", &ss.rw);
        let h = tera.render("string_storage.h", &ctx)?;
        let c = tera.render("string_storage.c", &ctx)?;
        write_generated(output_dir.join("string_storage.h"), h)?;
        write_generated(output_dir.join("string_storage.c"), c)?;
        log::info!(
            "Generated string_storage.h/.c ({} total, {} RO, {} RW)",
            ss.total_keys,
            ss.ro.as_ref().map_or(0, |g| g.num_keys),
            ss.rw.as_ref().map_or(0, |g| g.num_keys),
        );
    }

    // Generate persistence_storage.h/.c
    if let Some(ref ps) = persist_storage {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("persistence", ps);
        let h = tera.render("persistence_storage.h", &ctx)?;
        let c = tera.render("persistence_storage.c", &ctx)?;
        write_generated(output_dir.join("persistence_storage.h"), h)?;
        write_generated(output_dir.join("persistence_storage.c"), c)?;
        log::info!("Generated persistence_storage.h/.c ({} keys)", ps.num_keys);
    }

    // --- Generate dm.h / dm.c (main API layer) ---
    {
        let has_bool = bool_storage.is_some();
        let has_integers = !int_storages.is_empty();
        let has_ro_strings = str_storage.as_ref().and_then(|s| s.ro.as_ref()).is_some();
        let has_rw_strings = str_storage.as_ref().and_then(|s| s.rw.as_ref()).is_some();

        let has_persistence = persist_storage.is_some();

        let api_int_types: Vec<DmIntTypeInfo> = int_storages
            .iter()
            .map(|s| int_type_info(&s.suffix))
            .collect();

        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("has_bool", &has_bool);
        ctx.insert("has_integers", &has_integers);
        ctx.insert("has_ro_strings", &has_ro_strings);
        ctx.insert("has_rw_strings", &has_rw_strings);
        ctx.insert("has_persistence", &has_persistence);
        ctx.insert("int_types", &api_int_types);
        ctx.insert("target", target);
        ctx.insert("no_events", &no_events);

        let h = tera.render("dm.h", &ctx)?;
        let c = tera.render("dm.c", &ctx)?;
        write_generated(output_dir.join("dm.h"), h)?;
        write_generated(output_dir.join("dm.c"), c)?;
        log::info!(
            "Generated dm.h/.c (bool={}, int_types={}, ro_str={}, rw_str={})",
            has_bool,
            api_int_types.len(),
            has_ro_strings,
            has_rw_strings,
        );
    }

    // --- Generate dm_helpers.h / dm_helpers.c ---
    {
        let helpers = collect_helpers(model, ns_id);
        if !helpers.is_empty() {
            let has_string_helpers = helpers.iter().any(|h| h.is_string);
            let has_enum_helpers = helpers.iter().any(|h| h.is_enum);
            let mut ctx = Context::new();
            ctx.insert("version", version);
            ctx.insert("helpers", &helpers);
            ctx.insert("has_string_helpers", &has_string_helpers);
            ctx.insert("has_enum_helpers", &has_enum_helpers);
            let h = tera.render("dm_helpers.h", &ctx)?;
            write_generated(output_dir.join("dm_helpers.h"), h)?;
            if has_string_helpers {
                let c = tera.render("dm_helpers.c", &ctx)?;
                write_generated(output_dir.join("dm_helpers.c"), c)?;
            }
            log::info!(
                "Generated dm_helpers.h{} ({} helpers)",
                if has_string_helpers { "/.c" } else { "" },
                helpers.len()
            );
        }
    }

    // --- Generate Unity test files ---
    {
        let test_keys = collect_test_keys(model, ns_id);
        let has_persistence = persist_storage.is_some();
        let persist_test_entries = collect_persistence_test_entries(model, ns_id);
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("keys", &test_keys);
        ctx.insert("namespace", &model.meta.id);
        ctx.insert("no_events", &no_events);
        ctx.insert("has_persistence", &has_persistence);
        ctx.insert("persistence_entries", &persist_test_entries);
        let test_c = tera.render("test_dm.c", &ctx)?;
        write_generated(output_dir.join("test_dm.c"), test_c)?;
        let cmake = tera.render("CMakeLists.txt", &ctx)?;
        write_generated(output_dir.join("CMakeLists.txt"), cmake)?;
        log::info!(
            "Generated test_dm.c + CMakeLists.txt ({} test keys)",
            test_keys.len()
        );
    }

    Ok(())
}

/// Render the C++/tinyfsm event header + dispatch-by-key wrapper.
///
/// Emits nothing when no key carries `event = true` — a consumer with no
/// event keys gets no C++ artifacts even with the flag on.
fn generate_tinyfsm_events(
    tera: &Tera,
    version: &str,
    key_defs: &[KeyDefRenderable],
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (events, groups) = collect_event_keys(key_defs);
    if events.is_empty() {
        log::info!("No event keys — skipping tinyfsm event generation");
        return Ok(());
    }

    let mut hpp_ctx = Context::new();
    hpp_ctx.insert("version", version);
    hpp_ctx.insert("groups", &groups);
    let hpp = tera.render("dm_key_events.hpp", &hpp_ctx)?;
    write_generated(output_dir.join("dm_key_events.hpp"), hpp)?;

    let mut wrap_ctx = Context::new();
    wrap_ctx.insert("version", version);
    wrap_ctx.insert("events", &events);
    let wrapper_h = tera.render("dm_key_events_wrapper.hpp", &wrap_ctx)?;
    let wrapper_c = tera.render("dm_key_events_wrapper.cpp", &wrap_ctx)?;
    write_generated(output_dir.join("dm_key_events_wrapper.hpp"), wrapper_h)?;
    write_generated(output_dir.join("dm_key_events_wrapper.cpp"), wrapper_c)?;

    log::info!(
        "Generated dm_key_events.hpp + dm_key_events_wrapper.hpp/.cpp ({} event keys)",
        events.len()
    );
    Ok(())
}

/// Build the flat dispatch list + (namespace, class)-grouped struct list for
/// the `event = true` keys.
///
/// The FSM event struct name mirrors the key #define with the `DM_KEY_`
/// prefix swapped for `FSM_EVENT_`, matching the UDM generator's naming so
/// ingot is a drop-in replacement.
fn collect_event_keys(
    key_defs: &[KeyDefRenderable],
) -> (Vec<EventKeyRenderable>, Vec<EventGroupRenderable>) {
    let mut events = Vec::new();
    let mut groups: Vec<EventGroupRenderable> = Vec::new();

    for key in key_defs.iter().filter(|k| k.event) {
        let fsm_event_name = key.define_name.replacen("DM_KEY_", "FSM_EVENT_", 1);
        events.push(EventKeyRenderable {
            define_name: key.define_name.clone(),
            fsm_event_name: fsm_event_name.clone(),
        });

        let namespace = key.namespace.to_uppercase();
        let class = key.class.to_uppercase();
        match groups.last_mut() {
            Some(g) if g.namespace == namespace && g.class == class => {
                g.events.push(fsm_event_name)
            }
            _ => groups.push(EventGroupRenderable {
                namespace,
                class,
                events: vec![fsm_event_name],
            }),
        }
    }

    (events, groups)
}

/// A namespace entry for template rendering.
#[derive(Debug, Serialize)]
struct NamespaceDefRenderable {
    namespace_upper: String,
    ns_id: u16,
}

/// Collect unique namespaces from the model (deduplicated, ordered).
fn collect_namespaces(model: &DataModel, fallback_ns_id: u16) -> Vec<NamespaceDefRenderable> {
    let mut seen = std::collections::BTreeMap::new();
    for class in &model.classes {
        let name = class
            .namespace_name
            .as_deref()
            .unwrap_or(&model.meta.id)
            .to_uppercase();
        let id = class.namespace_id.unwrap_or(fallback_ns_id);
        seen.entry(name).or_insert(id);
    }
    if seen.is_empty() {
        // No classes — still emit the model-level namespace
        seen.insert(model.meta.id.to_uppercase(), fallback_ns_id);
    }
    seen.into_iter()
        .map(|(namespace_upper, ns_id)| NamespaceDefRenderable {
            namespace_upper,
            ns_id,
        })
        .collect()
}

/// Resolve the namespace ID for a class, using per-class override or model-level fallback.
pub(crate) fn resolve_ns_id(class: &crate::model::schema::Class, fallback: u16) -> u16 {
    class.namespace_id.unwrap_or(fallback)
}

/// Resolve the namespace name for a class, using per-class override or model-level fallback.
pub(crate) fn resolve_ns_name(class: &crate::model::schema::Class, model: &DataModel) -> String {
    class
        .namespace_name
        .as_deref()
        .unwrap_or(&model.meta.id)
        .to_string()
}

/// Resolve the class index, using explicit class_index or positional fallback.
pub(crate) fn resolve_class_idx(class: &crate::model::schema::Class, position: usize) -> u8 {
    class.class_index.unwrap_or(position as u8)
}

/// Build renderable key definitions from the model.
fn collect_key_definitions(model: &DataModel, ns_id: u16) -> Vec<KeyDefRenderable> {
    let mut defs = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_id = resolve_ns_id(class, ns_id);
        let c_ns_name = resolve_ns_name(class, model).to_uppercase();
        let c_idx = resolve_class_idx(class, pos);
        let class_name = class.id.to_uppercase();
        for (key_pos, key) in class.keys.iter().enumerate() {
            let type_code = key.data_type.type_code();

            let encoding = KeyEncoding {
                namespace: c_ns_id,
                class: c_idx,
                id: key.key_index.unwrap_or(key_pos as u16),
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };

            let encoded = encoding.encode();
            let key_name = key.id.to_uppercase().replace(' ', "_");

            defs.push(KeyDefRenderable {
                namespace: resolve_ns_name(class, model),
                class: class.id.clone(),
                name: key.id.clone(),
                define_name: format!("DM_KEY_{c_ns_name}_{class_name}_{key_name}"),
                hex_value: format!("{encoded:#010X}"),
                type_name: format!("{:?}", key.data_type).to_lowercase(),
                unit: key.unit.clone(),
                read_only: key.read_only,
                thread_safe: key.thread_safe,
                persistent: key.persistent,
                event: key.event,
            });
        }
    }

    defs
}

/// A helper getter/setter entry for template rendering.
#[derive(Debug, Serialize)]
struct HelperEntry {
    /// The key #define name (e.g. "DM_KEY_BATTERY_STATUS_VOLTAGE")
    define_name: String,
    /// Helper function suffix (e.g. "BATTERY_STATUS_VOLTAGE")
    helper_name: String,
    /// C type for the value (e.g. "uint16_t", "bool")
    c_type: String,
    /// dm_val_t union field (e.g. "u16val", "bval") — empty for strings
    val_field: String,
    /// True for string-type keys
    is_string: bool,
    /// True for read-only keys
    is_read_only: bool,
    /// True when this key has an `enum = "..."` reference; `c_type` is then
    /// the generated enum typedef name rather than the raw integer type.
    is_enum: bool,
    /// Raw integer C type backing an enum-typed key (unused otherwise).
    storage_c_type: String,
}

/// Collect helper entries for keys with helpers=true.
fn collect_helpers(model: &DataModel, ns_id: u16) -> Vec<HelperEntry> {
    let mut helpers = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_id = resolve_ns_id(class, ns_id);
        let c_ns_name = resolve_ns_name(class, model).to_uppercase();
        let c_idx = resolve_class_idx(class, pos);
        let class_name = class.id.to_uppercase();

        for (key_pos, key) in class.keys.iter().enumerate() {
            if !key.helpers {
                continue;
            }

            let type_code = key.data_type.type_code();
            let encoding = KeyEncoding {
                namespace: c_ns_id,
                class: c_idx,
                id: key.key_index.unwrap_or(key_pos as u16),
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };
            // Ensure encoding is used (validate key is encodable)
            let _ = encoding.encode();

            let key_name = key.id.to_uppercase().replace(' ', "_");
            let define_name = format!("DM_KEY_{c_ns_name}_{class_name}_{key_name}");
            let helper_name = format!("{c_ns_name}_{class_name}_{key_name}");

            let (raw_c_type, val_field, is_string) = match key.data_type {
                DataType::Bool => ("bool".to_string(), "bval".to_string(), false),
                DataType::Uint8 => ("uint8_t".to_string(), "u8val".to_string(), false),
                DataType::Int8 => ("int8_t".to_string(), "s8val".to_string(), false),
                DataType::Uint16 => ("uint16_t".to_string(), "u16val".to_string(), false),
                DataType::Int16 => ("int16_t".to_string(), "s16val".to_string(), false),
                DataType::Uint32 => ("uint32_t".to_string(), "u32val".to_string(), false),
                DataType::Int32 => ("int32_t".to_string(), "s32val".to_string(), false),
                DataType::String => ("const char *".to_string(), String::new(), true),
                DataType::Binary => ("const uint8_t *".to_string(), String::new(), true),
            };

            let is_enum = !is_string && key.enum_ref.is_some();
            let c_type = match (&key.enum_ref, is_enum) {
                (Some(enum_ref), true) => enums::enum_type_name(enum_ref, &model.meta.id),
                _ => raw_c_type.clone(),
            };

            helpers.push(HelperEntry {
                define_name,
                helper_name,
                c_type,
                val_field,
                is_string,
                is_read_only: key.read_only,
                is_enum,
                storage_c_type: raw_c_type,
            });
        }
    }

    helpers
}

/// A persistence test entry for Unity test generation.
#[derive(Debug, Serialize)]
struct PersistenceTestEntry {
    define_name: String,
    c_type: String,
    val_field: String,
    is_string: bool,
    is_bool: bool,
    default_c: String,
    test_c: String,
}

/// A test case entry for Unity test generation.
#[derive(Debug, Serialize)]
struct TestKeyEntry {
    define_name: String,
    c_type: String,
    val_field: String,
    is_string: bool,
    is_bool: bool,
    read_only: bool,
    /// C literal for the default value
    default_c: String,
    /// C literal for a test value (different from default)
    test_c: String,
}

/// Collect test entries for every key in the model.
fn collect_test_keys(model: &DataModel, ns_id: u16) -> Vec<TestKeyEntry> {
    let mut entries = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_id = resolve_ns_id(class, ns_id);
        let c_ns_name = resolve_ns_name(class, model).to_uppercase();
        let c_idx = resolve_class_idx(class, pos);
        let class_name = class.id.to_uppercase();

        for (key_pos, key) in class.keys.iter().enumerate() {
            let key_name = key.id.to_uppercase().replace(' ', "_");
            let define_name = format!("DM_KEY_{c_ns_name}_{class_name}_{key_name}");

            let (c_type, val_field, is_string, is_bool) = match key.data_type {
                DataType::Bool => ("bool", "bval", false, true),
                DataType::Uint8 => ("uint8_t", "u8val", false, false),
                DataType::Int8 => ("int8_t", "s8val", false, false),
                DataType::Uint16 => ("uint16_t", "u16val", false, false),
                DataType::Int16 => ("int16_t", "s16val", false, false),
                DataType::Uint32 => ("uint32_t", "u32val", false, false),
                DataType::Int32 => ("int32_t", "s32val", false, false),
                DataType::String => ("const char *", "", true, false),
                DataType::Binary => ("const uint8_t *", "", true, false),
            };

            let default_c = format_test_default(&key.default, key.data_type);
            let test_c = format_test_value(key.data_type, &default_c);

            let type_code = key.data_type.type_code();
            let _encoding = KeyEncoding {
                namespace: c_ns_id,
                class: c_idx,
                id: key.key_index.unwrap_or(key_pos as u16),
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };

            entries.push(TestKeyEntry {
                define_name,
                c_type: c_type.to_string(),
                val_field: val_field.to_string(),
                is_string,
                is_bool,
                read_only: key.read_only,
                default_c,
                test_c,
            });
        }
    }

    entries
}

/// Collect persistence test entries for persistent keys in the model.
fn collect_persistence_test_entries(model: &DataModel, ns_id: u16) -> Vec<PersistenceTestEntry> {
    let mut entries = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_id = resolve_ns_id(class, ns_id);
        let c_ns_name = resolve_ns_name(class, model).to_uppercase();
        let c_idx = resolve_class_idx(class, pos);
        let class_name = class.id.to_uppercase();

        for (key_pos, key) in class.keys.iter().enumerate() {
            if !key.persistent {
                continue;
            }

            let key_name = key.id.to_uppercase().replace(' ', "_");
            let define_name = format!("DM_KEY_{c_ns_name}_{class_name}_{key_name}");

            let (c_type, val_field, is_string, is_bool) = match key.data_type {
                DataType::Bool => ("bool", "bval", false, true),
                DataType::Uint8 => ("uint8_t", "u8val", false, false),
                DataType::Int8 => ("int8_t", "s8val", false, false),
                DataType::Uint16 => ("uint16_t", "u16val", false, false),
                DataType::Int16 => ("int16_t", "s16val", false, false),
                DataType::Uint32 => ("uint32_t", "u32val", false, false),
                DataType::Int32 => ("int32_t", "s32val", false, false),
                DataType::String => ("const char *", "", true, false),
                DataType::Binary => ("const uint8_t *", "", true, false),
            };

            let type_code = key.data_type.type_code();
            let _encoding = KeyEncoding {
                namespace: c_ns_id,
                class: c_idx,
                id: key.key_index.unwrap_or(key_pos as u16),
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };

            let default_c = format_test_default(&key.default, key.data_type);
            let test_c = format_test_value(key.data_type, &default_c);

            entries.push(PersistenceTestEntry {
                define_name,
                c_type: c_type.to_string(),
                val_field: val_field.to_string(),
                is_string,
                is_bool,
                default_c,
                test_c,
            });
        }
    }

    entries
}

fn format_test_default(default: &Option<toml::Value>, data_type: DataType) -> String {
    match default {
        Some(toml::Value::Boolean(b)) => if *b { "true" } else { "false" }.to_string(),
        Some(toml::Value::Integer(i)) => i.to_string(),
        Some(toml::Value::String(s)) => format!("\"{}\"", s.replace('"', "\\\"")),
        _ => match data_type {
            DataType::Bool => "false".to_string(),
            DataType::String | DataType::Binary => "\"\"".to_string(),
            _ => "0".to_string(),
        },
    }
}

/// Generate a test value that's different from the default.
fn format_test_value(data_type: DataType, default_c: &str) -> String {
    match data_type {
        DataType::Bool => {
            if default_c == "true" {
                "false".to_string()
            } else {
                "true".to_string()
            }
        }
        DataType::String => "\"test_value\"".to_string(),
        DataType::Binary => "\"\\x01\\x02\"".to_string(),
        DataType::Uint8 | DataType::Uint16 | DataType::Uint32 => {
            if default_c == "42" {
                "99".to_string()
            } else {
                "42".to_string()
            }
        }
        DataType::Int8 | DataType::Int16 | DataType::Int32 => {
            if default_c == "-7" {
                "42".to_string()
            } else {
                "-7".to_string()
            }
        }
    }
}
