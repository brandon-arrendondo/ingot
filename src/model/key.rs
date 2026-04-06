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

    pub fn decode(val: u32) -> Self {
        Self {
            namespace: ((val >> 22) & 0x3FF) as u16,
            class: ((val >> 17) & 0x1F) as u8,
            id: ((val >> 7) & 0x3FF) as u16,
            data_type: ((val >> 3) & 0xF) as u8,
            thread_safe: (val >> 2) & 1 == 1,
            derived: (val >> 1) & 1 == 1,
            read_only: val & 1 == 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encoding() {
        let key = KeyEncoding {
            namespace: 5,
            class: 3,
            id: 42,
            data_type: 2,
            thread_safe: true,
            derived: false,
            read_only: true,
        };
        let encoded = key.encode();
        let decoded = KeyEncoding::decode(encoded);
        assert_eq!(key, decoded);
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
        let decoded = KeyEncoding::decode(encoded);
        assert_eq!(key, decoded);
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
