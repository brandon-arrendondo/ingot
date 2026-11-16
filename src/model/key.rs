use super::schema::{Class, KeyDef, STAMPED};

/// 32-bit key encoding for the data model.
///
/// Bit layout:
/// ```text
/// | Namespace | Class   | ID      | Type    | Thread  | Derived | Read    |
/// | (10 bits) | (5 bits)| (10 bit)| (4 bit) | Safe(1) | (1 bit) | Only(1) |
/// | 31-22     | 21-17   | 16-7    | 6-3     | 2       | 1       | 0       |
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEncoding {
    pub namespace: u16,
    pub class: u8,
    pub id: u16,
    pub data_type: u8,
    pub thread_safe: bool,
    pub derived: bool,
    pub read_only: bool,
}

impl KeyEncoding {
    /// Pack the fields into the 32-bit key ID.
    ///
    /// @version 1
    /// @req REQ-ING-KEY-001
    pub fn encode(&self) -> u32 {
        let mut val: u32 = 0;
        val |= (self.namespace as u32 & 0x3FF) << 22;
        val |= (self.class as u32 & 0x1F) << 17;
        val |= (self.id as u32 & 0x3FF) << 7;
        val |= (self.data_type as u32 & 0xF) << 3;
        if self.thread_safe {
            val |= 1 << 2;
        }
        if self.derived {
            val |= 1 << 1;
        }
        if self.read_only {
            val |= 1;
        }
        val
    }
}

/// Encoded 32-bit ID of `key` in `class`: the class's namespace ID (or `fallback_ns` for a
/// single-file model), its class index, the key's index, type code and thread-safe and
/// read-only flags. Indices are the explicit ones or the positions DataModel::from_toml stamped.
///
/// @version 1
/// @req REQ-ING-KEY-001
/// @req REQ-ING-KEY-002
pub fn key_id(class: &Class, fallback_ns: u16, key: &KeyDef) -> u32 {
    KeyEncoding {
        namespace: class.namespace_id.unwrap_or(fallback_ns),
        class: class.class_index.expect(STAMPED),
        id: key.key_index.expect(STAMPED),
        data_type: key.data_type.type_code(),
        thread_safe: key.thread_safe,
        derived: false,
        read_only: key.read_only,
    }
    .encode()
}

/// `<NAMESPACE>_<CLASS>_<KEY>`, upper-case with spaces as underscores: the stem of a key's
/// `DM_KEY_` define, its helper names and its manifest and key-list name.
///
/// @version 1
/// @req REQ-ING-GEN-002
/// @req REQ-ING-FLT-001
pub fn define_stem(namespace: &str, class: &str, key: &str) -> String {
    format!(
        "{}_{}_{}",
        namespace.to_uppercase(),
        class.to_uppercase().replace(' ', "_"),
        key.to_uppercase().replace(' ', "_"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_stem_upper_cases_and_joins() {
        assert_eq!(
            define_stem("battery", "status", "state of charge"),
            "BATTERY_STATUS_STATE_OF_CHARGE"
        );
        assert_eq!(define_stem("rig", "my class", "x"), "RIG_MY_CLASS_X");
    }

    #[test]
    fn fields_land_in_their_bits() {
        let key = KeyEncoding {
            namespace: 5,
            class: 3,
            id: 42,
            data_type: 2,
            thread_safe: true,
            derived: false,
            read_only: true,
        };
        // 5<<22 | 3<<17 | 42<<7 | 2<<3 | thread-safe (bit 2) | read-only (bit 0)
        assert_eq!(key.encode(), 0x0146_1515);
    }

    #[test]
    fn max_values() {
        let key = KeyEncoding {
            namespace: 1023,
            class: 31,
            id: 1023,
            data_type: 15,
            thread_safe: true,
            derived: true,
            read_only: true,
        };
        let encoded = key.encode();
        assert_eq!(encoded, 0xFFFFFFFF);
    }

    #[test]
    fn zero_key() {
        let key = KeyEncoding {
            namespace: 0,
            class: 0,
            id: 0,
            data_type: 0,
            thread_safe: false,
            derived: false,
            read_only: false,
        };
        assert_eq!(key.encode(), 0);
    }
}
