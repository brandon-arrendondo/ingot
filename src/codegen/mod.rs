pub mod enums;
pub mod rust_kvp;
pub mod storage;
pub mod tables;
pub mod target;
pub mod templates;
pub mod yaml_manifest;

use crate::model::key::{define_stem, key_id};
use crate::model::schema::{Class, DataModel, DataType, EnumDef, KeyDef};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use tera::{Context, Tera};

/// Write generated text to `path`, normalised to end in exactly one newline.
///
/// @version 1
/// @req REQ-ING-GEN-001
pub(crate) fn write_generated(path: impl AsRef<Path>, content: String) -> std::io::Result<()> {
    let normalized = format!("{}\n", content.trim_end_matches('\n'));
    std::fs::write(path, normalized)
}

/// Opt-in codegen toggles. Every field defaults off and is purely additive to
/// the C99 core output, so `CodegenOptions::default()` reproduces the baseline
/// output byte for byte.
#[derive(Debug, Clone, Default)]
pub struct CodegenOptions {
    /// Disable event callback generation.
    pub no_events: bool,
    /// Emit C++/tinyfsm event structs + dispatch-by-key wrapper.
    pub emit_tinyfsm: bool,
    /// Emit the DM-shaped compatibility surface (macros, accessors, bare-key
    /// constants, per-key enum typedefs, legacy header aliases).
    pub dm_compat: bool,
    /// Include guard of a legacy `dm_enums.h` that already declares the
    /// per-key enum typedefs. When set, the DM-compat enum block is skipped
    /// in any translation unit that has already included that header.
    pub dm_enums_guard: Option<String>,
}

