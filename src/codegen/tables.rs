//! Table-interface outputs: the fixed headers `dmt_types.h` (store tables, TIV) and
//! `dsh_types.h` (shadow tables, DSH) and, in contract mode, the generated tables.

use std::path::Path;
use tera::{Context, Tera};

use super::render_template;
use crate::contract::types::RING_RECORD_HEADER_BYTES;

/// The fixed interface headers and their interface versions; their bytes depend only on those.
pub const TYPES_HEADERS: [(&str, &str); 2] =
    [("dmt_types.h", "TIV 1.1"), ("dsh_types.h", "DSH 1.10")];

/// The context of the fixed headers: the interface constants ingot's own checks use, so a header
/// states the same number the check does and there is no second copy of it.
///
/// @version 1
/// @req REQ-ING-LOG-011
/// @return the context `dmt_types.h` and `dsh_types.h` are rendered with.
fn types_context() -> Context {
    let mut ctx = Context::new();
    ctx.insert("log_ring_record_header_bytes", &RING_RECORD_HEADER_BYTES);
    ctx
}

/// Write the fixed interface headers into `output_dir`, creating the directory.
///
/// v2: `dsh_types.h` too.
/// v3: rendered with the interface constants (types_context).
/// @version 3
/// @req REQ-ING-TIV-001
/// @req REQ-ING-DSH-001
/// @req REQ-ING-LOG-011
pub fn write_table_types(tera: &Tera, output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;
    let ctx = types_context();
    for (header, interface) in TYPES_HEADERS {
        render_template(tera, header, &ctx, output_dir)?;
        log::info!("Generated {header} ({interface})");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @brief dmt_types.h rendered from the embedded templates with `ctx`.
    /// @version 1
    /// @return the header text.
    fn render_types(ctx: &Context) -> String {
        crate::codegen::templates::load(None)
            .unwrap()
            .render("dmt_types.h", ctx)
            .unwrap()
    }

    #[test]
    fn dmt_types_h_states_the_line_check_s_ring_record_header() {
        let text = render_types(&types_context());
        let define =
            format!("#define DMT_LOG_RING_RECORD_HEADER_BYTES {RING_RECORD_HEADER_BYTES}u\n");
        assert!(text.contains(&define), "dmt_types.h must state '{define}'");
    }

    #[test]
    fn the_ring_record_header_define_is_not_a_literal() {
        let mut ctx = types_context();
        ctx.insert("log_ring_record_header_bytes", &99u32);
        let text = render_types(&ctx);
        assert!(text.contains("#define DMT_LOG_RING_RECORD_HEADER_BYTES 99u\n"));
    }
}
