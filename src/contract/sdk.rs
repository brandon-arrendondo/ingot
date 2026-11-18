//! Per-principal SDKs rendered through a consumer-supplied runtime binding: ingot
//! resolves, per principal, the typed accessors its rights give, the command submit helpers, the
//! event views of the struct keys it reads, its identity (kind, uid, token, KSET digest and keyset) and the contract facts; the binding's
//! templates turn them into code against the consumer's own store runtime. Ingot names no
//! runtime.

use super::digest::{digests, subset};
use super::header::{view, HeaderView};
use super::keys::{CmdPart, Key};
use super::model::Tables;
use super::rights::{READ, WRITE};
use super::schema::KeyType;
use super::snapshot::kind_name;
use crate::codegen::write_generated;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tera::{Context, Tera};

/// One key accessor of an SDK.
#[derive(Serialize)]
struct Accessor {
    /// `namespace_class_key`, the function-name part.
    name: String,
    path: String,
    key: String,
    /// `scalar`, `buffer` or `struct`.
    form: &'static str,
    /// The C type of a scalar, or the element type of a buffer.
    ctype: &'static str,
    /// The unsigned C type of a scalar's width (its decode goes through it).
    utype: &'static str,
    /// `int`, `bool` or `float` (scalars).
    codec: &'static str,
    /// Scalar width, or a buffer's max_size.
    size: u32,
    get: bool,
    set: bool,
}

/// One event view of an SDK: a struct key the principal may read, whose value reaches it only
/// as a delivered event.
#[derive(Serialize)]
struct View {
    /// `namespace_class_key`, the function-name part.
    name: String,
    path: String,
    key: String,
}

/// One command submit helper of an SDK.
#[derive(Serialize)]
struct Submit {
    name: String,
    path: String,
    key: String,
    payload_max: u32,
    structured: bool,
}

/// A C identifier part from a dotted name.
///
/// @version 1
/// @req REQ-ING-SDK-001
fn ident(dotted: &str) -> String {
    dotted.replace('.', "_")
}

/// `0x%08XU`.
///
/// @version 1
/// @req REQ-ING-SDK-001
fn id(v: u32) -> String {
    format!("{v:#010X}U")
}

/// The scalar C type, its unsigned twin and its codec, or `None` for a buffer or struct type.
///
/// v2: the redundant bool codec, keyset sort or UNIX guard removed.
/// v3: shared with the struct value header fields.
/// @version 3
/// @req REQ-ING-SDK-002
/// @req REQ-ING-HDR-002
pub(super) fn scalar(ty: KeyType) -> Option<(&'static str, &'static str, &'static str)> {
    match ty {
        KeyType::Bool => Some(("bool", "uint8_t", "int")),
        KeyType::Uint8 => Some(("uint8_t", "uint8_t", "int")),
        KeyType::Int8 => Some(("int8_t", "uint8_t", "int")),
        KeyType::Uint16 => Some(("uint16_t", "uint16_t", "int")),
        KeyType::Int16 => Some(("int16_t", "uint16_t", "int")),
        KeyType::Uint32 => Some(("uint32_t", "uint32_t", "int")),
        KeyType::Int32 => Some(("int32_t", "uint32_t", "int")),
        KeyType::Float32 => Some(("float", "uint32_t", "float")),
        _ => None,
    }
}

/// The accessor of `key` for a principal with `bits` on it (struct rights are UNIX-only:
/// the model refuses them for other kinds),
/// or `None` when it gets none: no right, a struct it cannot write or read (struct values are
/// read by subscribing); a command key is written only through its submit helper.
///
/// v2: the redundant bool codec, keyset sort or UNIX guard removed.
/// @version 2
/// @req REQ-ING-SDK-002
fn accessor(key: &Key, bits: u8) -> Option<Accessor> {
    let command = matches!(key.command, Some((_, CmdPart::Command)));
    let (form, (ctype, utype, codec)) = match (scalar(key.ty), key.ty) {
        (Some(s), _) => ("scalar", s),
        (None, KeyType::Struct) => ("struct", ("uint8_t", "uint8_t", "")),
        (None, KeyType::String) => ("buffer", ("char", "uint8_t", "")),
        (None, _) => ("buffer", ("uint8_t", "uint8_t", "")),
    };
    let structured = form == "struct";
    let get = bits & READ != 0 && !structured;
    let set = bits & WRITE != 0 && !command;
    let size = key.ty.scalar_size().unwrap_or(key.max_size);
    (get || set).then(|| Accessor {
        name: ident(&key.path),
        path: key.path.clone(),
        key: id(key.id),
        form,
        ctype,
        utype,
        codec,
        size,
        get,
        set,
    })
}

