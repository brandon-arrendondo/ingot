//! The contract-level shadow rows of a model (apply groups, echo-only paths, constant
//! rows, topics and the command rows of the variant), the checks of the whole mapping, and what
//! contract.json and the keyspace say about it.

use super::keys::Errors;
use super::model::{present_on, Tables};
use super::schema::{
    ApplyGroup, Constant, ContractFile, Echo, NamespaceFile, PayloadRow, Rejected, ShadowCommand,
    Topic, TopicVar,
};
use super::shadow_size::{trees, SHADOW_QUOTA};
use super::{payload, refuse_commands, refuse_shadow, topics};
use serde::Serialize;
use serde_json::{json, Value};

/// The contract-level rows of the mapping.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Shadow {
    pub apply_groups: Vec<ApplyGroup>,
    pub echo: Vec<Echo>,
    pub constants: Vec<Constant>,
    pub topics: Vec<Topic>,
    /// The members of the pub topics' payloads that exist on the variant.
    pub payload: Vec<PayloadRow>,
    /// The command rows that exist on the variant, in namespace-file order.
    pub commands: Vec<ShadowCommand>,
    /// Where the binder lists the desired members it rejected (none: it lists none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected: Option<Rejected>,
    /// Named values the consumer is configured with, usable in any topic template.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<TopicVar>,
    /// Every (row, variant) the contract-level rows name, before the variant's rows are taken,
    /// for the declared-variant check.
    #[serde(skip)]
    pub variant_uses: Vec<(String, String)>,
}

/// The rows of `declared` that exist on `variant` (`only` gives a row's variants), and every
/// (row, variant) they name (`name` names a row).
///
/// @version 1
/// @req REQ-ING-SHD-004
/// @req REQ-ING-TOP-003
fn of_variant<T>(
    declared: Vec<T>,
    variant: Option<&str>,
    name: impl Fn(&T) -> String,
    only: impl Fn(&T) -> Option<&[String]>,
) -> (Vec<T>, Vec<(String, String)>) {
    let uses = declared
        .iter()
        .flat_map(|r| {
            let at = name(r);
            only(r)
                .into_iter()
                .flatten()
                .map(move |v| (at.clone(), v.clone()))
        })
        .collect();
    let mine = declared
        .into_iter()
        .filter(|r| present_on(only(r), variant))
        .collect();
    (mine, uses)
}

/// The rows `contract.toml` declares, taken out of it (echo paths and payload members of
/// `variant` only), and the namespace files' command rows that exist on `variant`.
///
/// v2: topics and command rows.
/// v3: echo paths limited to variants.
/// v4: topic payload members.
/// v5: the rejected-settings list.
/// v6: the rejected-settings list on its variants only.
/// v7: topics on their variants only; the variant-limited rows through `limited_rows`.
/// v8: the declared placeholders.
/// @version 8
/// @req REQ-ING-TOP-007
/// @req REQ-ING-SHD-002
/// @req REQ-ING-CMD-001
/// @req REQ-ING-TOP-001
/// @req REQ-ING-TOP-003
/// @req REQ-ING-SHD-011
/// @req REQ-ING-TOP-005
pub fn rows(
    file: &mut ContractFile,
    namespaces: &[NamespaceFile],
    variant: Option<&str>,
) -> Shadow {
    let commands = namespaces
        .iter()
        .flat_map(|ns| &ns.shadow_commands)
        .filter(|r| present_on(r.variants.as_deref(), variant))
        .cloned()
        .collect();
    let mut s = Shadow {
        apply_groups: std::mem::take(&mut file.apply_groups),
        constants: std::mem::take(&mut file.shadow_constant),
        placeholders: std::mem::take(&mut file.placeholders),
        commands,
        ..Shadow::default()
    };
    limited_rows(&mut s, file, OnVariant { variant });
    s
}

/// The rows that may be limited to variants (echo paths, payload members, topics and the
/// rejected-settings list), taken out of `file` into `s` for `on`, with every (row, variant)
/// they name.
///
/// @version 1
/// @req REQ-ING-SHD-004
/// @req REQ-ING-TOP-005
fn limited_rows(s: &mut Shadow, file: &mut ContractFile, on: OnVariant) {
    let uses = &mut s.variant_uses;
    s.echo = on.take(std::mem::take(&mut file.shadow_echo), uses, |e| {
        (format!("shadow_echo {}", e.path), e.variants.as_deref())
    });
    s.payload = on.take(std::mem::take(&mut file.topic_payload), uses, |p| {
        (
            format!("topic_payload {} {}", p.topic, p.path),
            p.variants.as_deref(),
        )
    });
    s.topics = on.take(std::mem::take(&mut file.topics), uses, |t| {
        (format!("topic {}", t.id), t.variants.as_deref())
    });
    let rejected = file.shadow_rejected.take().into_iter().collect();
    s.rejected = on
        .take(rejected, uses, |r| {
            (format!("shadow_rejected {}", r.path), r.variants.as_deref())
        })
        .into_iter()
        .next();
}

