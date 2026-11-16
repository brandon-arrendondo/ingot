use clap::Parser;
use std::path::PathBuf;

mod codegen;
mod contract;
mod hash;
mod model;

/// `ingot --version`: the package version and the commit the binary was built from (the same
/// INGOT_GIT_COMMIT contract mode stamps into its outputs).
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("INGOT_GIT_COMMIT"),
    ")"
);

/// ingot - Embedded database code generator
///
/// Generates optimized C99 or no_std Rust code for embedded key-value
/// databases. Uses compile-time perfect hashing for O(1) key lookup with
/// minimal RAM/ROM footprint.
///
/// Supported targets: STM32 (32-bit), ESP32 (Xtensa/RISC-V), 8-bit
/// microcontrollers, 64-bit Linux systems, and no_std Rust.
#[derive(Parser, Debug)]
#[command(name = "ingot", version = VERSION, about, long_about)]
struct Cli {
    /// Path to data model TOML file(s) or directory of TOML files (repeatable)
    #[arg(short, long, required_unless_present_any = ["table_types", "contract"])]
    model: Vec<PathBuf>,

    /// Write only the model-independent table-interface headers (dmt_types.h, dsh_types.h)
    /// to --output
    #[arg(long, conflicts_with_all = ["model", "contract"])]
    table_types: bool,

    /// Contract mode: generate the table-interface store tables from this contract.toml
    #[arg(long, conflicts_with = "model")]
    contract: Option<PathBuf>,

    /// Contract mode: the previous release's contract.json, for the minor/major checks
    #[arg(long, requires = "contract")]
    previous_contract: Option<PathBuf>,

    /// Contract mode: accept an ingot built from a dirty tree (for ingot's own tests)
    #[arg(long, requires = "contract")]
    allow_dirty_ingot: bool,

    /// Contract mode: the shadow composer's buffer in bytes; a full shadow document that can
    /// exceed it is warned about
    #[arg(long, requires = "contract")]
    shadow_buffer: Option<u32>,

    /// Contract mode: a directory of SDK templates binding the generated per-principal SDKs to
    /// the consumer's store runtime (no SDK without it)
    #[arg(long, requires = "contract")]
    sdk_binding: Option<PathBuf>,

    /// Output directory for generated code
    #[arg(short, long, default_value = "generated")]
    output: PathBuf,

    /// Target platform
    #[arg(short, long, value_enum, default_value_t = Target::Linux64)]
    target: Target,

    /// Disable event callback generation
    #[arg(long)]
    no_events: bool,

    /// Emit C++/tinyfsm event structs + dispatch-by-key wrapper for event keys
    /// (additive, opt-in; independent of --no-events; C99 output unchanged)
    #[arg(long)]
    emit_tinyfsm: bool,

    /// Emit the DM-shaped C compatibility surface (additive, opt-in): uppercase
    /// DATAMODEL_GET/SET_<PATH> aliases, lowercase DataModel_{Get,Set}<path>
    /// aliases, bare-key `#define <PATH> DM_KEY_<PATH>` constants, per-key
    /// <PATH>_ENUM_T typedefs, and legacy header-name aliases. C targets only.
    #[arg(long)]
    dm_compat: bool,