/// Join values as `a, b, c`, the element list of a generated C or Rust array initialiser.
///
/// @version 1
/// @req REQ-ING-GEN-004
/// @req REQ-ING-GEN-005
/// @req REQ-ING-GEN-006
/// @req REQ-ING-RST-002
pub(crate) fn join_list<T: std::fmt::Display>(values: &[T]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A key definition ready for template rendering.
#[derive(Debug, Serialize)]
pub struct KeyDefRenderable {
    pub namespace: String,
    pub class: String,
    pub name: String,
    pub define_name: String,
    /// The `DM_KEY_`-less uppercase path (e.g. "APPLIANCE_STATUS_MODE"), used
    /// for the `--dm-compat` bare-key `#define <PATH> DM_KEY_<PATH>` alias.
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
/// @version 1
/// @req REQ-ING-GEN-011
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

/// Render each named template with `ctx` into a same-named file in `output_dir`.
///
/// @version 1
/// @req REQ-ING-GEN-001
/// @return Ok once every file is written, else the first render or write error
fn render_files(
    tera: &Tera,
    ctx: &Context,
    output_dir: &Path,
    names: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    for name in names {
        write_generated(output_dir.join(name), tera.render(name, ctx)?)?;
    }
    Ok(())
}

/// A context holding only the generator version, shared by every template.
///
/// @version 1
/// @req REQ-ING-GEN-001
/// @return a Tera context with `version` set
fn version_context(version: &str) -> Context {
    let mut ctx = Context::new();
    ctx.insert("version", version);
    ctx
}

/// Generate all C code from a parsed data model.
///
/// v2: the Unity test collectors no longer take the namespace ID they did not use.
/// v3: neither does the helper collector.
/// v4: renders with the templates the caller loaded (embedded unless --templates).
/// @version 4
/// @req REQ-ING-GEN-001
pub fn generate(
    model: &DataModel,
    ns_id: u16,
    output_dir: &Path,
    tera: &Tera,
    target: &target::TargetConfig,
    opts: CodegenOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;

    let version = env!("CARGO_PKG_VERSION");

    // Collect all key definitions
    let key_defs = collect_key_definitions(model, ns_id);

    // The compat surface adds bare-key macros and per-key enum constants to
    // the C namespace; refuse to emit headers whose names would clash.
    if opts.dm_compat {
        check_compat_identifiers(model, &key_defs)?;
    }

    // Generate key_definitions.h
    {
        let mut ctx = version_context(version);
        ctx.insert("keys", &key_defs);
        ctx.insert("dm_compat", &opts.dm_compat);
        render_files(tera, &ctx, output_dir, &["key_definitions.h"])?;
        log::info!("Generated key_definitions.h ({} keys)", key_defs.len());
    }

    // Generate C++/tinyfsm event artifacts (opt-in, additive — see --emit-tinyfsm).
    // Off by default so C99-only consumers and their output are untouched.
    generate_event_artifacts(tera, version, &key_defs, ns_id, &opts, output_dir)?;

    // Generate jenkins_hash.h and jenkins_hash.c
    {
        let ctx = version_context(version);
        render_files(
            tera,
            &ctx,
            output_dir,
            &["jenkins_hash.h", "jenkins_hash.c"],
        )?;
        log::info!("Generated jenkins_hash.h/.c");
    }

    // Generate dm_key.h (under --dm-compat, with the legacy
    // DATA_MODEL_KEY_TYPE_* compile-time aliases onto DM_KEY_TYPE_*).
    {
        let mut ctx = version_context(version);
        ctx.insert("dm_compat", &opts.dm_compat);
        render_files(tera, &ctx, output_dir, &["dm_key.h"])?;
        log::info!("Generated dm_key.h");
    }

    // Generate dm_namespace_definitions.h
    {
        let namespaces = collect_namespaces(model, ns_id);
        let mut ctx = version_context(version);
        ctx.insert("namespaces", &namespaces);
        render_files(tera, &ctx, output_dir, &["dm_namespace_definitions.h"])?;
        log::info!("Generated dm_namespace_definitions.h");
    }

    // Generate dm_enums.h (only when the model defines named enums). Under
    // --dm-compat it also carries one <PATH>_ENUM_T typedef per enum-typed
    // key; such a key always references a named enum, so the file exists
    // whenever there is a compat typedef to put in it.
    let enum_defs = enums::collect_enums(model);
    if !enum_defs.is_empty() {
        let mut ctx = version_context(version);
        ctx.insert("enums", &enum_defs);
        ctx.insert("compat_enums", &compat_enum_types(model, &opts));
        ctx.insert("compat_guard", &opts.dm_enums_guard);
        render_files(tera, &ctx, output_dir, &["dm_enums.h"])?;
        log::info!("Generated dm_enums.h ({} enum(s))", enum_defs.len());
    }

    // Generate dm_full.yaml manifest
    yaml_manifest::generate_yaml_manifest(model, ns_id, output_dir)?;

    // --- Collect all storage data ---
    let bool_storage = storage::boolean::collect_boolean_storage(model, ns_id)?;
    let int_storages = storage::integer::collect_integer_storage(model, ns_id)?;
    let str_storage = storage::string::collect_string_storage(model, ns_id)?;
    let persist_storage = storage::persistence::collect_persistence_storage(model);

    // Generate boolean_storage.h/.c
    if let Some(ref bs) = bool_storage {
        let mut ctx = version_context(version);
        ctx.insert("bool", bs);
        render_files(
            tera,
            &ctx,
            output_dir,
            &["boolean_storage.h", "boolean_storage.c"],
        )?;
        log::info!(
            "Generated boolean_storage.h/.c ({} keys, {} word(s))",
            bs.num_keys,
            bs.num_words
        );
    }

    // Generate integer_storage.h/.c
    if !int_storages.is_empty() {
        let mut ctx = version_context(version);
        ctx.insert("types", &int_storages);
        render_files(
            tera,
            &ctx,
            output_dir,
            &["integer_storage.h", "integer_storage.c"],
        )?;
        log::info!(
            "Generated integer_storage.h/.c ({} type groups)",
            int_storages.len()
        );
    }

    // Generate string_storage.h/.c
    if let Some(ref ss) = str_storage {
        let mut ctx = version_context(version);
        ctx.insert("total_keys", &ss.total_keys);
        ctx.insert("ro", &ss.ro);
        ctx.insert("rw", &ss.rw);
        render_files(
            tera,
            &ctx,
            output_dir,
            &["string_storage.h", "string_storage.c"],
        )?;
        log::info!(
            "Generated string_storage.h/.c ({} total, {} RO, {} RW)",
            ss.total_keys,
            ss.ro.as_ref().map_or(0, |g| g.num_keys),
            ss.rw.as_ref().map_or(0, |g| g.num_keys),
        );
    }

    // Generate persistence_storage.h/.c
    if let Some(ref ps) = persist_storage {
        let mut ctx = version_context(version);
        ctx.insert("persistence", ps);
        ctx.insert("dm_compat", &opts.dm_compat);
        let names = ["persistence_storage.h", "persistence_storage.c"];
        render_files(tera, &ctx, output_dir, &names)?;
        log::info!("Generated persistence_storage.h/.c ({} keys)", ps.num_keys);
    }

    // --- Generate the DM-compat header-name aliases ---
    let present = [
        bool_storage.is_some(),
        !int_storages.is_empty(),
        str_storage.is_some(),
        persist_storage.is_some(),
    ];
    generate_dm_compat_aliases(tera, version, &opts, present, output_dir)?;

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

        let mut ctx = version_context(version);
        ctx.insert("has_bool", &has_bool);
        ctx.insert("has_integers", &has_integers);
        ctx.insert("has_ro_strings", &has_ro_strings);
        ctx.insert("has_rw_strings", &has_rw_strings);
        ctx.insert("has_persistence", &has_persistence);
        ctx.insert("int_types", &api_int_types);
        ctx.insert("target", target);
        ctx.insert("no_events", &opts.no_events);

        render_files(tera, &ctx, output_dir, &["dm.h", "dm.c"])?;
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
        let helpers = collect_helpers(model);
        if !helpers.is_empty() {
            let has_string_helpers = helpers.iter().any(|h| h.is_string);
            let has_enum_helpers = helpers.iter().any(|h| h.is_enum);
            let mut ctx = version_context(version);
            ctx.insert("helpers", &helpers);
            ctx.insert("has_string_helpers", &has_string_helpers);
            ctx.insert("has_enum_helpers", &has_enum_helpers);
            ctx.insert("dm_compat", &opts.dm_compat);
            let names: &[&str] = if has_string_helpers {
                &["dm_helpers.h", "dm_helpers.c"]
            } else {
                &["dm_helpers.h"]
            };
            render_files(tera, &ctx, output_dir, names)?;
            log::info!(
                "Generated dm_helpers.h{} ({} helpers)",
                if has_string_helpers { "/.c" } else { "" },
                helpers.len()
            );
        }
    }

    render_unity_tests(
        tera,
        model,
        output_dir,
        opts.no_events,
        persist_storage.as_ref(),
    )
}

/// Render the Unity test file and its CMakeLists.txt.
///
/// @version 1
/// @req REQ-ING-GEN-009
/// @return Ok once both files are written, else the first render or write error
fn render_unity_tests(
    tera: &Tera,
    model: &DataModel,
    output_dir: &Path,
    no_events: bool,
    persist_storage: Option<&storage::persistence::PersistenceStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    let test_keys = collect_test_keys(model);
    let persist_test_entries = collect_persistence_test_entries(model);
    let mut ctx = version_context(env!("CARGO_PKG_VERSION"));
    ctx.insert("keys", &test_keys);
    ctx.insert("namespace", &model.meta.id);
    ctx.insert("no_events", &no_events);
    ctx.insert("has_persistence", &persist_storage.is_some());
    ctx.insert("persistence_entries", &persist_test_entries);
    // The stored image's layout, so a test can write a malformed file.
    let has_persistent_string = persist_storage
        .is_some_and(|ps| ps.entries.iter().any(|e| e.is_string && e.c_type == "char"));
    ctx.insert("has_persistent_string", &has_persistent_string);
    if let Some(ps) = persist_storage {
        ctx.insert("persistence", ps);
    }
    render_files(tera, &ctx, output_dir, &["test_dm.c", "CMakeLists.txt"])?;
    log::info!(
        "Generated test_dm.c + CMakeLists.txt ({} test keys)",
        test_keys.len()
    );
    Ok(())
}

/// Emit the opt-in event artifacts: the tinyfsm headers under
/// `--emit-tinyfsm`, plus the DM-compat `dm_key_events.h/.c` alias when
/// `--dm-compat` is also set and there is a dispatch table to forward to.
///
/// @version 1
/// @req REQ-ING-EVT-001
/// @return Ok once the selected files are written, else the first render or write error
fn generate_event_artifacts(
    tera: &Tera,
    version: &str,
    key_defs: &[KeyDefRenderable],
    ns_id: u16,
    opts: &CodegenOptions,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !opts.emit_tinyfsm {
        return Ok(());
    }
    let emitted = generate_tinyfsm_events(tera, version, key_defs, ns_id, output_dir)?;
    if opts.dm_compat && emitted {
        let ctx = version_context(version);
        render_files(
            tera,
            &ctx,
            output_dir,
            &["dm_key_events.h", "dm_key_events.c"],
        )?;
        log::info!("Generated dm_key_events.h/.c (DM-compat send_dm_key_event alias)");
    }
    Ok(())
}

/// Templates rendered from the flat list of event keys, each written under its own name.
///
/// They share one context, so every file lists exactly the same keys in the same order.
const EVENT_KEY_TEMPLATES: [&str; 4] = [
    "dm_key_events_wrapper.hpp",
    "dm_key_events_wrapper.cpp",
    "dm_key_has_event.hpp",
    "dm_key_event_list.hpp",
];

/// Render the template `name` and write the result to `output_dir/name`.
///
/// v2: crate-visible, so the table-interface outputs render through it too.
/// @version 2
/// @req REQ-ING-GEN-001
/// @req REQ-ING-TIV-001
pub(crate) fn render_template(
    tera: &Tera,
    name: &str,
    ctx: &Context,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let rendered = tera.render(name, ctx)?;
    write_generated(output_dir.join(name), rendered)?;
    Ok(())
}

/// Render the C++/tinyfsm event header, the dispatch-by-key wrapper, the
/// has-event predicate and the (key id, event) typelist.
///
/// Emits nothing when no key carries `event = true` — a consumer with no
/// event keys gets no C++ artifacts even with the flag on. Returns `true`
/// when the dispatch table was emitted, so a caller emitting something that
/// forwards to it knows whether there is anything to forward to.
///
/// v2: renders EVENT_KEY_TEMPLATES from one context, with the model's namespace id.
/// @version 2
/// @req REQ-ING-EVT-001
/// @req REQ-ING-EVT-002
/// @req REQ-ING-EVT-003
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

    let mut hpp_ctx = version_context(version);
    hpp_ctx.insert("groups", &groups);
    render_files(tera, &hpp_ctx, output_dir, &["dm_key_events.hpp"])?;

    let mut events_ctx = version_context(version);
    events_ctx.insert("events", &events);
    events_ctx.insert("ns_id", &ns_id);
    render_files(tera, &events_ctx, output_dir, &EVENT_KEY_TEMPLATES)?;

    log::info!(
        "Generated dm_key_events.hpp + {} ({} event keys)",
        EVENT_KEY_TEMPLATES.join(" + "),
        events.len()
    );
    Ok(true)
}

