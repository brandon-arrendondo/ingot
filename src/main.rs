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

    /// Templates directory (overrides INGOT_TEMPLATES_DIR and all auto-search paths)
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

/// Find the templates/ directory.
///
/// Search order:
///   1. Explicit path from `--templates` / `INGOT_TEMPLATES_DIR` (if provided)
///   2. `<exe_dir>/templates/`
///   3. `<exe_dir>/../share/ingot/templates/`  (FHS install: /usr/bin → /usr/share/ingot)
///   4. `<exe_dir>/../../templates/`           (dev: target/debug/ingot → crate root)
///   5. `<cwd>/templates/`
///   6. `/usr/share/ingot/templates/`          (absolute fallback for non-FHS installs)
fn resolve_template_dir(
    explicit: Option<&std::path::Path>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(p) = explicit {
        if p.is_dir() {
            return Ok(p.to_path_buf());
        }
        return Err(format!(
            "Specified templates directory does not exist: {}",
            p.display()
        )
        .into());
    }

    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe.parent().unwrap_or(std::path::Path::new("."));

        // 2. next to the executable
        let c = exe_dir.join("templates");
        if c.is_dir() {
            return Ok(c);
        }

        // 3. FHS: <prefix>/bin/../share/ingot/templates → <prefix>/share/ingot/templates
        let c = exe_dir.join("../share/ingot/templates");
        if c.is_dir() {
            return Ok(c.canonicalize().unwrap_or(c));
        }

        // 4. development layout: target/debug/ingot → ../../templates
        if let Some(c) = exe_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("templates"))
        {
            if c.is_dir() {
                return Ok(c);
            }
        }
    }

    // 5. CWD
    let cwd = std::env::current_dir()?;
    let c = cwd.join("templates");
    if c.is_dir() {
        return Ok(c);
    }

    // 6. absolute system fallback
    let c = std::path::Path::new("/usr/share/ingot/templates");
    if c.is_dir() {
        return Ok(c.to_path_buf());
    }

    Err(
        "Could not find templates/ directory. Use --templates <DIR> or set INGOT_TEMPLATES_DIR."
            .into(),
    )
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.include_list.is_some() && cli.exclude_list.is_some() {
        return Err("--include-list and --exclude-list are mutually exclusive".into());
    }

    let template_dir = resolve_template_dir(cli.templates.as_deref())?;

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

    if cli.target == Target::Rust {
        if cli.emit_tinyfsm {
            log::warn!("--emit-tinyfsm has no effect for --target rust (C++/tinyfsm only)");
        }
        if cli.no_events {
            log::warn!("--no-events has no effect for --target rust (C-only concept)");
        }
        codegen::rust_kvp::generate(&data_model, ns_id, &cli.output, &template_dir)?;
        log::info!("Rust code generation complete → {}", cli.output.display());
        return Ok(());
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

fn merge_models(paths: &[PathBuf]) -> Result<model::DataModel, Box<dyn std::error::Error>> {
    let mut merged_classes = Vec::new();
    let mut merged_enums = std::collections::BTreeMap::new();

    for path in paths {
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
            doc: None,
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
fn load_models(paths: &[PathBuf]) -> Result<model::DataModel, Box<dyn std::error::Error>> {
    let toml_files = expand_to_toml_files(paths)?;

    if toml_files.len() == 1 {
        let path = &toml_files[0];
        let model_str = std::fs::read_to_string(path)?;
        let m: model::DataModel =
            toml::from_str(&model_str).map_err(|e| format!("{}: {e}", path.display()))?;
        log::info!("Parsed namespace '{}' v{}", m.meta.id, m.meta.version);
        return Ok(m);
    }

    merge_models(&toml_files)
}
