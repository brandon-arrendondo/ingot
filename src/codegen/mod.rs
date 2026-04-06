pub mod storage;
pub mod target;

use crate::model::key::KeyEncoding;
use crate::model::schema::DataModel;
use serde::Serialize;
use std::path::Path;
use tera::{Context, Tera};

/// A key definition ready for template rendering.
#[derive(Debug, Serialize)]
pub struct KeyDefRenderable {
    pub namespace: String,
    pub class: String,
    pub name: String,
    pub define_name: String,
    pub hex_value: String,
    pub type_name: String,
    pub unit: Option<String>,
    pub read_only: bool,
    pub thread_safe: bool,
    pub persistent: bool,
    pub event: bool,
}

/// Generate all C code from a parsed data model.
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

    // Collect all key definitions
    let key_defs = collect_key_definitions(model, ns_id);

    // Generate key_definitions.h
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        ctx.insert("keys", &key_defs);
        let rendered = tera.render("key_definitions.h", &ctx)?;
        std::fs::write(output_dir.join("key_definitions.h"), rendered)?;
        log::info!("Generated key_definitions.h ({} keys)", key_defs.len());
    }

    // Generate jenkins_hash.h and jenkins_hash.c
    {
        let mut ctx = Context::new();
        ctx.insert("version", version);
        let h = tera.render("jenkins_hash.h", &ctx)?;
        let c = tera.render("jenkins_hash.c", &ctx)?;
        std::fs::write(output_dir.join("jenkins_hash.h"), h)?;
        std::fs::write(output_dir.join("jenkins_hash.c"), c)?;
        log::info!("Generated jenkins_hash.h/.c");
    }

    // Generate integer_storage.h and integer_storage.c
    {
        let int_storages = storage::integer::collect_integer_storage(model, ns_id)?;
        if !int_storages.is_empty() {
            let mut ctx = Context::new();
            ctx.insert("version", version);
            ctx.insert("types", &int_storages);
            let h = tera.render("integer_storage.h", &ctx)?;
            let c = tera.render("integer_storage.c", &ctx)?;
            std::fs::write(output_dir.join("integer_storage.h"), h)?;
            std::fs::write(output_dir.join("integer_storage.c"), c)?;
            log::info!(
                "Generated integer_storage.h/.c ({} type groups)",
                int_storages.len()
            );
        }
    }

    Ok(())
}

/// Build renderable key definitions from the model.
fn collect_key_definitions(model: &DataModel, ns_id: u16) -> Vec<KeyDefRenderable> {
    let mut defs = Vec::new();
    let ns_name = model.meta.id.to_uppercase();

    for (class_idx, class) in model.classes.iter().enumerate() {
        let class_name = class.id.to_uppercase();
        // Track per-type ID counters (matching key encoding)
        let mut type_counters: [u16; 16] = [0; 16];

        for key in &class.keys {
            let type_code = key.data_type.type_code();
            let id = type_counters[type_code as usize];
            type_counters[type_code as usize] += 1;

            let encoding = KeyEncoding {
                namespace: ns_id,
                class: class_idx as u8,
                id,
                data_type: type_code,
                thread_safe: key.thread_safe,
                derived: false,
                read_only: key.read_only,
            };

            let encoded = encoding.encode();
            let key_name = key.id.to_uppercase().replace(' ', "_");

            defs.push(KeyDefRenderable {
                namespace: model.meta.id.clone(),
                class: class.id.clone(),
                name: key.id.clone(),
                define_name: format!("DM_KEY_{ns_name}_{class_name}_{key_name}"),
                hex_value: format!("{encoded:#010X}"),
                type_name: format!("{:?}", key.data_type).to_lowercase(),
                unit: key.unit.clone(),
                read_only: key.read_only,
                thread_safe: key.thread_safe,
                persistent: key.persistent,
                event: key.event,
            });
        }
    }

    defs
}
