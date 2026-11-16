//! Struct value headers: `header = [{ name, type }, ...]` on a struct key declares the fixed
//! header its value starts with, every field little-endian and packed (a bool one byte, 0 or 1),
//! then the payload. The store stays layout-blind: a header changes no table and no digest, only
//! contract.json, dmt_constants.h (constants, a native struct and its byte-shift codec) and the
//! SDK binding context.

use super::constants::{c_identifier, enum_constants, NAME_MAX};
use super::keys::{Errors, Key};
use super::model::Tables;
use super::schema::{HeaderFieldDef, KeyType};
use super::sdk::scalar;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The types a header field may have.
const FIELD_TYPES: [KeyType; 7] = [
    KeyType::Bool,
    KeyType::Uint8,
    KeyType::Int8,
    KeyType::Uint16,
    KeyType::Int16,
    KeyType::Uint32,
    KeyType::Int32,
];

/// One resolved header field, as contract.json carries it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: KeyType,
    pub offset: u32,
    pub size: u32,
}

/// The header's byte count: the end of its last field.
///
/// @version 1
/// @req REQ-ING-HDR-001
pub fn header_bytes(fields: &[HeaderField]) -> u32 {
    fields.last().map_or(0, |f| f.offset + f.size)
}

/// A key's header in contract.json, written flat beside the other key fields; both absent
/// without a header (and in snapshots from before headers).
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct HeaderJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Vec<HeaderField>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_bytes: Option<u32>,
}

impl HeaderJson {
    /// The header fields and byte count of key `k`.
    ///
    /// @version 1
    /// @req REQ-ING-HDR-001
    pub fn of(k: &Key) -> Self {
        HeaderJson {
            header: k.header.clone(),
            header_bytes: k.header.as_deref().map(header_bytes),
        }
    }
}

/// The fields of `defs` laid out in order with no padding.
///
/// @version 1
/// @req REQ-ING-HDR-001
fn layout(defs: &[HeaderFieldDef]) -> Vec<HeaderField> {
    let mut offset = 0;
    defs.iter()
        .map(|d| {
            let size = d.ty.scalar_size().unwrap_or(0);
            let field = HeaderField {
                name: d.name.clone(),
                ty: d.ty,
                offset,
                size,
            };
            offset += size;
            field
        })
        .collect()
}

/// What is wrong with the fields themselves: an empty list, a type outside `FIELD_TYPES`, a name
/// that is not a C identifier or is repeated.
///
/// @version 1
/// @req REQ-ING-HDR-001
fn field_refusals(defs: &[HeaderFieldDef]) -> Vec<String> {
    let mut out = Vec::new();
    if defs.is_empty() {
        out.push("has no field".to_string());
    }
    for (i, d) in defs.iter().enumerate() {
        if !FIELD_TYPES.contains(&d.ty) {
            out.push(format!(
                "field {}: type {} is not one of bool, uint8, int8, uint16, int16, uint32, int32",
                d.name,
                d.ty.name()
            ));
        }
        if !c_identifier(&d.name) {
            out.push(format!("field {} is not a C identifier", d.name));
        }
        if defs[..i].iter().any(|e| e.name == d.name) {
            out.push(format!("field {} is declared twice", d.name));
        }
    }
    out
}

/// What is wrong with a header on `key`: it is not a struct key, its fields, or the header
/// leaves no payload byte in max_size.
///
/// @version 1
/// @req REQ-ING-HDR-001
fn refusals(key: &Key, defs: &[HeaderFieldDef], fields: &[HeaderField]) -> Vec<String> {
    let mut out = field_refusals(defs);
    let bytes = header_bytes(fields);
    if key.ty != KeyType::Struct {
        out.push(format!(
            "on a {} key: only a struct key has a header",
            key.ty.name()
        ));
    } else if bytes >= key.max_size {
        out.push(format!(
            "of {bytes} bytes leaves no payload in max_size {}",
            key.max_size
        ));
    }
    out
}

/// The resolved header of `key` (declared `defs`), `None` when it declares none or is refused;
/// each refusal goes to `errors`. `key` is sized already.
///
/// @version 1
/// @req REQ-ING-HDR-001
pub fn resolve(
    key: &Key,
    defs: Option<&[HeaderFieldDef]>,
    errors: &mut Errors,
) -> Option<Vec<HeaderField>> {
    let defs = defs?;
    let fields = layout(defs);
    let problems = refusals(key, defs, &fields);
    for p in &problems {
        errors.push(format!("{}: header {p}", key.path));
    }
    problems.is_empty().then_some(fields)
}

/// One header field as the generated C names and reads it.
#[derive(Debug, Serialize)]
pub struct FieldView {
    pub name: String,
    /// `<PREFIX>_<FIELD>_OFFSET`
    pub offset_name: String,
    pub ctype: &'static str,
    pub offset: u32,
    pub size: u32,
    /// `bool`, `unsigned` or `signed`.
    pub kind: &'static str,
    /// The C expression of the field's little-endian bytes at `in` as a `uint32_t`.
    pub word: String,
    /// The C expression converting that word `u` to the field's type (two's complement for a
    /// signed type, without an implementation-defined conversion).
    pub value: String,
}

/// One key's header as dmt_constants.h and the SDK binding context name it.
#[derive(Debug, Serialize)]
pub struct HeaderView {
    pub path: String,
    /// `DMT_<KEY PATH>`, the prefix of its constants.
    pub prefix: String,
    /// `dmt_<key path>_header_t`
    pub ctype: String,
    /// `dmt_<key path>_header_encode`
    pub encode: String,
    /// `dmt_<key path>_header_decode`
    pub decode: String,
    pub bytes: u32,
    /// max_size minus the header bytes.
    pub payload_max: u32,
    pub fields: Vec<FieldView>,
}

