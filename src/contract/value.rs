//! Default values in the table interface's value encoding, and write rates.

use super::enums::ResolvedEnum;
use super::schema::KeyType;

/// Encode `value` as a default of a key of type `ty` (little-endian scalars, one-byte bool,
/// IEEE-754 float32, string and binary bytes); `enum_values`, when given, must contain it.
///
/// v2: the enum is the variant-resolved enum.
/// @version 2
/// @req REQ-ING-TIV-004
/// @req REQ-ING-TIV-010
pub fn encode_default(
    ty: KeyType,
    value: &toml::Value,
    max_size: u32,
    enum_values: Option<&ResolvedEnum>,
) -> Result<Vec<u8>, String> {
    let bytes = match ty {
        KeyType::Bool => encode_bool(value),
        KeyType::Float32 => encode_float(value),
        KeyType::String => encode_string(value),
        KeyType::Binary => encode_binary(value),
        KeyType::Struct => Err("a struct key has no default".to_string()),
        _ => encode_int(ty, value, enum_values),
    }?;
    if bytes.len() as u64 > u64::from(max_size) {
        return Err(format!(
            "default is {} bytes, max_size is {max_size}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// One byte, 0 or 1, from a TOML boolean.
///
/// @version 1
/// @req REQ-ING-TIV-004
fn encode_bool(value: &toml::Value) -> Result<Vec<u8>, String> {
    value
        .as_bool()
        .map(|b| vec![u8::from(b)])
        .ok_or_else(|| format!("default {value} is not a boolean"))
}

/// IEEE-754 binary32 little-endian from a TOML float or integer that is finite as float32.
///
/// v2: one finiteness check: a non-finite f64 is non-finite as f32 too.
/// @version 2
/// @req REQ-ING-TIV-004
fn encode_float(value: &toml::Value) -> Result<Vec<u8>, String> {
    let number = value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64));
    match number {
        Some(f) if (f as f32).is_finite() => Ok((f as f32).to_le_bytes().to_vec()),
        _ => Err(format!("default {value} is not a finite float32")),
    }
}

/// The string's UTF-8 bytes, without a terminator.
///
/// @version 1
/// @req REQ-ING-TIV-004
fn encode_string(value: &toml::Value) -> Result<Vec<u8>, String> {
    value
        .as_str()
        .map(|s| s.as_bytes().to_vec())
        .ok_or_else(|| format!("default {value} is not a string"))
}

/// Bytes from a TOML array of integers 0..=255.
///
/// @version 1
/// @req REQ-ING-TIV-004
fn encode_binary(value: &toml::Value) -> Result<Vec<u8>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("default {value} is not an array of bytes"))?;
    items
        .iter()
        .map(|item| {
            item.as_integer()
                .and_then(|i| u8::try_from(i).ok())
                .ok_or_else(|| format!("binary default element {item} is not a byte"))
        })
        .collect()
}

/// Little-endian bytes of an in-range integer default.
///
/// v2: membership in the variant-resolved enum.
/// @version 2
/// @req REQ-ING-TIV-004
fn encode_int(
    ty: KeyType,
    value: &toml::Value,
    enum_values: Option<&ResolvedEnum>,
) -> Result<Vec<u8>, String> {
    let n = value
        .as_integer()
        .ok_or_else(|| format!("default {value} is not an integer"))?;
    if enum_values.is_some_and(|values| !values.values().any(|m| m.value == n)) {
        return Err(format!("default {n} is not a value of the key's enum"));
    }
    int_bytes(ty, n).ok_or_else(|| format!("default {n} is out of range for {}", ty.name()))
}

/// `(min, max, width in bytes)` of an integer type.
///
/// v2: visible to the range checks.
/// @version 2
/// @req REQ-ING-TIV-004
pub(super) fn int_range(ty: KeyType) -> Option<(i64, i64, usize)> {
    match ty {
        KeyType::Uint8 => Some((0, u8::MAX.into(), 1)),
        KeyType::Int8 => Some((i8::MIN.into(), i8::MAX.into(), 1)),
        KeyType::Uint16 => Some((0, u16::MAX.into(), 2)),
        KeyType::Int16 => Some((i16::MIN.into(), i16::MAX.into(), 2)),
        KeyType::Uint32 => Some((0, u32::MAX.into(), 4)),
        KeyType::Int32 => Some((i32::MIN.into(), i32::MAX.into(), 4)),
        _ => None,
    }
}

/// `n` as the little-endian bytes of `ty`, or `None` when out of range. In range, the low
/// `width` bytes of the two's-complement `i64` are exactly the type's encoding.
///
/// @version 1
/// @req REQ-ING-TIV-004
fn int_bytes(ty: KeyType, n: i64) -> Option<Vec<u8>> {
    let (min, max, width) = int_range(ty)?;
    (min..=max)
        .contains(&n)
        .then(|| n.to_le_bytes()[..width].to_vec())
}

