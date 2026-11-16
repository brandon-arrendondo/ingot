//! `dtp_payload`: the members of each pub topic's payload, as reporter rows grouped by topic in
//! topic order, each topic's members in emission order (contiguous object prefixes).

use super::super::model::Tables;
use super::super::payload::key_of;
use super::super::schema::{Constant, PathForm, PayloadRow, Scope};
use super::reported::{constant_json, key_row, other, placement, tree_order};
use super::Pools;

/// One member's row: its key's value (the member's form and unknown rule, nothing else of the
/// key's shadow mapping) or its constant's JSON text.
///
/// @version 1
/// @req REQ-ING-TOP-003
fn member(tables: &Tables, r: &PayloadRow, hash: &str, pools: &mut Pools) -> String {
    match key_of(tables, r) {
        Some(k) => {
            let mut key = k.clone();
            key.wire = Default::default();
            key.wire.outbound_unknown = r.outbound_unknown.clone();
            let f = PathForm {
                path: r.path.clone(),
                form: r.form,
                scope: Some(Scope::Full),
                variants: None,
            };
            key_row(tables, &key, &f, pools)
        }
        None => {
            let c = Constant {
                path: r.path.clone(),
                value: r.value.clone(),
                image: r.image,
                forced: false,
            };
            let json = constant_json(tables, &c, hash);
            let placed = placement(true, false, false);
            other(&r.path, "DSH_RK_CONSTANT", Some(&json), 0, &placed)
        }
    }
}

/// The `dtp_payload` rows, and each topic's `.payload_first` / `.payload_count` in topic
/// order.
///
/// @version 1
/// @req REQ-ING-TOP-003
pub fn rows(tables: &Tables, hash: &str, pools: &mut Pools) -> (Vec<String>, Vec<String>) {
    let mut rows = Vec::new();
    let mut slices = Vec::new();
    for t in &tables.shadow.topics {
        let mine: Vec<(String, String)> = tables
            .shadow
            .payload
            .iter()
            .filter(|r| r.topic == t.id)
            .map(|r| (r.path.clone(), member(tables, r, hash, pools)))
            .collect();
        slices.push(format!(
            ".payload_first = {}u, .payload_count = {}u",
            rows.len(),
            mine.len()
        ));
        rows.extend(tree_order(mine));
    }
    (rows, slices)
}
