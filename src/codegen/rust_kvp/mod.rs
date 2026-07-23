pub mod enums;
pub mod storage;

use crate::model::schema::DataModel;
use serde::Serialize;
use std::path::Path;
use tera::{Context, Tera};

use super::write_generated;

/// Encoded key constant, emitted as `pub const DM_KEY_...: u32 = 0x...;`.
#[derive(Debug, Serialize)]
struct KeyConst {
    const_name: String,
    hex_value: String,
}

/// Generate a single `dm.rs` file exposing the data model as a `no_std`
/// struct with named accessor methods, backed by the same perfect-hash
/// static-array storage strategy as the C backend.
pub fn generate(
    model: &DataModel,
    ns_id: u16,
    output_dir: &Path,
    template_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;

    let tera = Tera::new(
        template_dir
            .join("*")
            .to_str()
            .ok_or("invalid template path")?,
    )?;

    let version = env!("CARGO_PKG_VERSION");

    let rust_enums = enums::collect_rust_enums(model);
    let (int_groups, int_accessors) = storage::collect_int_groups(model, ns_id)?;
    let (bool_group, bool_accessors) = storage::collect_bool_group(model, ns_id)?;
    let bytes_accessors = storage::collect_bytes_accessors(model, ns_id);

    let mut key_consts: Vec<KeyConst> = Vec::new();
    let mut persistent_key_names: Vec<String> = Vec::new();

    for a in &int_accessors {
        key_consts.push(KeyConst {
            const_name: a.const_name.clone(),
            hex_value: a.hex_value.clone(),
        });
        if a.persistent {
            persistent_key_names.push(a.const_name.clone());
        }
    }
    for a in &bool_accessors {
        key_consts.push(KeyConst {
            const_name: a.const_name.clone(),
            hex_value: a.hex_value.clone(),
        });
        if a.persistent {
            persistent_key_names.push(a.const_name.clone());
        }
    }
    for a in &bytes_accessors {
        key_consts.push(KeyConst {
            const_name: a.const_name.clone(),
            hex_value: a.hex_value.clone(),
        });
        if a.persistent {
            persistent_key_names.push(a.const_name.clone());
        }
    }

    let has_int = !int_groups.is_empty();
    let has_bool = bool_group.is_some();
    let has_bytes = !bytes_accessors.is_empty();

    let mut ctx = Context::new();
    ctx.insert("version", version);
    ctx.insert("enums", &rust_enums);
    ctx.insert("key_consts", &key_consts);
    ctx.insert("int_groups", &int_groups);
    ctx.insert("int_accessors", &int_accessors);
    ctx.insert("bool_group", &bool_group);
    ctx.insert("bool_accessors", &bool_accessors);
    ctx.insert("bytes_accessors", &bytes_accessors);
    ctx.insert("persistent_key_names", &persistent_key_names);
    ctx.insert("has_int", &has_int);
    ctx.insert("has_bool", &has_bool);
    ctx.insert("has_bytes", &has_bytes);

    let rendered = tera.render("dm_rust.rs", &ctx)?;
    write_generated(output_dir.join("dm.rs"), rendered)?;

    log::info!(
        "Generated dm.rs ({} int group(s), bool={}, {} string/binary key(s), {} persistent)",
        int_groups.len(),
        has_bool,
        bytes_accessors.len(),
        persistent_key_names.len(),
    );

    Ok(())
}