/// Emit the DM-compat headers that only repoint a legacy DM filename to
/// ingot's own header. `present` flags whether boolean, integer, string, and
/// persistence storage were generated; each storage alias follows its header.
///
/// @version 1
/// @req REQ-ING-CMP-001
/// @return Ok once the aliases are written (none without --dm-compat), else the first error
fn generate_dm_compat_aliases(
    tera: &Tera,
    version: &str,
    opts: &CodegenOptions,
    present: [bool; 4],
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !opts.dm_compat {
        return Ok(());
    }
    let storage_aliases = [
        "dm_boolean_storage.h",
        "dm_integer_storage.h",
        "dm_string_storage.h",
        "dm_persistence_storage.h",
    ];
    let names: Vec<&str> = std::iter::once("dm_key_definitions.h")
        .chain(
            storage_aliases
                .iter()
                .zip(present)
                .filter_map(|(name, emitted)| emitted.then_some(*name)),
        )
        .collect();
    render_files(tera, &version_context(version), output_dir, &names)?;
    log::info!("Generated DM-compat header aliases: {}", names.join(", "));
    Ok(())
}

/// Build the flat dispatch list + (namespace, class)-grouped struct list for
/// the `event = true` keys.
///
/// The FSM event struct name mirrors the key #define with the `DM_KEY_`
/// prefix swapped for `FSM_EVENT_`, matching the DM generator's naming so
/// ingot is a drop-in replacement.
/// @version 1
/// @req REQ-ING-EVT-001
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
/// @version 1
/// @req REQ-ING-GEN-003
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

