//! Table-interface outputs: the fixed headers `dmt_types.h` (store tables, TIV) and
//! `dsh_types.h` (shadow tables, DSH) and, in contract mode, the generated tables.

use std::path::Path;
use tera::{Context, Tera};

use super::render_template;

/// The fixed interface headers and their interface versions; their bytes depend only on those.
pub const TYPES_HEADERS: [(&str, &str); 2] =
    [("dmt_types.h", "TIV 1.0"), ("dsh_types.h", "DSH 1.10")];

/// Write the fixed interface headers into `output_dir`, creating the directory.
///
/// v2: `dsh_types.h` too.
/// @version 2
/// @req REQ-ING-TIV-001
/// @req REQ-ING-DSH-001
pub fn write_table_types(tera: &Tera, output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;
    for (header, interface) in TYPES_HEADERS {
        render_template(tera, header, &Context::new(), output_dir)?;
        log::info!("Generated {header} ({interface})");
    }
    Ok(())
}