    /// With --dm-compat: include guard of a legacy dm_enums.h that already
    /// declares the per-key <PATH>_ENUM_T typedefs. The compat typedefs are
    /// skipped in any translation unit that has already included it.
    #[arg(
        long,
        value_name = "MACRO",
        requires = "dm_compat",
        value_parser = parse_c_identifier
    )]
    dm_enums_guard: Option<String>,

    /// YAML file listing keys to include (whitelist); all others are excluded
    #[arg(long)]
    include_list: Option<PathBuf>,

    /// YAML file listing keys to exclude (blacklist); all others are included
    #[arg(long)]
    exclude_list: Option<PathBuf>,

    /// YAML file listing keys that should be marked persistent
    #[arg(long)]
    persistent_keys: Option<PathBuf>,

    /// YAML file with per-key property overrides (default_value)
    #[arg(long)]
    property_override_list: Option<PathBuf>,

    /// Product variant for per-variant default overrides (e.g. variant_a, variant_b_v2)
    #[arg(long)]
    variant: Option<String>,

    /// Templates directory (overrides INGOT_TEMPLATES_DIR, the template search path and the
    /// templates built into ingot)
    #[arg(long, env = "INGOT_TEMPLATES_DIR")]
    templates: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum Target {
    /// 32-bit ARM STM32 microcontrollers (bare-metal)
    Stm32,
    /// ESP32 Xtensa-based (FreeRTOS)
    EspXtensa,
    /// ESP32 RISC-V based (FreeRTOS)
    EspRiscv,
    /// 8-bit microcontrollers (bare-metal)
    Mcu8bit,
    /// 64-bit Linux systems
    Linux64,
    /// Rust data model (no_std, perfect-hash static storage) instead of C
    Rust,
}

/// Parse the command line, set the log level and run generation, exiting 1 on error.
///
/// v2: runs through dispatch, which also handles --table-types.
/// @version 2
/// @req REQ-ING-CLI-001
fn main() {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(match cli.verbose {
            0 => log::LevelFilter::Warn,
            1 => log::LevelFilter::Info,
            2 => log::LevelFilter::Debug,
            _ => log::LevelFilter::Trace,
        })
        .init();

    for path in &cli.model {
        log::info!("Model: {}", path.display());
    }
    log::info!("Output: {}", cli.output.display());
    log::info!("Target: {:?}", cli.target);

    if let Err(e) = dispatch(&cli) {
        log::error!("{e}");
        std::process::exit(1);
    }
}

/// Load the templates and run the mode the command line selects.
///
/// v2: contract mode, which only uses the built-in templates.
/// v3: model mode searches upstream's template directories before the built-in templates.
/// @version 3
/// @req REQ-ING-CLI-001
/// @req REQ-ING-CLI-002
/// @req REQ-ING-TIV-001
/// @req REQ-ING-TIV-009
fn dispatch(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.contract.is_some() && cli.templates.is_some() {
        return Err("contract mode uses the built-in templates; drop --templates".into());
    }
    let explicit = cli.templates.as_deref();
    match (&cli.contract, cli.table_types) {
        (Some(contract), _) => contract::generate(
            &contract_options(cli, contract),
            &codegen::templates::load(None)?,
        ),
        (None, true) => {
            codegen::tables::write_table_types(&codegen::templates::load(explicit)?, &cli.output)
        }
        (None, false) => run(cli, &codegen::templates::load_searching(explicit)?),
    }
}

/// The C code-generation toggles selected on the command line.
///
/// @version 1
/// @req REQ-ING-CLI-001
/// @req REQ-ING-CMP-001
/// @return the options codegen::generate renders with
fn codegen_options(cli: &Cli) -> codegen::CodegenOptions {
    codegen::CodegenOptions {
        no_events: cli.no_events,
        emit_tinyfsm: cli.emit_tinyfsm,
        dm_compat: cli.dm_compat,
        dm_enums_guard: cli.dm_enums_guard.clone(),
    }
}

/// Accept only a C identifier, since the value is pasted into `#ifndef`.
///
/// @version 1
/// @req REQ-ING-CMP-003
/// @return the identifier, or an error naming the rejected value
fn parse_c_identifier(raw: &str) -> Result<String, String> {
    let mut chars = raw.chars();
    let starts_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if starts_ok && chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(raw.to_string())
    } else {
        Err(format!("'{raw}' is not a C identifier"))
    }
}