/// Writes per day from `"<n>/s"`, `"<n>/min"`, `"<n>/h"` or `"<n>/day"`.
///
/// @version 1
/// @req REQ-ING-TIV-007
pub fn parse_rate(text: &str) -> Result<u32, String> {
    let bad = || format!("write rate '{text}' is not \"<n>/s|min|h|day\" within u32 per day");
    let (count, unit) = text.split_once('/').ok_or_else(bad)?;
    let per_unit: u64 = match unit {
        "s" => 86_400,
        "min" => 1_440,
        "h" => 24,
        "day" => 1,
        _ => return Err(bad()),
    };
    let count: u64 = count.trim().parse().map_err(|_| bad())?;
    u32::try_from(count * per_unit).map_err(|_| bad())
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    #[test]
    fn scalars_encode_little_endian() {
        let v = |i| Value::Integer(i);
        assert_eq!(
            encode_default(KeyType::Uint16, &v(0x1234), 2, None),
            Ok(vec![0x34, 0x12])
        );
        assert_eq!(
            encode_default(KeyType::Int8, &v(-1), 1, None),
            Ok(vec![0xFF])
        );
        assert_eq!(
            encode_default(KeyType::Int32, &v(-2), 4, None),
            Ok(vec![0xFE, 0xFF, 0xFF, 0xFF])
        );
        assert_eq!(
            encode_default(KeyType::Bool, &Value::Boolean(true), 1, None),
            Ok(vec![1])
        );
        assert_eq!(
            encode_default(KeyType::Float32, &Value::Float(1.5), 4, None),
            Ok(1.5f32.to_le_bytes().to_vec())
        );
    }

    #[test]
    fn every_integer_type_takes_its_full_range_at_its_width() {
        let v = |i| Value::Integer(i);
        let cases: [(KeyType, i64, i64, usize); 6] = [
            (KeyType::Uint8, 0, 0xFF, 1),
            (KeyType::Int8, -0x80, 0x7F, 1),
            (KeyType::Uint16, 0, 0xFFFF, 2),
            (KeyType::Int16, -0x8000, 0x7FFF, 2),
            (KeyType::Uint32, 0, 0xFFFF_FFFF, 4),
            (KeyType::Int32, -0x8000_0000, 0x7FFF_FFFF, 4),
        ];
        for (ty, min, max, width) in cases {
            let at = |n: i64| encode_default(ty, &v(n), 4, None);
            assert_eq!(
                at(max),
                Ok(max.to_le_bytes()[..width].to_vec()),
                "{ty:?} max"
            );
            assert_eq!(
                at(min),
                Ok(min.to_le_bytes()[..width].to_vec()),
                "{ty:?} min"
            );
            assert!(
                at(max + 1).is_err() && at(min - 1).is_err(),
                "{ty:?} out of range"
            );
        }
    }

    #[test]
    fn buffers_encode_their_bytes() {
        let s = Value::String("ab".into());
        assert_eq!(
            encode_default(KeyType::String, &s, 4, None),
            Ok(b"ab".to_vec())
        );
        let b = Value::Array(vec![Value::Integer(1), Value::Integer(255)]);
        assert_eq!(
            encode_default(KeyType::Binary, &b, 2, None),
            Ok(vec![1, 255])
        );
    }

    #[test]
    fn bad_defaults_are_refused() {
        let v = |i| Value::Integer(i);
        assert!(encode_default(KeyType::Uint8, &v(256), 1, None).is_err());
        assert!(encode_default(KeyType::Uint16, &v(-1), 2, None).is_err());
        assert!(encode_default(KeyType::String, &Value::String("abc".into()), 2, None).is_err());
        assert!(encode_default(KeyType::Float32, &Value::Float(f64::NAN), 4, None).is_err());
        assert!(encode_default(KeyType::Float32, &Value::Float(1e300), 4, None).is_err());
        assert!(encode_default(KeyType::Bool, &v(1), 1, None).is_err());
        assert!(encode_default(KeyType::Struct, &v(0), 8, None).is_err());
        let wide = Value::Array(vec![Value::Integer(256)]);
        assert!(encode_default(KeyType::Binary, &wide, 4, None).is_err());
        let member = |value| crate::contract::enums::Member {
            value,
            wire: String::new(),
            accept: Vec::new(),
            explicit: false,
            report_as: None,
        };
        let enums: ResolvedEnum = [
            ("off".to_string(), member(0)),
            ("on".to_string(), member(1)),
        ]
        .into_iter()
        .collect();
        assert!(encode_default(KeyType::Uint8, &v(2), 1, Some(&enums)).is_err());
        assert_eq!(
            encode_default(KeyType::Uint8, &v(1), 1, Some(&enums)),
            Ok(vec![1])
        );
    }

    #[test]
    fn rates_become_writes_per_day() {
        assert_eq!(parse_rate("10/s"), Ok(864_000));
        assert_eq!(parse_rate("2/min"), Ok(2_880));
        assert_eq!(parse_rate("3/h"), Ok(72));
        assert_eq!(parse_rate("5/day"), Ok(5));
        assert_eq!(parse_rate(" 10 /s"), Ok(864_000), "spaces around the count");
        assert!(parse_rate("1/week").is_err());
        assert!(parse_rate("x/s").is_err());
        assert!(parse_rate("50000/s").is_err(), "4.32e9 per day exceeds u32");
    }
}