/// The submit helpers of principal `p`: every command whose command key it may write (the model
/// gives struct rights to UNIX principals only).
///
/// v2: the redundant bool codec, keyset sort or UNIX guard removed.
/// @version 2
/// @req REQ-ING-SDK-003
fn submits(tables: &Tables, p: usize) -> Vec<Submit> {
    tables
        .commands
        .iter()
        .filter_map(|c| {
            let i = tables.keys.iter().position(|k| k.id == c.keys[0])?;
            let structured = tables.keys[i].ty == KeyType::Struct;
            let writes = tables.rights[p][i] & WRITE != 0;
            writes.then(|| Submit {
                name: ident(&c.path),
                path: c.path.clone(),
                key: id(c.keys[0]),
                payload_max: c.max_payload,
                structured,
            })
        })
        .collect()
}

/// The contract, provenance and principal names of an SDK.
///
/// v2: no runtime version (the binding states what it needs).
/// @version 2
/// @req REQ-ING-SDK-001
fn insert_header(ctx: &mut Context, tables: &Tables, name: &str, commit: &str) {
    ctx.insert("principal", name);
    ctx.insert("p", &ident(name));
    ctx.insert("guard", &ident(name).to_uppercase());
    ctx.insert("name", &tables.name);
    ctx.insert("major", &tables.version.major);
    ctx.insert("minor", &tables.version.minor);
    ctx.insert("patch", &tables.patch);
    ctx.insert("variant", &tables.variant);
    ctx.insert("model_hash", &tables.model_hash);
    ctx.insert("ingot_version", env!("CARGO_PKG_VERSION"));
    ctx.insert("ingot_commit", commit);
}

/// C initialisers of `items`.
///
/// @version 1
/// @req REQ-ING-SDK-001
fn list<T>(items: &[T], f: impl Fn(&T) -> String) -> Vec<String> {
    items.iter().map(f).collect()
}

/// Principal `p`'s identity: its kind, uid and token, the sorted keyset and the digest at the
/// current minor.
///
/// v3: the kind, uid and token instead of a runtime's transport names.
/// v4: the kind name shared with contract.json.
/// @version 4
/// @req REQ-ING-SDK-001
fn insert_identity(ctx: &mut Context, tables: &Tables, p: usize) {
    let pr = &tables.principals[p];
    ctx.insert("kind", &kind_name(pr.kind));
    ctx.insert("uid", &pr.uid);
    ctx.insert("token", &pr.token);
    let keyset = subset(tables, p, tables.version.minor);
    ctx.insert("keyset_ids", &list(&keyset, |(i, _)| id(*i)));
    ctx.insert("keyset_types", &list(&keyset, |(_, t)| format!("{t}u")));
    let digest = digests(tables)[p][usize::from(tables.version.minor)];
    ctx.insert("digest", &list(&digest, |b| format!("{b:#04x}U")));
}

/// The rendering context of principal `p`'s SDK.
///
/// v2: the redundant bool codec, keyset sort or UNIX guard removed.
/// v3: the struct value headers of the keys it has a right on; accessors from `accessors`.
/// v4: the struct key facts (headers and event views) from `insert_struct_keys`.
/// @version 4
/// @req REQ-ING-SDK-001
/// @req REQ-ING-SDK-002
/// @req REQ-ING-SDK-003
/// @req REQ-ING-SDK-006
/// @req REQ-ING-HDR-002
fn context(tables: &Tables, p: usize, commit: &str) -> Context {
    let pr = &tables.principals[p];
    let accessors = accessors(tables, p);
    let submits = submits(tables, p);
    let mut ctx = Context::new();
    insert_header(&mut ctx, tables, &pr.name, commit);
    insert_identity(&mut ctx, tables, p);
    insert_scalar_helpers(&mut ctx, &accessors);
    let structs =
        accessors.iter().any(|a| a.form == "struct") || submits.iter().any(|s| s.structured);
    ctx.insert("need_struct", &structs);
    ctx.insert("need_header", &(structs || !submits.is_empty()));
    ctx.insert("accessors", &accessors);
    ctx.insert("submits", &submits);
    insert_struct_keys(&mut ctx, tables, p);
    ctx
}

/// Principal `p`'s struct key facts: the value headers of the keys it has a right on and the
/// event views of the struct keys it may read.
///
/// @version 1
/// @req REQ-ING-SDK-006
/// @req REQ-ING-HDR-002
fn insert_struct_keys(ctx: &mut Context, tables: &Tables, p: usize) {
    ctx.insert("value_headers", &value_headers(tables, p));
    ctx.insert("views", &views(tables, p));
}

/// The event views of principal `p`: every struct key it may read, command keys included, in
/// key ID order.
///
/// @version 1
/// @req REQ-ING-SDK-006
fn views(tables: &Tables, p: usize) -> Vec<View> {
    tables
        .keys
        .iter()
        .zip(&tables.rights[p])
        .filter(|(k, bits)| k.ty == KeyType::Struct && **bits & READ != 0)
        .map(|(k, _)| View {
            name: ident(&k.path),
            path: k.path.clone(),
            key: id(k.id),
        })
        .collect()
}

