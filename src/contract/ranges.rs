//! Units, ranges, discrete sets, sentinel defaults and scale of a declared key. They
//! are tooling and binder inputs: the store tables do not carry them.

use super::keys::{Errors, Key};
use super::schema::{KeyDef, UnitDef};
use super::value::int_range;
use std::collections::BTreeMap;

/// An inclusive range on the step grid, in wire units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Range {
    pub min: i64,
    pub max: i64,
    pub step: i64,
}

impl Range {
    /// Whether `v` is in the range and on its grid.
    ///
    /// @version 1
    /// @req REQ-ING-RNG-001
    pub fn contains(&self, v: i64) -> bool {
        (self.min..=self.max).contains(&v) && (v - self.min) % self.step == 0
    }
}

/// The unit vocabulary, when the model declares one.
pub type Units = BTreeMap<String, UnitDef>;

/// Resolve and check the key's unit, range, allowed set, sentinel and scale from `def`.
///
/// @version 1
/// @req REQ-ING-RNG-001
pub fn apply(key: &mut Key, def: &KeyDef, units: Option<&Units>, errors: &mut Errors) {
    let mut refuse = |why: String| errors.push(format!("{}: {why}", key.path));
    check_unit(def, units, &mut refuse);
    let numeric = int_range(key.ty).filter(|_| key.enum_values.is_none());
    let has_limits = def.min.is_some() || def.max.is_some() || def.step.is_some();
    let constrained = has_limits || def.allowed.is_some() || def.unset.is_some();
    match numeric {
        None if constrained || def.scale.is_some() => {
            refuse("range, allowed, unset and scale are for integer keys without an enum".into())
        }
        None => {}
        Some((lo, hi, _)) => {
            key.range = range_of(def, (lo, hi), &mut refuse);
            key.allowed = allowed_of(def, has_limits, (lo, hi), &mut refuse);
            key.scale = def.scale.unwrap_or(1.0);
            key.unset = def.unset;
            check_defaults(key, def, &mut refuse);
        }
    }
}

/// The unit is a token of the vocabulary, and a scale other than 1 names its unit.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn check_unit(def: &KeyDef, units: Option<&Units>, refuse: &mut impl FnMut(String)) {
    if let (Some(unit), Some(units)) = (&def.unit, units) {
        if !units.contains_key(unit) {
            refuse(format!("unit {unit} is not in the unit vocabulary"));
        }
    }
    if let Some(scale) = def.scale {
        if !(scale.is_finite() && scale > 0.0) {
            refuse(format!("scale {scale} must be a positive finite number"));
        }
        if scale != 1.0 && def.unit.is_none() {
            refuse("a scaled key names its displayed unit".into());
        }
    }
}

/// The range from min/max/step: both bounds given, inside the type, on a grid of a positive
/// step that divides the span.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn range_of(def: &KeyDef, (lo, hi): (i64, i64), refuse: &mut impl FnMut(String)) -> Option<Range> {
    let (Some(min), Some(max)) = (def.min, def.max) else {
        if def.min.is_some() || def.max.is_some() || def.step.is_some() {
            refuse("a range needs both min and max".into());
        }
        return None;
    };
    let step = def.step.unwrap_or(1);
    let ok = lo <= min && min <= max && max <= hi && step > 0 && (max - min) % step == 0;
    if !ok {
        refuse(format!(
            "range {min}..{max} step {step} must lie in {lo}..{hi}, with min <= max and a positive step dividing the span"
        ));
    }
    ok.then_some(Range { min, max, step })
}

/// The allowed set: not combined with a range, non-empty, unique, inside the type.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn allowed_of(
    def: &KeyDef,
    has_limits: bool,
    (lo, hi): (i64, i64),
    refuse: &mut impl FnMut(String),
) -> Option<Vec<i64>> {
    let values = def.allowed.as_ref()?;
    let mut sorted = values.clone();
    sorted.sort_unstable();
    sorted.dedup();
    let ok = !has_limits
        && !values.is_empty()
        && sorted.len() == values.len()
        && values.iter().all(|v| (lo..=hi).contains(v));
    if !ok {
        refuse(
            "allowed must be a non-empty set of distinct values of the type, without min/max/step"
                .into(),
        );
    }
    Some(sorted)
}

/// Whether the key's range or allowed set accepts `v` (false when it has neither).
///
/// @version 1
/// @req REQ-ING-RNG-001
fn accepts(key: &Key, v: i64) -> bool {
    key.range.is_some_and(|r| r.contains(v)) || key.allowed.as_ref().is_some_and(|a| a.contains(&v))
}

/// Every default (base and per variant) is accepted by the range or set, unless it is the
/// `unset` sentinel; the sentinel itself equals the base default and lies outside.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn check_defaults(key: &Key, def: &KeyDef, refuse: &mut impl FnMut(String)) {
    let limited = key.range.is_some() || key.allowed.is_some();
    if let Some(unset) = key.unset {
        let base = def.default.as_ref().and_then(toml::Value::as_integer);
        if !limited || accepts(key, unset) || base != Some(unset) {
            refuse(format!(
                "unset {unset} must equal the default and lie outside the range or allowed set"
            ));
        }
    }
    let defaults = def.default.iter().chain(def.defaults.values());
    for v in defaults.filter_map(toml::Value::as_integer) {
        if limited && !accepts(key, v) && Some(v) != key.unset {
            refuse(format!("default {v} is outside the range or allowed set"));
        }
    }
}