/// The kind of a header field's type: `bool`, `signed` or `unsigned`.
///
/// @version 1
/// @req REQ-ING-HDR-002
fn kind(ty: KeyType) -> &'static str {
    match ty {
        KeyType::Bool => "bool",
        KeyType::Int8 | KeyType::Int16 | KeyType::Int32 => "signed",
        _ => "unsigned",
    }
}

/// The C expression assembling `size` little-endian bytes at `in[offset]` into a `uint32_t`.
///
/// @version 1
/// @req REQ-ING-HDR-002
fn word(offset: u32, size: u32) -> String {
    (0..size)
        .map(|i| match i {
            0 => format!("(uint32_t)in[{offset}]"),
            _ => format!("((uint32_t)in[{}] << {})", offset + i, 8 * i),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The C expression converting the word `u` of a field to its `ctype`: a cast for an unsigned
/// type, and for a signed one the two's-complement value through `int64_t`, in range before it
/// is narrowed.
///
/// @version 1
/// @req REQ-ING-HDR-002
fn value(ty: KeyType, ctype: &str, size: u32) -> String {
    let bits = 8 * size;
    match kind(ty) {
        "signed" => format!(
            "({ctype})((int64_t)u - (int64_t)(u >> {}) * INT64_C({}))",
            bits - 1,
            1u64 << bits
        ),
        _ => format!("({ctype})u"),
    }
}

/// The view of one field of a header whose constants start with `prefix`.
///
/// @version 1
/// @req REQ-ING-HDR-002
fn field_view(prefix: &str, f: &HeaderField) -> FieldView {
    let ctype = scalar(f.ty).map_or("", |(c, _, _)| c);
    FieldView {
        name: f.name.clone(),
        offset_name: format!("{prefix}_{}_OFFSET", f.name.to_uppercase()),
        ctype,
        offset: f.offset,
        size: f.size,
        kind: kind(f.ty),
        word: word(f.offset, f.size),
        value: value(f.ty, ctype, f.size),
    }
}

/// The view of `key`'s header, when it has one.
///
/// @version 1
/// @req REQ-ING-HDR-002
pub fn view(key: &Key) -> Option<HeaderView> {
    let fields = key.header.as_ref()?;
    let ident = key.path.replace('.', "_");
    let prefix = format!("DMT_{}", ident.to_uppercase());
    let lower = format!("dmt_{}_header", ident.to_lowercase());
    let bytes = header_bytes(fields);
    Some(HeaderView {
        path: key.path.clone(),
        ctype: format!("{lower}_t"),
        encode: format!("{lower}_encode"),
        decode: format!("{lower}_decode"),
        bytes,
        payload_max: key.max_size - bytes,
        fields: fields.iter().map(|f| field_view(&prefix, f)).collect(),
        prefix,
    })
}

/// Every header of the tables' keys, in key ID order.
///
/// @version 1
/// @req REQ-ING-HDR-002
pub fn views(tables: &Tables) -> Vec<HeaderView> {
    tables.keys.iter().filter_map(view).collect()
}

/// The names of a header view that the name checks need: its constants and its decode function.
/// The decode name stands for the encode name (as long, and neither can equal the other's form)
/// and the type name (shorter); a lower-case name collides only where the key's
/// `_HEADER_BYTES` does, and never with an upper-case one.
///
/// @version 1
/// @req REQ-ING-HDR-002
fn names(h: &HeaderView) -> Vec<String> {
    let mut all = vec![
        format!("{}_HEADER_BYTES", h.prefix),
        format!("{}_PAYLOAD_MAX", h.prefix),
        h.decode.clone(),
    ];
    all.extend(h.fields.iter().map(|f| f.offset_name.clone()));
    all
}

/// Every header name is a C identifier of at most `NAME_MAX` characters that no other header
/// name or enum constant of dmt_constants.h has.
///
/// @version 1
/// @req REQ-ING-HDR-002
pub fn check(tables: &Tables, errors: &mut Errors) {
    let mut owner: BTreeMap<String, String> = enum_constants(tables)
        .into_iter()
        .map(|c| (c.name, c.key))
        .collect();
    for h in views(tables) {
        for name in names(&h) {
            let mut refuse =
                |why: String| errors.push(format!("{}: header name {name} {why}", h.path));
            if !c_identifier(&name) {
                refuse("is not a C identifier".into());
            } else if name.len() > NAME_MAX {
                refuse(format!("is longer than {NAME_MAX} characters"));
            }
            match owner.get(&name) {
                Some(other) => refuse(format!("is also {other}'s")),
                None => {
                    owner.insert(name.clone(), h.path.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_little_endian_and_a_signed_value_is_two_s_complement() {
        assert_eq!(word(3, 1), "(uint32_t)in[3]");
        assert_eq!(word(4, 2), "(uint32_t)in[4] | ((uint32_t)in[5] << 8)");
        assert_eq!(value(KeyType::Uint16, "uint16_t", 2), "(uint16_t)u");
        assert_eq!(
            value(KeyType::Int8, "int8_t", 1),
            "(int8_t)((int64_t)u - (int64_t)(u >> 7) * INT64_C(256))"
        );
        assert_eq!(
            value(KeyType::Int32, "int32_t", 4),
            "(int32_t)((int64_t)u - (int64_t)(u >> 31) * INT64_C(4294967296))"
        );
    }
}
