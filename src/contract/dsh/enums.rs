//! `dsh_enums`, `dsh_outbound` and `dsh_inbound`: each enum a table row uses, once, with its
//! codes' outbound wire strings (sorted by code) and its inbound strings (sorted by text).

use super::super::enums::ResolvedEnum;
use super::text;
use std::collections::BTreeMap;

/// The enums registered so far, in first-use order, and their rows.
#[derive(Default)]
pub struct EnumPool {
    index: BTreeMap<String, u16>,
    enums: Vec<String>,
    outbound: Vec<String>,
    inbound: Vec<String>,
}

/// Every inbound string of `e` with its code, sorted by text: a member's wire string and its
/// accept aliases; a member that reports as another contributes only its aliases (its wire
/// string is the target's).
///
/// @version 1
/// @req REQ-ING-DSH-001
pub fn inbound(e: &ResolvedEnum) -> Vec<(String, i64)> {
    let mut list: Vec<(String, i64)> = e
        .values()
        .flat_map(|m| {
            let own = m.report_as.is_none().then_some(&m.wire);
            own.into_iter()
                .chain(&m.accept)
                .map(move |s| (s.clone(), m.value))
        })
        .collect();
    list.sort();
    list
}

impl EnumPool {
    /// The index of enum `name` (`namespace::enum`), registering its rows on first use.
    ///
    /// @version 1
    /// @req REQ-ING-DSH-001
    pub fn index(&mut self, name: &str, e: &ResolvedEnum) -> u16 {
        if let Some(i) = self.index.get(name) {
            return *i;
        }
        let i = self.enums.len() as u16;
        let mut codes: Vec<(i64, &str)> = e.values().map(|m| (m.value, m.wire.as_str())).collect();
        codes.sort();
        let ins = inbound(e);
        self.enums.push(format!(
            "{{ .name = {}, .out_first = {}u, .out_count = {}u, .in_first = {}u, .in_count = {}u }}",
            text(Some(name)),
            self.outbound.len(),
            codes.len(),
            self.inbound.len(),
            ins.len()
        ));
        self.outbound.extend(
            codes
                .iter()
                .map(|(c, w)| format!("{{ .code = {c}, .wire = {} }}", text(Some(w)))),
        );
        self.inbound.extend(
            ins.iter()
                .map(|(s, c)| format!("{{ .text = {}, .code = {c} }}", text(Some(s)))),
        );
        self.index.insert(name.to_string(), i);
        i
    }

    /// The enum, outbound and inbound rows.
    ///
    /// @version 1
    /// @req REQ-ING-DSH-001
    pub fn rows(self) -> (Vec<String>, Vec<String>, Vec<String>) {
        (self.enums, self.outbound, self.inbound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::enums::Member;

    /// @brief A member with a wire string, aliases and an optional report target.
    /// @version 1
    fn member(value: i64, wire: &str, accept: &[&str], report_as: Option<&str>) -> Member {
        Member {
            value,
            wire: wire.to_string(),
            accept: accept.iter().map(|s| s.to_string()).collect(),
            explicit: true,
            report_as: report_as.map(str::to_string),
        }
    }

    #[test]
    fn a_reporting_member_is_inbound_only_through_its_aliases() {
        let e: ResolvedEnum = [
            ("max".to_string(), member(4, "high", &["max"], None)),
            ("high".to_string(), member(3, "high", &["hi"], Some("max"))),
        ]
        .into();
        assert_eq!(
            inbound(&e),
            vec![("hi".into(), 3), ("high".into(), 4), ("max".into(), 4)]
        );
    }

    #[test]
    fn an_enum_registers_once_with_sorted_rows() {
        let e: ResolvedEnum = [
            ("b".to_string(), member(2, "bee", &[], None)),
            ("a".to_string(), member(1, "zed", &[], None)),
        ]
        .into();
        let mut pool = EnumPool::default();
        assert_eq!(pool.index("ns::e", &e), 0);
        assert_eq!(pool.index("ns::f", &e), 1);
        assert_eq!(pool.index("ns::e", &e), 0);
        let (enums, outbound, inbound) = pool.rows();
        assert_eq!(enums.len(), 2);
        assert!(enums[1].contains(".out_first = 2u") && enums[1].contains(".in_first = 2u"));
        assert_eq!(outbound[0], "{ .code = 1, .wire = \"zed\" }");
        assert_eq!(inbound[0], "{ .text = \"bee\", .code = 2 }");
    }
}
