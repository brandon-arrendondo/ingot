pub mod storage;
pub mod target;
pub mod yaml_manifest;

use crate::model::key::KeyEncoding;
use crate::model::schema::{DataModel, DataType};
use serde::Serialize;
use std::path::Path;
use tera::{Context, Tera};

/// Opt-in codegen toggles. Every field defaults OFF and is purely additive to
/// the C99 core output, so the default (`CodegenOptions::default()`) reproduces
/// the byte-identical baseline consumers rely on.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodegenOptions {
    /// Disable event callback generation.
    pub no_events: bool,
    /// Emit C++/tinyfsm event structs + dispatch-by-key wrapper.
    pub emit_tinyfsm: bool,
    /// Emit the UDM-shaped compatibility surface (macros, accessors, bare-key
    /// constants, `dm_enums.h`).
    pub udm_compat: bool,
}

/// A key definition ready for template rendering.
#[derive(Debug, Serialize)]
pub struct KeyDefRenderable {
    pub namespace: String,
    pub class: String,
    pub name: String,
    pub define_name: String,
    /// The `DM_KEY_`-less uppercase path (e.g. "APPLIANCE_STATUS_MODE"), used
    /// for the `--udm-compat` bare-key `#define <PATH> DM_KEY_<PATH>` alias.
    pub bare_name: String,
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
    opts: CodegenOptions,
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
        ctx.insert("udm_compat", &opts.udm_compat);
        let rendered = tera.render("key_definitions.h", &ctx)?;
        std::fs::write(output_dir.join("key_definitions.h"), rendered)?;
        log::info!("Generated key_definitions.h ({} keys)", key_defs.len());
    }

    // Generate C++/tinyfsm event artifacts (opt-in, additive — see --emit-tinyfsm).
    // Off by default so C99-only consumers and their output are untouched.
    if opts.emit_tinyfsm {
        let emitted_events = generate_tinyfsm_events(&tera, version, &key_defs, ns_id, output_dir)?;
        // UDM-compat only (P2-20260707-006): also emit dm_key_events.h/.c, the
        // C-linkage `send_dm_key_event(uint32_t)` surface the dropped
        // gen/udm/dm_key_events.c used to provide. Only meaningful once there
        // is a dispatch table to forward to.
        if opts.udm_compat && emitted_events {
            generate_dm_key_events_alias(&tera, version, output_dir)?;
        }
    }

    // Generate jenkins_hash.h and jenkins_hash.c
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        let h = tera.render("jenkins_hash.h", &ctx)?;
        let c = tera.render("jenkins_hash.c", &ctx)?;
        std::fs::write(output_dir.join("jenkins_hash.h"), h)?;
        std::fs::write(output_dir.join("jenkins_hash.c"), c)?;
        log::info!("Generated jenkins_hash.h/.c");
    }

    // Generate dm_key.h
    //
    // Under --udm-compat, also emits `DATA_MODEL_KEY_TYPE_*` #define aliases
    // onto the `DM_KEY_TYPE_*` enumerators declared in this same file. The
    // recovered original gen/udm/dm_key.h declared `DATA_MODEL_KEY_TYPE` as
    // its own enum with identical values for the shared members (BOOLEAN=0,
    // UINT8=1, UINT16=2, UINT32=3, INT8=4, INT16=5, INT32=6, STRING=8);
    // libBissellIoT call sites (e.g. telem_broker_handler.c) use these names
    // as array-index-valid compile-time integer constants in static
    // initializers, so a #define alias onto the enumerator (not a runtime
    // value) is required (P2-20260707-014 Gap B).
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("udm_compat", &opts.udm_compat);
        let h = tera.render("dm_key.h", &ctx)?;
        std::fs::write(output_dir.join("dm_key.h"), h)?;
        log::info!("Generated dm_key.h");
    }

    // Generate dm_namespace_definitions.h
    {
        let namespaces = collect_namespaces(model, ns_id);
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("namespaces", &namespaces);
        let h = tera.render("dm_namespace_definitions.h", &ctx)?;
        std::fs::write(output_dir.join("dm_namespace_definitions.h"), h)?;
        log::info!("Generated dm_namespace_definitions.h");
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
        std::fs::write(output_dir.join("boolean_storage.h"), h)?;
        std::fs::write(output_dir.join("boolean_storage.c"), c)?;
        log::info!(
            "Generated boolean_storage.h/.c ({} keys, {} word(s))",
            bs.num_keys,
            bs.num_words
        );

        // --- Generate dm_boolean_storage.h alias (UDM-compat only) ---
        // libBissellIoT's telemetry.c, endpoint_handler.c, local_comm.c, and udm_comm.c
        // #include "dm_boolean_storage.h" — the filename the dropped gen/udm codegen used to
        // emit for BooleanStorage_SetKey/GetKey. Alias to the canonical boolean_storage.h
        // rather than hand-maintaining it in the lib tree (P2-20260707-014 Gap A).
        if opts.udm_compat {
            let mut ctx = Context::new();
            ctx.insert("version", version);
            let h = tera.render("dm_boolean_storage.h", &ctx)?;
            std::fs::write(output_dir.join("dm_boolean_storage.h"), h)?;
            log::info!("Generated dm_boolean_storage.h (UDM-compat alias for boolean_storage.h)");
        }
    }

    // Generate integer_storage.h/.c
    if !int_storages.is_empty() {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("types", &int_storages);
        let h = tera.render("integer_storage.h", &ctx)?;
        let c = tera.render("integer_storage.c", &ctx)?;
        std::fs::write(output_dir.join("integer_storage.h"), h)?;
        std::fs::write(output_dir.join("integer_storage.c"), c)?;
        log::info!(
            "Generated integer_storage.h/.c ({} type groups)",
            int_storages.len()
        );

        // --- Generate dm_integer_storage.h alias (UDM-compat only) ---
        // Same shape as the dm_boolean_storage.h alias above (P2-20260707-014): the dropped
        // gen/udm codegen emitted IntegerStorage_Set/GetUINT8Key/etc. under this filename;
        // ingot's own integer_storage.h exposes the identical signatures.
        if opts.udm_compat {
            let mut ctx = Context::new();
            ctx.insert("version", version);
            let h = tera.render("dm_integer_storage.h", &ctx)?;
            std::fs::write(output_dir.join("dm_integer_storage.h"), h)?;
            log::info!("Generated dm_integer_storage.h (UDM-compat alias for integer_storage.h)");
        }
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
        std::fs::write(output_dir.join("string_storage.h"), h)?;
        std::fs::write(output_dir.join("string_storage.c"), c)?;
        log::info!(
            "Generated string_storage.h/.c ({} total, {} RO, {} RW)",
            ss.total_keys,
            ss.ro.as_ref().map_or(0, |g| g.num_keys),
            ss.rw.as_ref().map_or(0, |g| g.num_keys),
        );

        // --- Generate dm_string_storage.h alias (UDM-compat only) ---
        // Same shape as the dm_boolean_storage.h/dm_integer_storage.h aliases above
        // (P2-20260707-014): telemetry.c/endpoint_handler.c/local_comm.c #include this filename
        // without calling any StringStorage_* function directly.
        if opts.udm_compat {
            let mut ctx = Context::new();
            ctx.insert("version", version);
            let h = tera.render("dm_string_storage.h", &ctx)?;
            std::fs::write(output_dir.join("dm_string_storage.h"), h)?;
            log::info!("Generated dm_string_storage.h (UDM-compat alias for string_storage.h)");
        }
    }

    // Generate persistence_storage.h/.c
    if let Some(ref ps) = persist_storage {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("persistence", ps);
        let h = tera.render("persistence_storage.h", &ctx)?;
        let c = tera.render("persistence_storage.c", &ctx)?;
        std::fs::write(output_dir.join("persistence_storage.h"), h)?;
        std::fs::write(output_dir.join("persistence_storage.c"), c)?;
        log::info!("Generated persistence_storage.h/.c ({} keys)", ps.num_keys);

        // --- Generate dm_persistence_storage.h alias (UDM-compat only) ---
        // Same shape as the other dm_*_storage.h aliases above (P2-20260707-014):
        // endpoint_handler.c/local_comm.c #include this filename without calling any
        // PersistenceStorage_*/DataModel_*PersistentKeys function directly.
        if opts.udm_compat {
            let mut ctx = Context::new();
            ctx.insert("version", version);
            let h = tera.render("dm_persistence_storage.h", &ctx)?;
            std::fs::write(output_dir.join("dm_persistence_storage.h"), h)?;
            log::info!(
                "Generated dm_persistence_storage.h (UDM-compat alias for persistence_storage.h)"
            );
        }
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
        ctx.insert("no_events", &opts.no_events);

        let h = tera.render("dm.h", &ctx)?;
        let c = tera.render("dm.c", &ctx)?;
        std::fs::write(output_dir.join("dm.h"), h)?;
        std::fs::write(output_dir.join("dm.c"), c)?;
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
            let mut ctx = Context::new();
            ctx.insert("version", version);
            ctx.insert("helpers", &helpers);
            ctx.insert("has_string_helpers", &has_string_helpers);
            ctx.insert("udm_compat", &opts.udm_compat);
            let h = tera.render("dm_helpers.h", &ctx)?;
            std::fs::write(output_dir.join("dm_helpers.h"), h)?;
            if has_string_helpers {
                let c = tera.render("dm_helpers.c", &ctx)?;
                std::fs::write(output_dir.join("dm_helpers.c"), c)?;
            }
            log::info!(
                "Generated dm_helpers.h{} ({} helpers)",
                if has_string_helpers { "/.c" } else { "" },
                helpers.len()
            );
        }
    }

    // --- Generate dm_enums.h (UDM-compat only) ---
    // Emits <PATH>_ENUM_T typedefs + named constants for every key that
    // references an [enums.*] domain. Off by default so the shim output is
    // untouched; nothing is written when no key carries an enum.
    if opts.udm_compat {
        let enum_types = collect_enum_types(model, ns_id);
        if !enum_types.is_empty() {
            let mut ctx = Context::new();
            ctx.insert("version", version);
            ctx.insert("enums", &enum_types);
            let h = tera.render("dm_enums.h", &ctx)?;
            std::fs::write(output_dir.join("dm_enums.h"), h)?;
            log::info!("Generated dm_enums.h ({} enum types)", enum_types.len());
        } else {
            log::info!("No enum-typed keys — skipping dm_enums.h");
        }
    }

    // --- Generate dm_key_definitions.h alias (UDM-compat only) ---
    // libBissellIoT's ~1033 DATAMODEL_SET/GET call sites #include "dm_key_definitions.h" — the
    // filename its OWN dropped gen/udm codegen used to emit for the bare-key #defines. Rather
    // than hand-maintain that filename in the lib tree (drift risk), emit a one-line alias
    // header here that #includes the canonical key_definitions.h, so those call sites compile
    // unchanged against the shared store (P2-20260707-006).
    if opts.udm_compat {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        let h = tera.render("dm_key_definitions.h", &ctx)?;
        std::fs::write(output_dir.join("dm_key_definitions.h"), h)?;
        log::info!("Generated dm_key_definitions.h (UDM-compat alias for key_definitions.h)");
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
        ctx.insert("no_events", &opts.no_events);
        ctx.insert("has_persistence", &has_persistence);
        ctx.insert("persistence_entries", &persist_test_entries);
        let test_c = tera.render("test_dm.c", &ctx)?;
        std::fs::write(output_dir.join("test_dm.c"), test_c)?;
        let cmake = tera.render("CMakeLists.txt", &ctx)?;
        std::fs::write(output_dir.join("CMakeLists.txt"), cmake)?;
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
/// Returns `true` when the dispatch table was actually emitted (i.e. the
/// model has at least one event key), so callers gating a downstream,
/// dispatch-table-dependent emission (`--udm-compat`'s `dm_key_events.h`) know
/// whether there is anything to forward to.
fn generate_tinyfsm_events(
    tera: &Tera,
    version: &str,
    key_defs: &[KeyDefRenderable],
    ns_id: u16,
    output_dir: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (events, groups) = collect_event_keys(key_defs);
    if events.is_empty() {
        log::info!("No event keys — skipping tinyfsm event generation");
        return Ok(false);
    }

    let mut hpp_ctx = Context::new();
    hpp_ctx.insert("version", version);
    hpp_ctx.insert("groups", &groups);
    let hpp = tera.render("dm_key_events.hpp", &hpp_ctx)?;
    std::fs::write(output_dir.join("dm_key_events.hpp"), hpp)?;

    let mut wrap_ctx = Context::new();
    wrap_ctx.insert("version", version);
    wrap_ctx.insert("events", &events);
    wrap_ctx.insert("ns_id", &ns_id);
    let wrapper_h = tera.render("dm_key_events_wrapper.hpp", &wrap_ctx)?;
    let wrapper_c = tera.render("dm_key_events_wrapper.cpp", &wrap_ctx)?;
    std::fs::write(output_dir.join("dm_key_events_wrapper.hpp"), wrapper_h)?;
    std::fs::write(output_dir.join("dm_key_events_wrapper.cpp"), wrapper_c)?;

    // The has-event predicate lets a consumer's value-change callback ask, cheaply, whether
    // a changed key is one anything can react to. Without it `event = false` is true of the
    // generated TYPES (no FSM_EVENT_ struct) but silently false of the runtime TRAFFIC: the
    // callback fires per changed key and has to enqueue every one of them. See the header's
    // own comment for the measured cost of that gap on the B12 robot.
    let has_event = tera.render("dm_key_has_event.hpp", &wrap_ctx)?;
    std::fs::write(output_dir.join("dm_key_has_event.hpp"), has_event)?;

    log::info!(
        "Generated dm_key_events.hpp + dm_key_events_wrapper.hpp/.cpp + dm_key_has_event.hpp ({} event keys)",
        events.len()
    );
    Ok(true)
}

/// Render the UDM-compat `dm_key_events.h`/`.c` alias (P2-20260707-006).
///
/// Behavior-identical replacement for the dropped `gen/udm/dm_key_events.h`/
/// `.c`: declares/defines `send_dm_key_event(uint32_t)` with C linkage,
/// forwarding to the ingot-native `send_tinyfsm_event_by_key` dispatch table.
/// Only called when that dispatch table was actually emitted.
fn generate_dm_key_events_alias(
    tera: &Tera,
    version: &str,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = Context::new();
    ctx.insert("version", version);
    let h = tera.render("dm_key_events.h", &ctx)?;
    let c = tera.render("dm_key_events.c", &ctx)?;
    std::fs::write(output_dir.join("dm_key_events.h"), h)?;
    std::fs::write(output_dir.join("dm_key_events.c"), c)?;
    log::info!("Generated dm_key_events.h/.c (UDM-compat send_dm_key_event alias)");
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
fn resolve_ns_id(class: &crate::model::schema::Class, fallback: u16) -> u16 {
    class.namespace_id.unwrap_or(fallback)
}

/// Resolve the namespace name for a class, using per-class override or model-level fallback.
fn resolve_ns_name(class: &crate::model::schema::Class, model: &DataModel) -> String {
    class
        .namespace_name
        .as_deref()
        .unwrap_or(&model.meta.id)
        .to_string()
}

/// Resolve the class index, using explicit class_index or positional fallback.
fn resolve_class_idx(class: &crate::model::schema::Class, position: usize) -> u8 {
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
            let bare_name = format!("{c_ns_name}_{class_name}_{key_name}");

            defs.push(KeyDefRenderable {
                namespace: resolve_ns_name(class, model),
                class: class.id.clone(),
                name: key.id.clone(),
                define_name: format!("DM_KEY_{bare_name}"),
                bare_name,
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
    /// Helper function suffix, uppercased path (e.g. "BATTERY_STATUS_VOLTAGE")
    helper_name: String,
    /// Original-case path (e.g. "battery_status_voltage" / "product_config_prodSerial"),
    /// used for the `--udm-compat` lowercase `DataModel_{Get,Set}<path>` symbol alias.
    orig_path: String,
    /// C type for the value (e.g. "uint16_t", "bool")
    c_type: String,
    /// dm_val_t union field (e.g. "u16val", "bval") — empty for strings
    val_field: String,
    /// True for string-type keys
    is_string: bool,
    /// True for read-only keys
    is_read_only: bool,
    /// `Some("<PATH>_ENUM_T")` when this key has an `enum_ref` — original UDM
    /// emitted `DATAMODEL_GET_<PATH>` as a real `static inline <ENUM_TYPE>`
    /// wrapper casting the native getter's raw storage-typed return to the
    /// enum type (see `~/.local/share/Trash/files/gen/udm/dm_helpers.h:2226`,
    /// cited in P2-20260707-013), not the bare macro alias used for non-enum
    /// keys. `None` for keys without an enum, which keep the P2-010 macro
    /// shape. The SET side is unaffected: original UDM's enum setters took
    /// the raw storage type (uint8_t), same as the existing macro alias.
    enum_type_name: Option<String>,
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

            // Original-case path for the UDM-compat lowercase accessor alias.
            // The UDM catalog authors a lowercase/concatenated `id` plus a
            // mixed-case display `name`; the lowercase accessor uses the display
            // name (class + key), falling back to `id` when no name is given, so
            // camelCase segments (awsJob, prodSerial, versionNumber) survive and
            // the alias matches the UDM symbol libBissellIoT links against, e.g.
            // DataModel_Getlocal_config_awsJob_id. The uppercase C surface is
            // derived independently via id.to_uppercase(), so it is unaffected.
            let ns_orig = resolve_ns_name(class, model);
            let class_orig = class.name.as_deref().unwrap_or(&class.id).replace(' ', "_");
            let key_orig = key.name.as_deref().unwrap_or(&key.id).replace(' ', "_");
            let orig_path = format!("{ns_orig}_{class_orig}_{key_orig}");

            // Mirrors collect_enum_types' `prefix`/`type_name` computation exactly
            // (same c_ns_name/class_name/key_name inputs) so the two never drift.
            let enum_type_name = key
                .enum_ref
                .as_ref()
                .filter(|enum_ref| model.enums.contains_key(*enum_ref))
                .map(|_| format!("{helper_name}_ENUM_T"));

            let (c_type, val_field, is_string) = match key.data_type {
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

            helpers.push(HelperEntry {
                define_name,
                helper_name,
                orig_path,
                c_type,
                val_field,
                is_string,
                is_read_only: key.read_only,
                enum_type_name,
            });
        }
    }

    helpers
}

/// A single named enum constant ready for template rendering.
#[derive(Debug, Serialize)]
struct EnumConstRenderable {
    /// Fully-qualified constant name (e.g. "APPLIANCE_STATUS_MODE_OFF").
    name: String,
    /// The declared integer value (wire-semantic — must equal the schema's).
    value: i64,
}

/// A `<PATH>_ENUM_T` typedef ready for template rendering.
#[derive(Debug, Serialize)]
struct EnumTypeRenderable {
    /// The typedef name (e.g. "APPLIANCE_STATUS_MODE_ENUM_T").
    type_name: String,
    /// The uppercase key path prefix (e.g. "APPLIANCE_STATUS_MODE"), used for
    /// the trailing `_MAX_STORAGE_VALUE` sentinel constant.
    prefix: String,
    /// The stdint sentinel bound for this key's storage width (e.g. "UINT8_MAX").
    max_macro: String,
    /// Named constants, ordered by value then name for deterministic output.
    values: Vec<EnumConstRenderable>,
}

/// Map a key's storage type to the stdint `*_MAX` sentinel used for the
/// generated enum's `_MAX_STORAGE_VALUE` guard (mirrors the UDM emitter).
fn enum_max_macro(data_type: DataType) -> &'static str {
    match data_type {
        DataType::Bool | DataType::Uint8 => "UINT8_MAX",
        DataType::Int8 => "INT8_MAX",
        DataType::Uint16 => "UINT16_MAX",
        DataType::Int16 => "INT16_MAX",
        DataType::Uint32 => "UINT32_MAX",
        DataType::Int32 => "INT32_MAX",
        // Enums never attach to string/binary keys; guard defensively.
        DataType::String | DataType::Binary => "UINT8_MAX",
    }
}

/// Collect one `<PATH>_ENUM_T` typedef per enum-referencing key.
///
/// The typedef and its constants are named after the *key's* path (not the
/// enum domain), matching the UDM generator: the same `[enums.*]` domain
/// referenced by two keys yields two distinct typedefs. Constant values are
/// emitted verbatim from the schema (Data Quality — they are wire-semantic).
fn collect_enum_types(model: &DataModel, ns_id: u16) -> Vec<EnumTypeRenderable> {
    let mut out = Vec::new();

    for (pos, class) in model.classes.iter().enumerate() {
        let c_ns_name = resolve_ns_name(class, model).to_uppercase();
        let class_name = class.id.to_uppercase();
        let _ = resolve_ns_id(class, ns_id);
        let _ = resolve_class_idx(class, pos);

        for key in &class.keys {
            let Some(enum_ref) = key.enum_ref.as_ref() else {
                continue;
            };
            let Some(enum_def) = model.enums.get(enum_ref) else {
                continue;
            };

            let key_name = key.id.to_uppercase().replace(' ', "_");
            let prefix = format!("{c_ns_name}_{class_name}_{key_name}");

            let mut values: Vec<EnumConstRenderable> = enum_def
                .values
                .iter()
                .map(|(name, &value)| EnumConstRenderable {
                    name: format!("{prefix}_{}", name.to_uppercase().replace(' ', "_")),
                    value,
                })
                .collect();
            values.sort_by(|a, b| a.value.cmp(&b.value).then(a.name.cmp(&b.name)));

            out.push(EnumTypeRenderable {
                type_name: format!("{prefix}_ENUM_T"),
                max_macro: enum_max_macro(key.data_type).to_string(),
                prefix,
                values,
            });
        }
    }

    out
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