/// Generate the no_std Rust module, warning about any C-only flags given.
///
/// @version 1
/// @req REQ-ING-RST-001
/// @return Ok once the module is written, else the generation error
fn generate_rust(
    cli: &Cli,
    data_model: &model::schema::DataModel,
    ns_id: u16,
    tera: &tera::Tera,
) -> Result<(), Box<dyn std::error::Error>> {
    warn_c_only_flags(cli);
    codegen::rust_kvp::generate(data_model, ns_id, &cli.output, tera)?;
    log::info!("Rust code generation complete → {}", cli.output.display());
    Ok(())
}

/// Warn about each C-only flag given with `--target rust`, where it has no effect.
///
/// @version 1
/// @req REQ-ING-RST-001
fn warn_c_only_flags(cli: &Cli) {
    let flags = [
        (cli.emit_tinyfsm, "--emit-tinyfsm"),
        (cli.no_events, "--no-events"),
        (cli.dm_compat, "--dm-compat"),
    ];
    for (_, flag) in flags.iter().filter(|(set, _)| *set) {
        log::warn!("{flag} has no effect for --target rust (C targets only)");
    }
}

/// The contract-mode options the command line gives.
///
/// v2: the shadow composer's buffer.
/// v3: the SDK binding.
/// @version 3
/// @req REQ-ING-TIV-002
/// @req REQ-ING-DSH-005
/// @req REQ-ING-SDK-004
fn contract_options<'a>(cli: &'a Cli, contract: &'a std::path::Path) -> contract::Options<'a> {
    contract::Options {
        contract,
        variant: cli.variant.as_deref(),
        previous: cli.previous_contract.as_deref(),
        output: &cli.output,
        allow_dirty: cli.allow_dirty_ingot,
        shadow_buffer: cli.shadow_buffer,
        sdk_binding: cli.sdk_binding.as_deref(),
    }
}

/// Load, filter, validate and generate the model the command line names.
///
/// v2: renders with the templates `dispatch` loaded.
/// v3: positions are stamped when each file is parsed (DataModel::from_toml), not here.
/// v4: --target rust goes through generate_rust; C generation takes the CodegenOptions.
/// @version 4
/// @req REQ-ING-CLI-001
fn run(cli: &Cli, tera: &tera::Tera) -> Result<(), Box<dyn std::error::Error>> {
    if cli.include_list.is_some() && cli.exclude_list.is_some() {
        return Err("--include-list and --exclude-list are mutually exclusive".into());
    }

    let mut data_model = load_models(&cli.model)?;

    // Apply key filtering lists
    if let Some(ref path) = cli.include_list {
        let list = model::filter::load_key_list(path)?;
        log::info!("Include list: {} keys from {}", list.len(), path.display());
        model::filter::apply_include_list(&mut data_model, &list);
    }
    if let Some(ref path) = cli.exclude_list {
        let list = model::filter::load_key_list(path)?;
        log::info!("Exclude list: {} keys from {}", list.len(), path.display());
        model::filter::apply_exclude_list(&mut data_model, &list);
    }
    if let Some(ref path) = cli.persistent_keys {
        let list = model::filter::load_key_list(path)?;
        log::info!(
            "Persistent keys: {} entries from {}",
            list.len(),
            path.display()
        );
        model::filter::apply_persistent_keys(&mut data_model, &list);
    }
    if let Some(ref path) = cli.property_override_list {
        let overrides = model::filter::load_property_overrides(path)?;
        log::info!(
            "Property overrides: {} entries from {}",
            overrides.len(),
            path.display()
        );
        let applied = model::filter::apply_property_overrides(&mut data_model, &overrides);
        log::info!("Applied {} property override(s)", applied);
    }

    // Resolve per-variant defaults and enum overrides
    if let Some(ref variant) = cli.variant {
        let stats = resolve_variant(&mut data_model, variant);
        log::info!(
            "Variant '{}': {} key default(s), {} enum(s) overridden",
            variant,
            stats.0,
            stats.1
        );
    }

    if let Err(errors) = model::validation::validate(&data_model) {
        for e in &errors {
            log::error!("{e}");
        }
        return Err(format!("{} validation error(s)", errors.len()).into());
    }
    log::info!("Validation passed");

    let key_count: usize = data_model.classes.iter().map(|c| c.keys.len()).sum();
    log::info!(
        "{} classes, {} keys, {} enums",
        data_model.classes.len(),
        key_count,
        data_model.enums.len()
    );

    print_statistics(&data_model);

    // Namespace ID 0 as fallback (per-class overrides take precedence)
    let ns_id: u16 = data_model.meta.namespace_id.unwrap_or(0);

    if cli.target == Target::Rust {
        return generate_rust(cli, &data_model, ns_id, tera);
    }

    let target = match cli.target {
        Target::Stm32 => codegen::target::Target::Stm32,
        Target::EspXtensa => codegen::target::Target::EspXtensa,
        Target::EspRiscv => codegen::target::Target::EspRiscv,
        Target::Mcu8bit => codegen::target::Target::Mcu8bit,
        Target::Linux64 => codegen::target::Target::Linux64,
        Target::Rust => unreachable!("handled above"),
    };
    let target_config = codegen::target::TargetConfig::for_target(target);

    codegen::generate(
        &data_model,
        ns_id,
        &cli.output,
        tera,
        &target_config,
        codegen_options(cli),
    )?;
    log::info!("Code generation complete → {}", cli.output.display());

    Ok(())
}