/// The accessors of principal `p`, in key ID order.
///
/// @version 1
/// @req REQ-ING-SDK-002
fn accessors(tables: &Tables, p: usize) -> Vec<Accessor> {
    tables
        .keys
        .iter()
        .zip(&tables.rights[p])
        .filter_map(|(k, bits)| accessor(k, *bits))
        .collect()
}

/// The struct value headers of the keys principal `p` may read or write.
///
/// @version 1
/// @req REQ-ING-HDR-002
fn value_headers(tables: &Tables, p: usize) -> Vec<HeaderView> {
    tables
        .keys
        .iter()
        .zip(&tables.rights[p])
        .filter(|(_, bits)| **bits != 0)
        .filter_map(|(k, _)| view(k))
        .collect()
}

/// Which scalar codec helpers the SDK's accessors use (an unused one would not compile
/// warning-free).
///
/// @version 1
/// @req REQ-ING-SDK-002
fn insert_scalar_helpers(ctx: &mut Context, accessors: &[Accessor]) {
    let any = |codec: Option<&str>, set: bool| {
        accessors.iter().any(|a| {
            a.form == "scalar"
                && codec.is_none_or(|c| a.codec == c)
                && (if set { a.set } else { a.get })
        })
    };
    ctx.insert("need_put", &any(None, true));
    ctx.insert("need_get", &any(None, false));
    ctx.insert("need_float_bits", &any(Some("float"), true));
    ctx.insert("need_bits_float", &any(Some("float"), false));
}

/// The placeholder a binding file name carries for the principal's identifier.
pub const PRINCIPAL_PLACEHOLDER: &str = "PRINCIPAL";

/// The files of a binding directory, in name order: at least one, and files only.
///
/// @version 1
/// @req REQ-ING-SDK-004
fn binding_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry?.path();
        if !path.is_file() {
            return Err(format!("{}: a binding holds files only", path.display()).into());
        }
        files.push(path);
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("{}: the binding has no templates", dir.display()).into());
    }
    Ok(files)
}

/// The suffix a binding template may carry so that the binding directory holds no source file:
/// it is stripped from the output name.
pub const TEMPLATE_SUFFIX: &str = ".tera";

/// A binding's templates and, for each, its output name (the file name without a trailing
/// `.tera`); an empty output name, or one another template renders to as well, is refused.
pub struct Binding {
    tera: Tera,
    outputs: Vec<(String, String)>,
}

/// The output name of template `name`: `name` without a trailing `.tera`.
///
/// @version 1
/// @req REQ-ING-SDK-005
fn output_name(name: &str) -> &str {
    name.strip_suffix(TEMPLATE_SUFFIX).unwrap_or(name)
}

/// The output name of template `name`, refused when empty or already another template's.
///
/// @version 1
/// @req REQ-ING-SDK-005
fn unique_output(name: &str, outputs: &[(String, String)]) -> Result<String, String> {
    let out = output_name(name).to_string();
    if out.is_empty() {
        Err(format!("{name}: a template's output name is empty"))
    } else if outputs.iter().any(|(_, o)| o == &out) {
        Err(format!(
            "{name}: renders to {out}, as another template does"
        ))
    } else {
        Ok(out)
    }
}

/// The templates of a binding directory, each under its file name, with their output names.
///
/// v2: output names without the `.tera` suffix, each non-empty and unique.
/// @version 2
/// @req REQ-ING-SDK-004
/// @req REQ-ING-SDK-005
fn binding(dir: &Path) -> Result<Binding, Box<dyn std::error::Error>> {
    let mut tera = Tera::default();
    let mut outputs: Vec<(String, String)> = Vec::new();
    for path in binding_files(dir)? {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let out = unique_output(&name, &outputs)?;
        tera.add_template_file(&path, Some(&name))?;
        outputs.push((name, out));
    }
    Ok(Binding { tera, outputs })
}

/// Render the binding at `dir` once per principal into `out/sdk/<principal>/`, each template
/// to its output name with `PRINCIPAL` replaced by the principal's identifier.
///
/// v2: through a consumer-supplied binding (no built-in runtime).
/// v3: output names without the `.tera` suffix.
/// @version 3
/// @req REQ-ING-SDK-001
/// @req REQ-ING-SDK-004
/// @req REQ-ING-SDK-005
pub fn write(
    dir: &Path,
    tables: &Tables,
    commit: &str,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let binding = binding(dir)?;
    for (p, pr) in tables.principals.iter().enumerate() {
        let name = ident(&pr.name);
        let to = out.join("sdk").join(&name);
        std::fs::create_dir_all(&to)?;
        let ctx = context(tables, p, commit);
        for (template, output) in &binding.outputs {
            let file = output.replace(PRINCIPAL_PLACEHOLDER, &name);
            write_generated(to.join(file), binding.tera.render(template, &ctx)?)?;
        }
    }
    Ok(())
}
