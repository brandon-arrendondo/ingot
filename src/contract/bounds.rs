//! Queue bounds per principal and the staging and in-process budget checks.

use super::keys::Errors;
use super::log_streams::streams;
use super::model::Tables;
use super::rights::READ;
use super::schema::{Backing, Kind};

/// Milliseconds in a day: rates are writes per day, latency budgets are milliseconds.
const MS_PER_DAY: u64 = 86_400_000;

/// Queue bound of principal `p`: `max(min_queue, ceil(headroom_pct * SUM rate_per_day *
/// latency_budget_ms / (100 * 86_400_000)))` over the keys it may read.
///
/// @version 1
/// @req REQ-ING-TIV-007
pub fn queue_bound(tables: &Tables, p: usize) -> u32 {
    let rate: u64 = readable(tables, p)
        .map(|i| u64::from(tables.keys[i].rate_per_day))
        .sum();
    let numerator = u64::from(tables.bounds.headroom_pct)
        * rate
        * u64::from(tables.principals[p].latency_budget_ms);
    let bound = numerator.div_ceil(100 * MS_PER_DAY);
    u32::try_from(bound)
        .unwrap_or(u32::MAX)
        .max(tables.bounds.min_queue)
}

/// Indices of the keys principal `p` may read.
///
/// @version 1
/// @req REQ-ING-TIV-007
fn readable(tables: &Tables, p: usize) -> impl Iterator<Item = usize> + '_ {
    tables.rights[p]
        .iter()
        .enumerate()
        .filter(|(_, bits)| **bits & READ != 0)
        .map(|(i, _)| i)
}

/// Lower bounds of the in-process budgets: (snapshot bytes, queue bytes).
///
/// @version 1
/// @req REQ-ING-TIV-007
pub fn inproc_floors(tables: &Tables) -> (u64, u64) {
    let inproc: Vec<usize> = (0..tables.principals.len())
        .filter(|&p| tables.principals[p].kind == Kind::Inproc)
        .collect();
    let snapshot = (0..tables.keys.len())
        .filter(|&i| inproc.iter().any(|&p| tables.rights[p][i] & READ != 0))
        .map(|i| u64::from(tables.keys[i].max_size))
        .sum();
    let queue = inproc
        .iter()
        .map(|&p| {
            let widest = readable(tables, p)
                .map(|i| tables.keys[i].max_size)
                .max()
                .unwrap_or(0);
            u64::from(queue_bound(tables, p)) * u64::from(widest)
        })
        .sum();
    (snapshot, queue)
}

/// Refuse `[bounds] default_rate_burst = 0`: it is the burst of every write rate without its
/// own, and a burst of 0 would switch the store's rate limit off on each of them. Refused once,
/// whether or not any declaration uses the default.
///
/// @version 1
/// @req REQ-ING-TIV-019
fn default_burst(tables: &Tables, errors: &mut Errors) {
    if tables.bounds.default_rate_burst == 0 {
        errors.push(
            "bounds.default_rate_burst must be at least 1 (0 switches the write-rate limit off \
             on every write_rate without its own rate_burst)"
                .to_string(),
        );
    }
}

/// Refuse budgets below their lower bounds, log sums above their upper bounds, and a default
/// burst of 0.
///
/// v2: the default burst checked.
/// v3: the log budgets checked.
/// @version 3
/// @req REQ-ING-LOG-002
/// @req REQ-ING-LOG-003
/// @req REQ-ING-TIV-007
/// @req REQ-ING-TIV-010
/// @req REQ-ING-TIV-019
pub fn check(tables: &Tables, errors: &mut Errors) {
    default_burst(tables, errors);
    log_budgets(tables, errors);
    let staging = tables
        .principals
        .iter()
        .map(|p| p.staging_bound_bytes)
        .max()
        .unwrap_or(0);
    if tables.bounds.staging_total_bytes < staging {
        errors.push(format!(
            "bounds.staging_total_bytes {} is below the largest principal staging bound {staging}",
            tables.bounds.staging_total_bytes
        ));
    }
    let (snapshot, queue) = inproc_floors(tables);
    let floors = [
        (
            "inproc_snapshot_bytes",
            tables.bounds.inproc_snapshot_bytes,
            snapshot,
        ),
        (
            "inproc_queue_bytes",
            tables.bounds.inproc_queue_bytes,
            queue,
        ),
    ];
    for (name, have, need) in floors {
        if u64::from(have) < need {
            errors.push(format!(
                "bounds.{name} {have} is below its lower bound {need}"
            ));
        }
    }
}

