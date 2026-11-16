// Storage code generation modules — one per data type family.
//
// Each module generates the C source and header for its storage type,
// including perfect hash tables, accessor functions, and initialization.

pub mod boolean;
pub mod integer;
pub mod persistence;
pub mod string;

/// Smallest signed C integer type that holds every perfect-hash G value.
///
/// Shared by the boolean, integer and string storage groups, which each emit a G table.
///
/// @version 1
/// @req REQ-ING-GEN-004
/// @req REQ-ING-GEN-005
/// @req REQ-ING-GEN-006
pub(crate) fn select_g_table_type(g: &[i32]) -> &'static str {
    let min = g.iter().copied().min().unwrap_or(0);
    let max = g.iter().copied().max().unwrap_or(0);

    if min >= i8::MIN as i32 && max <= i8::MAX as i32 {
        "int8_t"
    } else if min >= i16::MIN as i32 && max <= i16::MAX as i32 {
        "int16_t"
    } else {
        "int32_t"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_g_type_fits_i8() {
        assert_eq!(select_g_table_type(&[0, 1, -1, 127, -128]), "int8_t");
    }

    #[test]
    fn select_g_type_fits_i16() {
        assert_eq!(select_g_table_type(&[0, 128, -129]), "int16_t");
    }

    #[test]
    fn select_g_type_needs_i32() {
        assert_eq!(select_g_table_type(&[0, 40000, -40000]), "int32_t");
    }

    #[test]
    fn select_g_type_of_empty_table() {
        assert_eq!(select_g_table_type(&[]), "int8_t");
    }
}