/// Resolve the namespace name for a class, using per-class override or model-level fallback.
/// @version 1
/// @req REQ-ING-KEY-002
pub(crate) fn resolve_ns_name(class: &crate::model::schema::Class, model: &DataModel) -> String {
    class
        .namespace_name
        .as_deref()
        .unwrap_or(&model.meta.id)
        .to_string()
}

/// `<NAMESPACE>_<CLASS>_<KEY>` of `key` in `class`; the key's define is `DM_KEY_` + this.
///
/// @version 1
/// @req REQ-ING-GEN-002
pub(crate) fn key_stem(
    class: &crate::model::schema::Class,
    model: &DataModel,
    key: &KeyDef,
) -> String {
    define_stem(&resolve_ns_name(class, model), &class.id, &key.id)
}

/// Keys paired with their encoded 32-bit IDs.
pub(crate) type EncodedKeys<'m> = Vec<(u32, &'m KeyDef)>;

/// Every key of the model that `wanted` accepts, with its encoded ID, in declaration order.
///
/// v2: IDs from the shared model::key::key_id (positions stamped at parse time).
/// @version 2
/// @req REQ-ING-KEY-002
/// @req REQ-ING-GEN-004
/// @req REQ-ING-GEN-005
/// @req REQ-ING-GEN-006
pub(crate) fn encoded_keys(
    model: &DataModel,
    ns_id: u16,
    wanted: impl Fn(&KeyDef) -> bool,
) -> EncodedKeys<'_> {
    let mut keys = Vec::new();
    for class in &model.classes {
        for key in class.keys.iter().filter(|k| wanted(k)) {
            keys.push((key_id(class, ns_id, key), key));
        }
    }
    keys
}

