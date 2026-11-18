//! The named ids of internal command rows: `DSH_INTERNAL_<VERB>` for every `shadow_commands` row
//! of the variant that declares an `internal_id` (shadow table interface minor 10), so the
//! library dispatches on a name and never repeats the number.

use super::constants::{c_identifier, NAME_MAX};
use super::keys::Errors;
use super::model::{variants_overlap, Tables};
use super::refuse_commands::label;
use super::schema::{Body, Selector, ShadowCommand, WireLit};
use serde::Serialize;

/// One internal row constant: its name and the row's id.
#[derive(Serialize, Debug, PartialEq)]
pub struct InternalConstant {
    pub name: String,
    pub value: u16,
}

/// The text that selects row `r`, from its innermost selector (the body object's for an object
/// body): the wire string, else the element, else the field.
///
/// @version 1
/// @req REQ-ING-DSH-008
fn verb(r: &ShadowCommand) -> &str {
    let s: &Selector = match &r.body {
        Body::Object(o) => &o.selector,
        Body::Kind(_) => &r.selector,
    };
    match (&s.wire, &s.element) {
        (Some(WireLit::Text(w)), _) => w,
        (_, Some(e)) => e,
        _ => &s.field,
    }
}

/// `DSH_INTERNAL_<VERB>` in upper case, each dot an underscore.
///
/// @version 1
/// @req REQ-ING-DSH-008
fn name_of(r: &ShadowCommand) -> String {
    format!("DSH_INTERNAL_{}", verb(r).replace('.', "_")).to_uppercase()
}

/// The rows of the variant that declare an internal_id, with their constant's name and the id.
///
/// @version 1
/// @req REQ-ING-DSH-008
fn declared(tables: &Tables) -> impl Iterator<Item = (&ShadowCommand, String, u16)> {
    tables
        .shadow
        .commands
        .iter()
        .filter_map(|r| r.internal_id.map(|id| (r, name_of(r), id)))
}

/// The constants of the variant in row declaration order, one per name.
///
/// @version 1
/// @req REQ-ING-DSH-008
pub fn constants(tables: &Tables) -> Vec<InternalConstant> {
    let mut all: Vec<InternalConstant> = Vec::new();
    for (_, name, value) in declared(tables) {
        if !all.iter().any(|c| c.name == name) {
            all.push(InternalConstant { name, value });
        }
    }
    all
}

/// Every constant is a C identifier of at most `NAME_MAX` characters, and no name stands for two
/// ids on a shared variant.
///
/// @version 1
/// @req REQ-ING-DSH-008
pub fn check(tables: &Tables, errors: &mut Errors) {
    let rows: Vec<_> = declared(tables).collect();
    for (i, (r, name, id)) in rows.iter().enumerate() {
        let mut refuse = |why: String| {
            errors.push(format!("{}: internal constant {name} {why}", label(r)));
        };
        if !c_identifier(name) {
            refuse("is not a C identifier".into());
        } else if name.len() > NAME_MAX {
            refuse(format!("is longer than {NAME_MAX} characters"));
        }
        let other = rows[i + 1..].iter().find(|(o, n, v)| {
            n == name && v != id && variants_overlap(r.variants.as_deref(), o.variants.as_deref())
        });
        if let Some((o, _, v)) = other {
            refuse(format!("also names {} (internal_id {v})", label(o)));
        }
    }
}