/// The log stream sums of the tables' variant, which the `[bounds]` log budgets bound from above.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LogSums {
    /// Whether the variant has any log stream.
    pub any: bool,
    /// Whether any stream may write flash on the variant.
    pub any_flash: bool,
    /// RAM with every stream enabled: `ram_cap_bytes` of each stream that may be enabled.
    pub worst_ram: u64,
    /// RAM of the streams enabled at boot (boot backing not off).
    pub boot_ram: u64,
    /// Flash segments of the streams that may write flash: `flash_seg_bytes * flash_seg_count`.
    pub flash: u64,
    /// Their flash byte rates per day.
    pub flash_per_day: u64,
}

/// The log stream sums of the tables' variant (upper-bounded, unlike the in-process floors).
///
/// @version 1
/// @req REQ-ING-LOG-002
/// @req REQ-ING-LOG-003
/// @return the sums.
pub fn log_sums(tables: &Tables) -> LogSums {
    let v = tables.variant.as_str();
    let mut sums = LogSums::default();
    for (_, s) in streams(tables) {
        let (d, ram) = (&s.def, u64::from(s.def.ram_cap_bytes));
        sums.any = true;
        sums.worst_ram += if s.may_enable(v) { ram } else { 0 };
        sums.boot_ram += if s.backing(v) == Backing::Off { 0 } else { ram };
        if s.may_flash(v) {
            sums.any_flash = true;
            sums.flash += u64::from(d.flash_seg_bytes) * u64::from(d.flash_seg_count);
            sums.flash_per_day += u64::from(d.flash_rate_bytes_per_day);
        }
    }
    sums
}

/// Why `sum` breaks the upper bound `bounds.<name>` (`have`), if it does; `required`: a missing
/// bound is refused.
///
/// @version 1
/// @req REQ-ING-LOG-002
/// @req REQ-ING-LOG-003
/// @return the refusal, or None.
fn above(
    name: &str,
    have: Option<u32>,
    (sum, what): (u64, &str),
    required: bool,
) -> Option<String> {
    match have {
        None if required => Some(format!("log streams need bounds.{name}")),
        Some(b) if sum > u64::from(b) => {
            Some(format!("log streams: {what} is above bounds.{name} {b}"))
        }
        _ => None,
    }
}

/// Refuse log sums above their `[bounds]` upper bounds, and a missing bound the streams need.
///
/// @version 1
/// @req REQ-ING-LOG-002
/// @req REQ-ING-LOG-003
fn log_budgets(tables: &Tables, errors: &mut Errors) {
    let s = log_sums(tables);
    let b = &tables.bounds;
    let checks = [
        (
            "log_ram_budget_bytes",
            b.log_ram_budget_bytes,
            s.worst_ram,
            format!("worst-case RAM {} (every stream enabled)", s.worst_ram),
            s.any,
        ),
        (
            "log_ram_default_budget_bytes",
            b.log_ram_default_budget_bytes,
            s.boot_ram,
            format!("boot RAM {} (streams enabled at boot)", s.boot_ram),
            false,
        ),
        (
            "log_flash_bytes",
            b.log_flash_bytes,
            s.flash,
            format!("flash {}", s.flash),
            s.any_flash,
        ),
        (
            "log_flash_bytes_per_day",
            b.log_flash_bytes_per_day,
            s.flash_per_day,
            format!("flash {} per day", s.flash_per_day),
            s.any_flash,
        ),
    ];
    for (name, have, sum, what, required) in checks {
        errors.extend(above(name, have, (sum, &what), required));
    }
}