/// Build renderable key definitions from the model.
///
/// v2: the key ID comes from the shared encode_key.
/// v3: the define name comes from the shared key_stem.
/// v4: the key ID comes from model::key::key_id.
/// v5: carries the bare `<PATH>` name for the --dm-compat bare-key alias.
/// @version 5
/// @req REQ-ING-GEN-002
fn collect_key_definitions(model: &DataModel, ns_id: u16) -> Vec<KeyDefRenderable> {
    let mut defs = Vec::new();

    for class in &model.classes {
        for key in &class.keys {
            let encoded = key_id(class, ns_id, key);
            let bare_name = key_stem(class, model, key);

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
    /// Original-case path (e.g. "battery_status_voltage" / "local_config_deviceName"),
    /// used for the `--dm-compat` lowercase `DataModel_{Get,Set}<path>` symbol alias.
    orig_path: String,
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
    /// `Some("<PATH>_ENUM_T")` for an enum-typed key. Under `--dm-compat`,
    /// `DATAMODEL_GET_<PATH>` is then a `static inline` wrapper returning that
    /// per-key typedef, because C++ will not implicitly convert between enum
    /// types. Must match the names `collect_enum_types` emits.
    enum_type_name: Option<String>,
}

/// Original-case `<ns>_<class>_<key>` path for the DM-compat lowercase
/// accessor alias.
///
/// A DM catalog authors a lowercase `id` plus a mixed-case display `name`;
/// the alias uses the display names, falling back to `id`, so camelCase
/// segments survive (e.g. `DataModel_Getlocal_config_deviceName`). The
/// uppercase C surface derives from `id` independently.
///
/// @version 1
/// @req REQ-ING-CMP-001
/// @return the `<ns>_<class name>_<key name>` path, spaces as underscores
fn display_path(model: &DataModel, class: &Class, key: &KeyDef) -> String {
    let class_name = class.name.as_deref().unwrap_or(&class.id).replace(' ', "_");
    let key_name = key.name.as_deref().unwrap_or(&key.id).replace(' ', "_");
    format!("{}_{class_name}_{key_name}", resolve_ns_name(class, model))
}

/// Collect helper entries for keys with helpers=true.
///
/// v2: an unused KeyEncoding (and the namespace ID it needed) is no longer built.
/// v3: C type, dm_val_t field and buffer flag come from DataType.
/// v4: the define and helper names come from the shared key_stem.
/// v5: carries the display path and the per-key enum type for the --dm-compat aliases.
/// @version 5
/// @req REQ-ING-GEN-008
fn collect_helpers(model: &DataModel) -> Vec<HelperEntry> {
    let mut helpers = Vec::new();

    for class in &model.classes {
        for key in &class.keys {
            if !key.helpers {
                continue;
            }

            let helper_name = key_stem(class, model, key);
            let define_name = format!("DM_KEY_{helper_name}");

            let raw_c_type = key.data_type.c_value_type().to_string();
            let is_string = key.data_type.is_buffer();

            let is_enum = !is_string && key.enum_ref.is_some();
            let c_type = match (&key.enum_ref, is_enum) {
                (Some(enum_ref), true) => enums::enum_type_name(enum_ref, &model.meta.id),
                _ => raw_c_type.clone(),
            };
            let enum_type_name = is_enum.then(|| format!("{helper_name}_ENUM_T"));

            helpers.push(HelperEntry {
                define_name,
                orig_path: display_path(model, class, key),
                helper_name,
                c_type,
                val_field: key.data_type.val_field().to_string(),
                is_string,
                is_read_only: key.read_only,
                is_enum,
                storage_c_type: raw_c_type,
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
    /// `Some("<STEM>_T")` when the referenced named enum is itself called
    /// `<class>_<key>`: its members already carry exactly these constant
    /// names and values, so the compat type is a typedef of the named enum
    /// rather than a second declaration of the same enumerators.
    alias_of: Option<String>,
}

/// Map a key's storage type to the stdint `*_MAX` sentinel used for the
/// generated enum's `_MAX_STORAGE_VALUE` guard (mirrors the DM emitter).
///
/// @version 1
/// @req REQ-ING-CMP-001
/// @return the stdint `*_MAX` macro name
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

/// The per-key compat enum typedefs, or none unless `--dm-compat` is set.
///
/// @version 1
/// @req REQ-ING-CMP-001
/// @return the typedefs to render into dm_enums.h
fn compat_enum_types(model: &DataModel, opts: &CodegenOptions) -> Vec<EnumTypeRenderable> {
    if opts.dm_compat {
        collect_enum_types(model)
    } else {
        Vec::new()
    }
}

/// One key's `<PATH>_<VALUE>` constants, ordered by value then name.
///
/// @version 1
/// @req REQ-ING-CMP-001
/// @return the constants in value-then-name order
fn compat_enum_constants(prefix: &str, enum_def: &EnumDef) -> Vec<EnumConstRenderable> {
    let mut values: Vec<EnumConstRenderable> = enum_def
        .values
        .iter()
        .map(|(name, &value)| EnumConstRenderable {
            name: format!("{prefix}_{}", name.to_uppercase().replace(' ', "_")),
            value,
        })
        .collect();
    values.sort_by(|a, b| a.value.cmp(&b.value).then(a.name.cmp(&b.name)));
    values
}

/// Collect one `<PATH>_ENUM_T` typedef per enum-referencing key.
///
/// The typedef and its constants are named after the *key's* path (not the
/// enum domain), matching the DM generator: the same `[enums.*]` domain
/// referenced by two keys yields two distinct typedefs. Constant values are
/// emitted verbatim from the schema (Data Quality — they are wire-semantic).
///
/// @version 1
/// @req REQ-ING-CMP-001
/// @return one typedef per enum-referencing key, in model order
fn collect_enum_types(model: &DataModel) -> Vec<EnumTypeRenderable> {
    let mut out = Vec::new();

    for class in &model.classes {
        let c_ns_name = resolve_ns_name(class, model).to_uppercase();
        let class_name = class.id.to_uppercase();

        for key in &class.keys {
            let Some(enum_ref) = key.enum_ref.as_ref() else {
                continue;
            };
            let Some(enum_def) = model.enums.get(enum_ref) else {
                continue;
            };

            let key_name = key.id.to_uppercase().replace(' ', "_");
            let prefix = format!("{c_ns_name}_{class_name}_{key_name}");
            let named_stem = enums::enum_stem(enum_ref, &model.meta.id);
            let alias_of = (named_stem == prefix).then(|| format!("{named_stem}_T"));

            out.push(EnumTypeRenderable {
                alias_of,
                type_name: format!("{prefix}_ENUM_T"),
                max_macro: enum_max_macro(key.data_type).to_string(),
                values: compat_enum_constants(&prefix, enum_def),
                prefix,
            });
        }
    }

    out
}

/// Every C identifier the `--dm-compat` output declares, mapped to the
/// schema element that produced it, so two elements that would emit the same
/// name are reported instead of producing a header that fails to compile (or,
/// for a macro, silently rewrites another declaration).
#[derive(Default)]
struct IdentRegistry {
    seen: HashMap<String, String>,
    clashes: Vec<String>,
}

impl IdentRegistry {
    /// Record `ident` from `origin`, or a clash when another element already declared it.
    ///
    /// @version 1
    /// @req REQ-ING-CMP-002
    fn add(&mut self, ident: String, origin: &str) {
        if let Some(first) = self.seen.get(&ident) {
            self.clashes
                .push(format!("`{ident}` from {origin} clashes with {first}"));
        } else {
            self.seen.insert(ident, origin.to_string());
        }
    }

    /// Record every key's `DM_KEY_` define and its bare-key alias.
    ///
    /// @version 1
    /// @req REQ-ING-CMP-002
    fn add_keys(&mut self, key_defs: &[KeyDefRenderable]) {
        for k in key_defs {
            let path = format!("key {}.{}.{}", k.namespace, k.class, k.name);
            self.add(k.define_name.clone(), &path);
            self.add(k.bare_name.clone(), &format!("bare-key alias of {path}"));
        }
    }

    /// Record every named enum's typedef and members.
    ///
    /// @version 1
    /// @req REQ-ING-CMP-002
    fn add_named_enums(&mut self, model: &DataModel) {
        for e in enums::collect_enums(model) {
            let origin = format!("enum {}", e.type_name);
            self.add(e.type_name.clone(), &origin);
            for m in e.members {
                self.add(m.name, &origin);
            }
        }
    }

    /// Record every per-key compat typedef, its storage sentinel and its own constants.
    ///
    /// @version 1
    /// @req REQ-ING-CMP-002
    fn add_compat_enums(&mut self, model: &DataModel) {
        for e in collect_enum_types(model) {
            let origin = format!("compat enum {}", e.type_name);
            self.add(e.type_name.clone(), &origin);
            self.add(format!("{}_MAX_STORAGE_VALUE", e.prefix), &origin);
            if e.alias_of.is_none() {
                for c in e.values {
                    self.add(c.name, &origin);
                }
            }
        }
    }

    /// Record every helper's upper- and lower-case accessor aliases.
    ///
    /// v2: the helpers are collected without the namespace ID.
    /// @version 2
    /// @req REQ-ING-CMP-002
    fn add_helpers(&mut self, model: &DataModel) {
        for h in collect_helpers(model) {
            let origin = format!("accessors of {}", h.define_name);
            let mut verbs = vec!["Get"];
            if !h.is_read_only {
                verbs.push("Set");
            }
            for verb in verbs {
                let upper = verb.to_uppercase();
                self.add(format!("DataModel_{verb}_{}", h.helper_name), &origin);
                self.add(format!("DATAMODEL_{upper}_{}", h.helper_name), &origin);
                self.add(format!("DataModel_{verb}{}", h.orig_path), &origin);
            }
        }
    }
}

/// Fail when the `--dm-compat` surface would declare one C identifier twice.
///
/// v2: the helper names no longer take the namespace ID.
/// @version 2
/// @req REQ-ING-CMP-002
/// @return Ok when every identifier is unique, else an error listing each clash
fn check_compat_identifiers(
    model: &DataModel,
    key_defs: &[KeyDefRenderable],
) -> Result<(), String> {
    let mut reg = IdentRegistry::default();
    reg.add_keys(key_defs);
    reg.add_named_enums(model);
    reg.add_compat_enums(model);
    reg.add_helpers(model);
    if reg.clashes.is_empty() {
        return Ok(());
    }
    Err(format!(
        "--dm-compat would declare the same C identifier twice; rename a key, \
         class, enum value or `name` so these differ:\n  {}",
        reg.clashes.join("\n  ")
    ))
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
///
/// v2: defaults come from the shared c_default_literal; an unused KeyEncoding is no longer built.
/// v3: C type, dm_val_t field and buffer flag come from DataType.
/// v4: the define name comes from the shared key_stem.
/// @version 4
/// @req REQ-ING-GEN-009
fn collect_test_keys(model: &DataModel) -> Vec<TestKeyEntry> {
    let mut entries = Vec::new();

    for class in &model.classes {
        for key in &class.keys {
            let define_name = format!("DM_KEY_{}", key_stem(class, model, key));

            let default_c = c_default_literal(&key.default, key.data_type);
            let test_c = format_test_value(key.data_type, &default_c);

            entries.push(TestKeyEntry {
                define_name,
                c_type: key.data_type.c_value_type().to_string(),
                val_field: key.data_type.val_field().to_string(),
                is_string: key.data_type.is_buffer(),
                is_bool: key.data_type == DataType::Bool,
                read_only: key.read_only,
                default_c,
                test_c,
            });
        }
    }

    entries
}

/// Collect persistence test entries for persistent keys in the model.
///
/// v2: defaults come from the shared c_default_literal; an unused KeyEncoding is no longer built.
/// v3: C type, dm_val_t field and buffer flag come from DataType.
/// v4: the define name comes from the shared key_stem.
/// @version 4
/// @req REQ-ING-GEN-009
fn collect_persistence_test_entries(model: &DataModel) -> Vec<PersistenceTestEntry> {
    let mut entries = Vec::new();

    for class in &model.classes {
        for key in &class.keys {
            if !key.persistent {
                continue;
            }

            let define_name = format!("DM_KEY_{}", key_stem(class, model, key));

            let default_c = c_default_literal(&key.default, key.data_type);
            let test_c = format_test_value(key.data_type, &default_c);

            entries.push(PersistenceTestEntry {
                define_name,
                c_type: key.data_type.c_value_type().to_string(),
                val_field: key.data_type.val_field().to_string(),
                is_string: key.data_type.is_buffer(),
                is_bool: key.data_type == DataType::Bool,
                default_c,
                test_c,
            });
        }
    }

    entries
}

/// C literal of a key's default: `true`/`false`, an integer, an escaped string, or the
/// type's zero value when the key has no default.
///
/// Shared by the persistence struct initialiser and the generated Unity tests.
///
/// @version 1
/// @req REQ-ING-GEN-007
/// @req REQ-ING-GEN-009
pub(crate) fn c_default_literal(default: &Option<toml::Value>, data_type: DataType) -> String {
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
/// @version 1
/// @req REQ-ING-GEN-009
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_list_separates_with_comma_space() {
        assert_eq!(join_list(&[0, -1, 127]), "0, -1, 127");
    }

    #[test]
    fn join_list_of_one_and_of_none() {
        assert_eq!(join_list(&[42]), "42");
        assert_eq!(join_list::<i32>(&[]), "");
    }

    #[test]
    fn c_default_literal_of_declared_defaults() {
        use toml::Value;
        assert_eq!(
            c_default_literal(&Some(Value::Boolean(true)), DataType::Bool),
            "true"
        );
        assert_eq!(
            c_default_literal(&Some(Value::Integer(-7)), DataType::Int8),
            "-7"
        );
        assert_eq!(
            c_default_literal(&Some(Value::String("a\"b".into())), DataType::String),
            "\"a\\\"b\""
        );
    }

    #[test]
    fn c_default_literal_without_default_is_the_zero_value() {
        assert_eq!(c_default_literal(&None, DataType::Bool), "false");
        assert_eq!(c_default_literal(&None, DataType::Uint32), "0");
        assert_eq!(c_default_literal(&None, DataType::String), "\"\"");
        assert_eq!(c_default_literal(&None, DataType::Binary), "\"\"");
    }

    fn two_class_model() -> DataModel {
        DataModel::from_toml(
            r#"
[meta]
id = "rig"
version = "1.0.0"
namespace_id = 3

[[classes]]
id = "motor"
[[classes.keys]]
id = "speed"
type = "uint16"
[[classes.keys]]
id = "fault"
type = "bool"
key_index = 9
read_only = true

[[classes]]
id = "door"
class_index = 4
[[classes.keys]]
id = "open"
type = "bool"
thread_safe = true
"#,
        )
        .unwrap()
    }

    #[test]
    fn key_id_uses_explicit_index_or_stamped_position() {
        let mut model = two_class_model();
        let motor = &model.classes[0];
        // 3<<22 | 0<<17 | position 0<<7 | uint16 (2)<<3
        assert_eq!(key_id(motor, 3, &motor.keys[0]), 0x00C0_0010);
        // explicit key_index 9 wins over position 1; bool type 0; read-only bit 0
        assert_eq!(key_id(motor, 3, &motor.keys[1]), 0x00C0_0481);
        // a merged file's class carries its own namespace ID over the model fallback
        model.classes[1].namespace_id = Some(7);
        let door = &model.classes[1];
        assert_eq!(key_id(door, 3, &door.keys[0]) >> 22, 7);
    }

    #[test]
    fn encoded_keys_filters_in_declaration_order() {
        let model = two_class_model();
        let bools = encoded_keys(&model, 3, |k| k.data_type == DataType::Bool);
        let ids: Vec<u32> = bools.iter().map(|(id, _)| *id).collect();
        // motor.fault (class 0, index 9, read-only), door.open (class_index 4, thread-safe)
        assert_eq!(ids, vec![0x00C0_0481, 0x00C8_0004]);
        assert_eq!(bools[1].1.id, "open");
    }
}
