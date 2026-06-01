// TODO: remove once modules have consumers
#![allow(dead_code)]

use clap::Parser;
use std::path::PathBuf;

mod codegen;
mod hash;
mod model;

/// ingot - Embedded database C code generator
///
/// Generates optimized C code for key-value databases targeting embedded
/// systems. Uses compile-time perfect hashing for O(1) key lookup with
/// minimal RAM/ROM footprint.
///
/// Supported targets: STM32 (32-bit), ESP32 (Xtensa/RISC-V), 8-bit
/// microcontrollers, and 64-bit Linux systems.
#[derive(Parser, Debug)]
#[command(name = "ingot", version, about, long_about)]
struct Cli {
    /// Path to data model TOML file(s) or directory of TOML files (repeatable)
    #[arg(short, long, required = true)]
    model: Vec<PathBuf>,

    /// Output directory for generated C code
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

    /// Product variant for per-variant default overrides (e.g. peabodyv0, omnidrive_v2)
    #[arg(long)]
    variant: Option<String>,

    /// Enable verbose output
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(clap::ValueEnum, Clone, Debug)]
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
}

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

    if let Err(e) = run(&cli) {
        log::error!("{e}");
        std::process::exit(1);
    }
}

/// Find the templates/ directory relative to the executable or CWD.
fn resolve_template_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Check next to the executable first
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(std::path::Path::new("."));
        let candidate = dir.join("templates");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        // For development: check parent of target/debug/
        let dev_candidate = dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("templates"));
        if let Some(ref dc) = dev_candidate {
            if dc.is_dir() {
                return Ok(dc.clone());
            }
        }
    }

    // Fallback to CWD
    let cwd = std::env::current_dir()?;
    let candidate = cwd.join("templates");
    if candidate.is_dir() {
        return Ok(candidate);
    }

    Err("Could not find templates/ directory".into())
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.include_list.is_some() && cli.exclude_list.is_some() {
        return Err("--include-list and --exclude-list are mutually exclusive".into());
    }

    let template_dir = resolve_template_dir()?;

    let target = match cli.target {
        Target::Stm32 => codegen::target::Target::Stm32,
        Target::EspXtensa => codegen::target::Target::EspXtensa,
        Target::EspRiscv => codegen::target::Target::EspRiscv,
        Target::Mcu8bit => codegen::target::Target::Mcu8bit,
        Target::Linux64 => codegen::target::Target::Linux64,
    };
    let target_config = codegen::target::TargetConfig::for_target(target);

    let mut data_model = load_models(&cli.model)?;

    // Stamp original key positions before filtering so encoded IDs survive
    // include/exclude list pruning (matching gen_udm_code behaviour).
    for class in &mut data_model.classes {
        for (i, key) in class.keys.iter_mut().enumerate() {
            if key.key_index.is_none() {
                key.key_index = Some(i as u16);
            }
        }
    }

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

    codegen::generate(
        &data_model,
        ns_id,
        &cli.output,
        &template_dir,
        &target_config,
        cli.no_events,
        cli.emit_tinyfsm,
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

/// Load model(s) from one or more paths (files and/or directories).
///
/// A single file is loaded directly. Multiple paths or directories are
/// merged into one DataModel with per-class namespace info preserved.
fn load_models(paths: &[PathBuf]) -> Result<model::DataModel, Box<dyn std::error::Error>> {
    // Expand directories into individual .toml files
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

    // Single file — load directly (no merge overhead, preserves model meta)
    if toml_files.len() == 1 {
        let path = &toml_files[0];
        let model_str = std::fs::read_to_string(path)?;
        let m: model::DataModel =
            toml::from_str(&model_str).map_err(|e| format!("{}: {e}", path.display()))?;
        log::info!("Parsed namespace '{}' v{}", m.meta.id, m.meta.version);
        return Ok(m);
    }

    // Multiple files — merge into a single DataModel
    let mut merged_classes = Vec::new();
    let mut merged_enums = std::collections::BTreeMap::new();

    for path in &toml_files {
        let model_str = std::fs::read_to_string(path)?;
        let file_model: model::DataModel =
            toml::from_str(&model_str).map_err(|e| format!("{}: {e}", path.display()))?;

        log::info!(
            "  {} — namespace '{}' (id={:?}), {} class(es), {} enum(s)",
            path.file_name().unwrap_or_default().to_string_lossy(),
            file_model.meta.id,
            file_model.meta.namespace_id,
            file_model.classes.len(),
            file_model.enums.len(),
        );

        // Validate each file independently
        if let Err(errors) = model::validation::validate(&file_model) {
            for e in &errors {
                log::error!("{}: {e}", path.display());
            }
            return Err(format!("{}: {} validation error(s)", path.display(), errors.len()).into());
        }

        let ns_name = file_model.meta.id.clone();
        let ns_id = file_model.meta.namespace_id;

        for (i, mut class) in file_model.classes.into_iter().enumerate() {
            class.namespace_name = Some(ns_name.clone());
            class.namespace_id = ns_id;
            if class.class_index.is_none() {
                class.class_index = Some(i as u8);
            }
            // Rewrite enum_ref to namespaced form to avoid cross-namespace collisions
            for key in &mut class.keys {
                if let Some(ref enum_name) = key.enum_ref {
                    key.enum_ref = Some(format!("{}::{}", ns_name, enum_name));
                }
            }
            merged_classes.push(class);
        }

        // Namespace-prefix all enum keys to avoid collisions across files
        for (enum_name, enum_def) in file_model.enums {
            merged_enums.insert(format!("{}::{}", ns_name, enum_name), enum_def);
        }
    }

    Ok(model::DataModel {
        meta: model::schema::Meta {
            id: "unified".to_string(),
            version: "0.0.0".to_string(),
            doc: None,
            namespace_id: None,
        },
        enums: merged_enums,
        classes: merged_classes,
    })
}
