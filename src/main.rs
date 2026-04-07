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
    /// Path to the data model TOML specification
    #[arg(short, long)]
    model: PathBuf,

    /// Output directory for generated C code
    #[arg(short, long, default_value = "generated")]
    output: PathBuf,

    /// Target platform
    #[arg(short, long, value_enum, default_value_t = Target::Linux64)]
    target: Target,

    /// Disable event callback generation
    #[arg(long)]
    no_events: bool,

    /// YAML file listing keys to include (whitelist); all others are excluded
    #[arg(long)]
    include_list: Option<PathBuf>,

    /// YAML file listing keys to exclude (blacklist); all others are included
    #[arg(long)]
    exclude_list: Option<PathBuf>,

    /// YAML file listing keys that should be marked persistent
    #[arg(long)]
    persistent_keys: Option<PathBuf>,

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

    log::info!("Model: {}", cli.model.display());
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

    let model_str = std::fs::read_to_string(&cli.model)?;
    let mut data_model: model::DataModel = toml::from_str(&model_str)?;

    log::info!(
        "Parsed namespace '{}' v{}",
        data_model.meta.id,
        data_model.meta.version
    );

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

    // Resolve template directory (bundled with binary or local)
    let template_dir = resolve_template_dir()?;

    // Namespace ID (0 for single-namespace, configurable later for multi-namespace)
    let ns_id: u16 = 0;

    let target = match cli.target {
        Target::Stm32 => codegen::target::Target::Stm32,
        Target::EspXtensa => codegen::target::Target::EspXtensa,
        Target::EspRiscv => codegen::target::Target::EspRiscv,
        Target::Mcu8bit => codegen::target::Target::Mcu8bit,
        Target::Linux64 => codegen::target::Target::Linux64,
    };
    let target_config = codegen::target::TargetConfig::for_target(target);

    codegen::generate(
        &data_model,
        ns_id,
        &cli.output,
        &template_dir,
        &target_config,
        cli.no_events,
    )?;
    log::info!("Code generation complete → {}", cli.output.display());

    Ok(())
}
