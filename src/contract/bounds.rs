//! Queue bounds per principal and the staging and in-process budget checks.

use super::keys::Errors;
use super::model::Tables;
use super::rights::READ;
use super::schema::Kind;

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

/// Refuse budgets below their lower bounds, and a default burst of 0.
///
/// v2: the default burst checked.
/// @version 2
/// @req REQ-ING-TIV-007
/// @req REQ-ING-TIV-010
/// @req REQ-ING-TIV-019
pub fn check(tables: &Tables, errors: &mut Errors) {
    default_burst(tables, errors);
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