/// Resolve per-variant default overrides for keys and enum values.
///
/// For each key that has a `defaults[variant]` entry, replace `default`
/// with the variant-specific value. For each enum with a `variants[variant]`
/// entry, replace `values` with the variant's values.
///
/// Returns (key_defaults_overridden, enums_overridden).
/// @version 1
/// @req REQ-ING-CLI-004
fn resolve_variant(model: &mut model::DataModel, variant: &str) -> (usize, usize) {
    let mut key_count = 0;
    for class in &mut model.classes {
        for key in &mut class.keys {
            if let Some(val) = key.defaults.get(variant) {
                key.default = Some(val.clone());
                key_count += 1;
            }
        }
    }

    let mut enum_count = 0;
    for enum_def in model.enums.values_mut() {
        if let Some(variant_values) = enum_def.variants.get(variant) {
            // Merge variant values into the base values (variant overrides base)
            for (name, &val) in variant_values {
                enum_def.values.insert(name.clone(), val);
            }
            enum_count += 1;
        }
    }

    (key_count, enum_count)
}

/// Print the model's key count per storage type to stdout.
///
/// @version 1
/// @req REQ-ING-CLI-005
fn print_statistics(model: &model::DataModel) {
    use model::schema::DataType;

    let mut bool_count = 0usize;
    let mut u8_count = 0usize;
    let mut i8_count = 0usize;
    let mut u16_count = 0usize;
    let mut i16_count = 0usize;
    let mut u32_count = 0usize;
    let mut i32_count = 0usize;
    let mut ro_string_count = 0usize;
    let mut rw_string_count = 0usize;

    for class in &model.classes {
        for key in &class.keys {
            match key.data_type {
                DataType::Bool => bool_count += 1,
                DataType::Uint8 => u8_count += 1,
                DataType::Int8 => i8_count += 1,
                DataType::Uint16 => u16_count += 1,
                DataType::Int16 => i16_count += 1,
                DataType::Uint32 => u32_count += 1,
                DataType::Int32 => i32_count += 1,
                DataType::String => {
                    if key.read_only {
                        ro_string_count += 1;
                    } else {
                        rw_string_count += 1;
                    }
                }
                DataType::Binary => {}
            }
        }
    }

    println!("Data Model Statistics");
    println!("  bool: {bool_count}");
    println!("  u8: {u8_count}");
    println!("  u16: {u16_count}");
    println!("  u32: {u32_count}");
    println!("  int8: {i8_count}");
    println!("  int16: {i16_count}");
    println!("  int32: {i32_count}");
    println!("  read-only string: {ro_string_count}");
    println!("  read-write string: {rw_string_count}");
}

