/// Jenkins lookup3.c `final()` mixing function.
///
/// This is the hash used in the generated C code. It takes a 32-bit seed
/// and a 32-bit key and produces a 32-bit hash. The implementation must
/// match the C version exactly for cross-language compatibility.
///
/// Reference: Bob Jenkins, lookup3.c (public domain)
pub fn jenkins_hash(seed: u32, key: u32) -> u32 {
    let (mut a, mut b, mut c) = (key, seed, 0x9e3779b9u32);

    c ^= b;
    c = c.wrapping_sub(b.rotate_left(14));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(11));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(25));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(16));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(4));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(14));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(24));

    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test vectors generated from the Python reference implementation.
    /// These MUST match — any deviation means generated C code will
    /// produce wrong lookups at runtime.
    const TEST_VECTORS: &[(u32, u32, u32)] = &[
        (0x00000000, 0x00000000, 0x3fc64621),
        (0x00000000, 0x00000001, 0x60b47b7e),
        (0x00000001, 0x00000000, 0x5c01269c),
        (0xffffffff, 0xffffffff, 0xf42ae9f1),
        (0x12345678, 0xdeadbeef, 0x6808265d),
        (0x0000002a, 0x00000064, 0x06afa4b3),
        (0x9e3779b9, 0x00000000, 0x5ce543e9),
        (0x00000000, 0x9e3779b9, 0xccdf5301),
        (0x00000001, 0x00000001, 0xa1fe60a1),
        (0xcafebabe, 0x01020304, 0x16f52f96),
    ];

    #[test]
    fn matches_reference_implementation() {
        for &(seed, key, expected) in TEST_VECTORS {
            let actual = jenkins_hash(seed, key);
            assert_eq!(
                actual, expected,
                "jenkins_hash({seed:#010x}, {key:#010x}) = {actual:#010x}, expected {expected:#010x}"
            );
        }
    }

    #[test]
    fn deterministic() {
        let h1 = jenkins_hash(0, 0x12345678);
        let h2 = jenkins_hash(0, 0x12345678);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_seeds_differ() {
        let h1 = jenkins_hash(0, 0x12345678);
        let h2 = jenkins_hash(1, 0x12345678);
        assert_ne!(h1, h2);
    }

    #[test]
    fn different_keys_differ() {
        let h1 = jenkins_hash(0, 1);
        let h2 = jenkins_hash(0, 2);
        assert_ne!(h1, h2);
    }
}
