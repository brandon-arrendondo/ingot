/// Supported target platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Stm32,
    EspXtensa,
    EspRiscv,
    Mcu8bit,
    Linux64,
}

/// Target-specific configuration for code generation.
#[derive(Debug, Clone)]
pub struct TargetConfig {
    pub target: Target,
    pub pointer_width: u8,
    pub has_mutex: bool,
    pub alignment: u8,
    pub max_inline_size: usize,
    pub include_prefix: String,
}

impl TargetConfig {
    pub fn for_target(target: Target) -> Self {
        match target {
            Target::Stm32 => Self {
                target,
                pointer_width: 32,
                has_mutex: false,
                alignment: 4,
                max_inline_size: 64,
                include_prefix: String::new(),
            },
            Target::EspXtensa | Target::EspRiscv => Self {
                target,
                pointer_width: 32,
                has_mutex: true,
                alignment: 4,
                max_inline_size: 64,
                include_prefix: String::new(),
            },
            Target::Mcu8bit => Self {
                target,
                pointer_width: 16,
                has_mutex: false,
                alignment: 1,
                max_inline_size: 32,
                include_prefix: String::new(),
            },
            Target::Linux64 => Self {
                target,
                pointer_width: 64,
                has_mutex: true,
                alignment: 8,
                max_inline_size: 128,
                include_prefix: String::new(),
            },
        }
    }
}