/// The variant a row set is taken for.
struct OnVariant<'a> {
    variant: Option<&'a str>,
}

impl OnVariant<'_> {
    /// The rows of `declared` that exist on the variant, recording into `uses` every (row,
    /// variant) they name; `about` gives a row's name and its variants.
    ///
    /// @version 1
    /// @req REQ-ING-SHD-004
    /// @req REQ-ING-TOP-005
    fn take<T>(
        &self,
        declared: Vec<T>,
        uses: &mut Vec<(String, String)>,
        about: impl Fn(&T) -> (String, Option<&[String]>),
    ) -> Vec<T> {
        let (mine, named) = of_variant(declared, self.variant, |r| about(r).0, |r| about(r).1);
        uses.extend(named);
        mine
    }
}

/// Whether the model maps anything to the shadow (a command row needs a topic, so the topics
/// stand for the command rows).
///
/// v2: topics.
/// @version 2
/// @req REQ-ING-SHD-001
/// @req REQ-ING-TOP-001
pub fn declared(tables: &Tables) -> bool {
    let s = &tables.shadow;
    let rows = s.apply_groups.len() + s.echo.len() + s.constants.len() + s.topics.len();
    tables.keys.iter().any(|k| !k.wire.shadow.is_empty()) || rows > 0
}

/// The mapping's rows and worst-case sizes, `None` when the model maps nothing.
///
/// v2: topics and command rows.
/// v3: topic payload members.
/// v4: the rejected-settings list, only when declared.
/// v5: the declared placeholders, only when there are any.
/// @version 5
/// @req REQ-ING-TOP-007
/// @req REQ-ING-SHD-002
/// @req REQ-ING-SIZ-001
/// @req REQ-ING-CMD-001
/// @req REQ-ING-TOP-003
/// @req REQ-ING-SHD-011
pub fn json(tables: &Tables) -> Option<Value> {
    if !declared(tables) {
        return None;
    }
    let t = trees(tables);
    let s = &tables.shadow;
    let mut doc = json!({
        "apply_groups": s.apply_groups, "echo": s.echo, "constants": s.constants,
        "topics": s.topics, "payload": s.payload, "commands": s.commands,
        "full_max_bytes": t.reported.bytes(), "delta_forced_bytes": t.forced.bytes(),
    });
    if let Some(r) = &s.rejected {
        doc["rejected"] = json!(r);
    }
    if !s.placeholders.is_empty() {
        doc["placeholders"] = json!(s.placeholders);
    }
    Some(doc)
}

/// Every refusal of the mapping: keys and rows, the path trees, the full document's size,
/// topics and command rows.
///
/// The path trees and the size are checked per variant: in the all-variants view rows of
/// disjoint variants may share a path, and each variant's own pass checks its documents.
///
/// v2: topics and command rows.
/// v3: the delta's path conflicts, each once beside the full document's.
/// v4: trees and size only per variant.
/// v5: topic payloads, per variant.
/// @version 5
/// @req REQ-ING-TOP-003
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SHD-003
/// @req REQ-ING-SHD-004
/// @req REQ-ING-SIZ-001
/// @req REQ-ING-TOP-001
/// @req REQ-ING-CMD-001
pub fn check(tables: &Tables, errors: &mut Errors) {
    refuse_shadow::check(tables, errors);
    topics::check(tables, errors);
    refuse_commands::check(tables, errors);
    if !tables.all_variants_view() {
        tree_rules(tables, errors);
        payload::check(tables, errors);
    }
}

/// Path conflicts of the trees and the full document's size.
///
/// @version 1
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SIZ-001
fn tree_rules(tables: &Tables, errors: &mut Errors) {
    let t = trees(tables);
    let full = t.reported.bytes();
    errors.extend(t.desired.errors);
    let delta_only: Vec<String> = t
        .delta
        .errors
        .into_iter()
        .filter(|e| !t.reported.errors.contains(e))
        .collect();
    errors.extend(t.reported.errors);
    errors.extend(delta_only);
    if full > SHADOW_QUOTA {
        errors.push(format!(
            "the full shadow document is up to {full} bytes, over the {SHADOW_QUOTA}-byte AWS IoT shadow quota"
        ));
    }
}
