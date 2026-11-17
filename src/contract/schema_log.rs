//! Contract-mode input of a log stream: the core's `[core_log]` and a principal's `log = { ... }`
//! in `[[principals]]` (docs/contract-mode.rst), and the table-interface codes of its backings,
//! levels and flush triggers. Unknown fields are errors.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a stream's lines go while it is enabled.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum Backing {
    /// Disabled: no RAM, no flash.
    #[default]
    Off,
    /// A RAM ring of `ram_cap_bytes`.
    Ring,
    /// A RAM ring copied to a flash segment on a flush trigger.
    FlashOnTrigger,
    /// RAM staging flushed to flash in groups, under the byte rate.
    FlashGroup,
}

/// Every backing, in code order.
pub const BACKINGS: [Backing; 4] = [
    Backing::Off,
    Backing::Ring,
    Backing::FlashOnTrigger,
    Backing::FlashGroup,
];

impl Backing {
    /// `DMT_LB_*` code.
    ///
    /// @version 1
    /// @req REQ-ING-LOG-001
    /// @return the code.
    pub fn code(self) -> u8 {
        self as u8
    }

    /// Whether the backing writes flash.
    ///
    /// @version 1
    /// @req REQ-ING-LOG-003
    /// @return true for the flash backings.
    pub fn flash(self) -> bool {
        matches!(self, Backing::FlashOnTrigger | Backing::FlashGroup)
    }

    /// Name used in the model, contract.json and refusal messages.
    ///
    /// @version 1
    /// @req REQ-ING-LOG-004
    /// @return the name.
    pub fn name(self) -> &'static str {
        match self {
            Backing::Off => "off",
            Backing::Ring => "ring",
            Backing::FlashOnTrigger => "flash_on_trigger",
            Backing::FlashGroup => "flash_group",
        }
    }
}

/// A log level; a level mask holds `1 << code` per level.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Error,
    Warning,
    Info,
    Debug,
}

/// A flush trigger of a flash backing; `flush` is a mask of their bits.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlushTrigger {
    Fault,
    Watchdog,
    Shutdown,
    Reboot,
    ErrorLine,
    Request,
    RestartStorm,
    HighWater,
}

/// Bit mask of a set of codes (`1 << code` each).
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the mask.
pub fn mask<T: Copy>(items: &[T], code: impl Fn(T) -> u8) -> u16 {
    items.iter().fold(0, |acc, &i| acc | (1 << code(i)))
}

/// `DMT_LL_*` code of a level.
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the code.
pub fn level_code(level: Level) -> u8 {
    level as u8
}

/// Bit index of a flush trigger in `DMT_LT_*`.
///
/// @version 1
/// @req REQ-ING-LOG-001
/// @return the bit index.
pub fn trigger_code(trigger: FlushTrigger) -> u8 {
    trigger as u8
}

/// `[core_log]` or a principal's `log = { ... }`: one log stream.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LogDef {
    /// The stream's name; absent: the principal's name, each dot an underscore (`core` for the
    /// core's stream).
    pub name: Option<String>,
    /// The backing at boot.
    #[serde(default)]
    pub backing: Backing,
    #[serde(default)]
    pub backing_by_variant: BTreeMap<String, Backing>,
    /// The backings the runtime switch may select; absent: the backing (off is always allowed).
    pub allowed_backings: Option<Vec<Backing>>,
    #[serde(default)]
    pub allowed_backings_by_variant: BTreeMap<String, Vec<Backing>>,
    #[serde(default)]
    pub ram_cap_bytes: u32,
    #[serde(default)]
    pub max_line_bytes: u32,
    #[serde(default)]
    pub default_level_ram: Vec<Level>,
    #[serde(default)]
    pub default_level_file: Vec<Level>,
    #[serde(default)]
    pub flash_seg_bytes: u32,
    #[serde(default)]
    pub flash_seg_count: u32,
    #[serde(default)]
    pub flash_rate_bytes_per_day: u32,
    #[serde(default)]
    pub flash_rate_burst_bytes: u32,
    #[serde(default)]
    pub flush: Vec<FlushTrigger>,
    pub ctl_key: String,
    pub drop_counter_key: String,
    /// The key the core writes with the stream's effective state; no principal may write it.
    pub state_key: String,
    pub introduced_in: String,
}
