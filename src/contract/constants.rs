//! `dmt_constants.h`: the code of every value of every enum key, of every command's outcome enum
//! and of every enum a topic var names as a named C constant, `DMT_E_<KEY OR COMMAND PATH>_<VALUE>`
//! or `DMT_E_<NAMESPACE>_<ENUM>_<VALUE>`, for the variant the tables resolve, so a consumer never
//! writes an enum code by hand. The header is per model and outside the table interface.

use super::enums::ResolvedEnum;
use super::keys::Errors;
use super::model::Tables;
use crate::codegen::render_template;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use tera::{Context, Tera};

/// The longest constant name: C99's significant initial characters of an identifier.
pub const NAME_MAX: usize = 63;

/// One enum constant: its name, its code and the key, command or enum it is of.
#[derive(Serialize)]
pub struct EnumConstant {
    pub name: String,
    pub value: i64,
    pub key: String,
}

/// `DMT_E_<PATH>_<VALUE>` in upper case, each dot and each `::` an underscore (`path`: a key or
/// command path, or a `namespace::enum` reference).
///
/// v2: a `namespace::enum` reference.
/// @version 2
/// @req REQ-ING-TIV-017
/// @req REQ-ING-TOP-008
fn name_of(path: &str, value: &str) -> String {
    let path = path.replace("::", "_").replace('.', "_");
    format!("DMT_E_{path}_{value}").to_uppercase()
}

/// Every enum a var of the tables' topics names, as the variant resolves it, by its
/// `namespace::enum` reference (an undefined one is left out: the topic check refuses it).
///
/// @version 1
/// @req REQ-ING-TOP-008
pub fn var_enums(tables: &Tables) -> BTreeMap<&str, &ResolvedEnum> {
    let vars = tables.shadow.topics.iter().flat_map(|t| &t.vars);
    vars.filter_map(|v| v.enum_ref.as_deref())
        .filter_map(|r| tables.enums.get(r).map(|e| (r, e)))
        .collect()
}

/// Every enum a constant is generated for, with the path it is named after: each enum key's
/// enum and each command's outcome enum at its command key, in key ID order, then each enum a
/// topic var names, in reference order.
///
/// v2: the enums topic vars name, after the keyed ones.
/// @version 2
/// @req REQ-ING-TIV-017
/// @req REQ-ING-TIV-018
/// @req REQ-ING-TOP-008
fn sources(tables: &Tables) -> Vec<(&str, &ResolvedEnum)> {
    let keys = tables
        .keys
        .iter()
        .filter_map(|k| k.enum_values.as_ref().map(|e| (k.id, k.path.as_str(), e)));
    let commands = tables
        .commands
        .iter()
        .filter_map(|c| c.outcome.as_ref().map(|e| (c.keys[0], c.path.as_str(), e)));
    let mut keyed: Vec<_> = keys.chain(commands).collect();
    keyed.sort_by_key(|(id, _, _)| *id);
    let keyed = keyed.into_iter().map(|(_, path, e)| (path, e));
    keyed.chain(var_enums(tables)).collect()
}

/// The constants of every enum key and command outcome, in key ID order, then of every enum a
/// topic var names, each one's values in code order.
///
/// v2: command outcomes, at their command key's ID.
/// v3: the enums topic vars name.
/// @version 3
/// @req REQ-ING-TIV-017
/// @req REQ-ING-TIV-018
/// @req REQ-ING-TOP-008
pub fn enum_constants(tables: &Tables) -> Vec<EnumConstant> {
    let mut all = Vec::new();
    for (path, values) in sources(tables) {
        let mut mine: Vec<EnumConstant> = values
            .iter()
            .map(|(name, m)| EnumConstant {
                name: name_of(path, name),
                value: m.value,
                key: path.to_string(),
            })
            .collect();
        mine.sort_by_key(|c| c.value);
        all.extend(mine);
    }
    all
}

/// Whether `name` is a C identifier.
///
/// v2: shared with the internal row constants.
/// @version 2
/// @req REQ-ING-TIV-017
/// @req REQ-ING-DSH-008
pub(super) fn c_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every constant is a C identifier of at most `NAME_MAX` characters, and no two (key, value)
/// pairs name one constant.
///
/// @version 1
/// @req REQ-ING-TIV-017
pub fn check(tables: &Tables, errors: &mut Errors) {
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for c in enum_constants(tables) {
        let mut refuse =
            |why: String| errors.push(format!("{}: enum constant {} {why}", c.key, c.name));
        if !c_identifier(&c.name) {
            refuse("is not a C identifier".into());
        } else if c.name.len() > NAME_MAX {
            refuse(format!("is longer than {NAME_MAX} characters"));
        }
        match owner.get(&c.name) {
            Some(other) => refuse(format!("is also {other}'s")),
            None => {
                owner.insert(c.name.clone(), c.key.clone());
            }
        }
    }
}

/// Write `dmt_constants.h` into `output_dir`.
///
/// v2: and the struct value headers.
/// v3: whether a topic var names an enum.
/// @version 3
/// @req REQ-ING-TIV-017
/// @req REQ-ING-HDR-002
/// @req REQ-ING-TOP-008
pub fn write(
    tera: &Tera,
    tables: &Tables,
    commit: &str,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let v = &tables.version;
    let mut ctx = Context::new();
    ctx.insert("ingot_version", env!("CARGO_PKG_VERSION"));
    ctx.insert("ingot_commit", commit);
    ctx.insert("name", &tables.name);
    ctx.insert(
        "version",
        &format!("{}.{}.{}", v.major, v.minor, tables.patch),
    );
    ctx.insert("variant", &tables.variant);
    ctx.insert("constants", &enum_constants(tables));
    ctx.insert("headers", &super::header::views(tables));
    ctx.insert("var_enums", &!var_enums(tables).is_empty());
    render_template(tera, "dmt_constants.h", &ctx, output_dir)?;
    Ok(())
}
