//! KSET v1 handshake digests (docs/contract-mode.rst), which the store core recomputes.

use super::model::Tables;
use super::rights::{READ, WRITE};
use sha2::{Digest, Sha256};

/// KSET v1 domain tag.
const TAG: &[u8; 16] = b"ingot.keyset.v1\0";

/// KSET v1 input bytes for `(id, type code)` pairs: tag, `u32le(count)`, then each pair
/// as `u32le(id) u8(type)` in ascending unsigned ID order.
///
/// @version 1
/// @req REQ-ING-TIV-008
pub fn kset_bytes(pairs: &[(u32, u8)]) -> Vec<u8> {
    let mut sorted = pairs.to_vec();
    sorted.sort_unstable();
    let mut bytes = TAG.to_vec();
    bytes.extend((sorted.len() as u32).to_le_bytes());
    for (id, ty) in sorted {
        bytes.extend(id.to_le_bytes());
        bytes.push(ty);
    }
    bytes
}

/// SHA-256 of the KSET v1 input for `pairs`.
///
/// @version 1
/// @req REQ-ING-TIV-008
pub fn kset(pairs: &[(u32, u8)]) -> [u8; 32] {
    Sha256::digest(kset_bytes(pairs)).into()
}

/// `dmt_digest[p][m]` for every principal and every minor `0..=contract minor`.
///
/// @version 1
/// @req REQ-ING-TIV-008
pub fn digests(tables: &Tables) -> Vec<Vec<[u8; 32]>> {
    (0..tables.principals.len())
        .map(|p| {
            (0..=tables.version.minor)
                .map(|m| kset(&subset(tables, p, m)))
                .collect()
        })
        .collect()
}

/// The (ID, type) pairs principal `p` may read or write as of minor `m`.
///
/// v2: shared with the SDK keyset.
/// @version 2
/// @req REQ-ING-TIV-008
/// @req REQ-ING-SDK-001
pub(super) fn subset(tables: &Tables, p: usize, m: u16) -> Vec<(u32, u8)> {
    tables
        .keys
        .iter()
        .zip(&tables.rights[p])
        .filter(|(k, bits)| {
            **bits & (READ | WRITE) != 0 && k.introduced.minor_in(tables.version.major) <= m
        })
        .map(|(k, _)| (k.id, k.ty.code()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::model::hex;

    // Golden vectors V0-V5 of the KSET v1 handshake digest.
    const KA: (u32, u8) = (0x0042_0000, 0);
    const KB: (u32, u8) = (0x0042_0090, 2);
    const KC: (u32, u8) = (0x0042_0140, 8);
    const KD: (u32, u8) = (0x0080_0018, 3);
    const KE: (u32, u8) = (0x0042_01B8, 7);
    const KF: (u32, u8) = (0x0080_00D0, 10);
    const KG: (u32, u8) = (0x0042_0210, 2);

    fn digest_hex(pairs: &[(u32, u8)]) -> String {
        hex(&kset(pairs))
    }

    #[test]
    fn v0_empty() {
        assert_eq!(
            hex(&kset_bytes(&[])),
            "696e676f742e6b65797365742e76310000000000"
        );
        assert_eq!(
            digest_hex(&[]),
            "ae2b3b072b8afe5aee8c9f55514cf2132e3c31438a4221ae78750dc75cb35e93"
        );
    }

    #[test]
    fn v1_p1_minor0_input_order_does_not_matter() {
        assert_eq!(
            digest_hex(&[KD, KA, KC, KB]),
            "cf148898c69117866ff681d87440b3450f14152af306fbf93cbc6ee70332a02e"
        );
    }

    #[test]
    fn v2_p1_minor1_interleaves_namespaces() {
        assert_eq!(
            hex(&kset_bytes(&[KD, KA, KG, KC, KF, KB, KE])),
            "696e676f742e6b65797365742e76310007000000000042000090004200024001420008\
             b80142000710024200021800800003d00080000a"
        );
        assert_eq!(
            digest_hex(&[KD, KA, KG, KC, KF, KB, KE]),
            "a38693b5c330a9174fafad98c6c328501ccd2214e7b52e6a11f7fe8f7484993c"
        );
    }

    #[test]
    fn v3_p2_minor1() {
        assert_eq!(
            digest_hex(&[KA, KB, KD, KE, KG]),
            "7fad00a7d007bf9636617c40436d19688db25d966b452686c6eaffea17405916"
        );
    }

    #[test]
    fn v4_single_key() {
        assert_eq!(
            digest_hex(&[KA]),
            "37062e188658d3d03b052b9f9ed6cec0962a7f153f455cc218d35b12dbcad449"
        );
    }

    #[test]
    fn v5_type_change_changes_the_digest() {
        assert_eq!(
            digest_hex(&[KA, (KB.0, 3), KC, KD]),
            "c6ff658ee1b12ffedeb16c97dfc1f7b6038ca892e5a146791933d4e1aa129130"
        );
    }
}