/// Expand model paths to TOML files; a directory contributes its .toml files in sorted order.
///
/// @version 1
/// @req REQ-ING-CLI-003
fn expand_to_toml_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut toml_files: Vec<PathBuf> = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut dir_files: Vec<PathBuf> = std::fs::read_dir(path)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
                .collect();
            if dir_files.is_empty() {
                return Err(format!("No .toml files found in {}", path.display()).into());
            }
            dir_files.sort();
            log::info!(
                "Loading {} model file(s) from {}",
                dir_files.len(),
                path.display()
            );
            toml_files.extend(dir_files);
        } else {
            toml_files.push(path.clone());
        }
    }
    Ok(toml_files)
}

/// Validate several namespace files and merge them into one model, keeping each class's namespace.
///
/// v2: the merged meta no longer sets the removed, never-read `doc` field.
/// v3: each file is parsed (and its positions stamped) by DataModel::from_toml.
/// @version 3
/// @req REQ-ING-CLI-003
fn merge_models(paths: &[PathBuf]) -> Result<model::DataModel, Box<dyn std::error::Error>> {
    let mut merged_classes = Vec::new();
    let mut merged_enums = std::collections::BTreeMap::new();

    for path in paths {
        let model_str = std::fs::read_to_string(path)?;
        let file_model = model::DataModel::from_toml(&model_str)
            .map_err(|e| format!("{}: {e}", path.display()))?;

        log::info!(
            "  {} — namespace '{}' (id={:?}), {} class(es), {} enum(s)",
            path.file_name().unwrap_or_default().to_string_lossy(),
            file_model.meta.id,
            file_model.meta.namespace_id,
            file_model.classes.len(),
            file_model.enums.len(),
        );

        if let Err(errors) = model::validation::validate(&file_model) {
            for e in &errors {
                log::error!("{}: {e}", path.display());
            }
            return Err(format!("{}: {} validation error(s)", path.display(), errors.len()).into());
        }

        let ns_name = file_model.meta.id.clone();
        let ns_id = file_model.meta.namespace_id;

        for mut class in file_model.classes {
            class.namespace_name = Some(ns_name.clone());
            class.namespace_id = ns_id;
            for key in &mut class.keys {
                if let Some(ref enum_name) = key.enum_ref {
                    key.enum_ref = Some(format!("{}::{}", ns_name, enum_name));
                }
            }
            merged_classes.push(class);
        }

        for (enum_name, enum_def) in file_model.enums {
            merged_enums.insert(format!("{}::{}", ns_name, enum_name), enum_def);
        }
    }

    Ok(model::DataModel {
        meta: model::schema::Meta {
            id: "unified".to_string(),
            version: "0.0.0".to_string(),
            namespace_id: None,
        },
        enums: merged_enums,
        classes: merged_classes,
    })
}

/// Load model(s) from one or more paths (files and/or directories).
///
/// A single file is loaded directly. Multiple paths or directories are
/// merged into one DataModel with per-class namespace info preserved.
/// v2: parsed (and positions stamped) by DataModel::from_toml.
/// @version 2
/// @req REQ-ING-CLI-003
fn load_models(paths: &[PathBuf]) -> Result<model::DataModel, Box<dyn std::error::Error>> {
    let toml_files = expand_to_toml_files(paths)?;

    if toml_files.len() == 1 {
        let path = &toml_files[0];
        let model_str = std::fs::read_to_string(path)?;
        let m = model::DataModel::from_toml(&model_str)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        log::info!("Parsed namespace '{}' v{}", m.meta.id, m.meta.version);
        return Ok(m);
    }

    merge_models(&toml_files)
}
