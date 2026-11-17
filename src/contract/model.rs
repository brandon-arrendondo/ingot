//! Load a contract-mode model and resolve it into the content of the generated tables.

use super::enums::{self, ResolvedEnum};
use super::error_groups::{self, ErrorGroup};
use super::keys::{
    command_key_names, command_keys, command_outcome, declared_key, ClassCtx, Errors, Key,
};
use super::log_streams::{self, LogStream};
use super::ranges::Units;
use super::rights::{self, Principal};
use super::schema::{
    Bounds, CommandDef, ContractFile, NamespaceFile, Persist, PrincipalDef, Reserved, UidCapsDef,
    UnitsFile,
};
use super::shadow::{self, Shadow};
use super::types::{Version, OWNER_CORE, WK_RULES};
use super::variants;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One `dmt_commands` row.
#[derive(Debug)]
pub struct Command {
    pub path: String,
    pub keys: [u32; 3],
    pub max_payload: u32,
    pub max_result: u32,
    pub max_edits: u16,
    pub dedupe_ring: u16,
    pub owner: String,
    pub persist: Persist,
    /// Why the command is not on the wire, when it is internal.
    pub internal: Option<String>,
    /// The resolved enum of the result's outcome codes, when the command names one.
    pub outcome: Option<ResolvedEnum>,
}

/// A namespace's name, ID and classes `(name, class_index)`, for the layout checks.
#[derive(Debug)]
pub struct NsLayout {
    pub name: String,
    pub id: u16,
    pub classes: Vec<(String, u8)>,
}

/// Everything the generated tables contain.
#[derive(Debug)]
pub struct Tables {
    pub layout: Vec<NsLayout>,
    pub name: String,
    pub version: Version,
    pub patch: u16,
    pub variant: String,
    /// The declared variants (empty when the model declares none).
    pub variants: Vec<String>,
    /// The unit vocabulary, when the model declares one.
    pub units: Option<Units>,
    /// The error groups, resolved for the variant.
    pub error_groups: Vec<ErrorGroup>,
    pub model_hash: String,
    pub keys: Vec<Key>,
    pub reserved: Vec<u32>,
    pub principals: Vec<Principal>,
    /// The store core's own log stream, when the model declares one.
    pub core_log: Option<LogStream>,
    pub rights: Vec<Vec<u8>>,
    pub commands: Vec<Command>,
    pub uid_caps: Vec<UidCapsDef>,
    pub bounds: Bounds,
    /// The contract-level shadow rows.
    pub shadow: Shadow,
    /// Every enum, resolved for the variant, by `namespace::name`.
    pub enums: BTreeMap<String, ResolvedEnum>,
}

/// The parsed input files, each with its path relative to the contract file's directory.
struct Loaded {
    contract: ContractFile,
    namespaces: Vec<NamespaceFile>,
    units: Option<Units>,
    files: Vec<(String, Vec<u8>)>,
    shadow: Shadow,
}

/// Load and resolve the model whose contract file is `contract_path`, for `variant`
/// (`None`: the base values, as the all-variants keyspace shows them).
///
/// v2: declared variants are checked and recorded.
/// v3: the unit vocabulary is recorded.
/// v4: whether `--variant` names a declared variant is the caller's check
/// (variants::check_selected), so the base can be resolved too.
/// v5: the error groups, resolved for the variant.
/// v6: grants are checked against the key paths of every variant.
/// v7: the shadow wire mapping, resolved and checked for the variant.
/// v8: every resolved enum is kept; rights resolved by principal_rights.
/// v9: the contract-level rows' variants are checked too.
/// v10: the enum constants are checked.
/// v11: the internal row constants are checked.
/// v12: the struct value header names are checked, every check through check_resolved.
/// v13: the log streams' per-variant entries are checked too.
/// v14: and the core's log stream, resolved.
/// @version 14
/// @return the tables and their refusals, or why the model could not be read.
/// @req REQ-ING-LOG-006
/// @req REQ-ING-LOG-004
/// @req REQ-ING-HDR-002
/// @req REQ-ING-TIV-017
/// @req REQ-ING-DSH-008
/// @req REQ-ING-SHD-001
/// @req REQ-ING-SIZ-001
/// @req REQ-ING-PRS-001
/// @req REQ-ING-ERR-001
/// @req REQ-ING-TIV-015
/// @req REQ-ING-TIV-002
/// @req REQ-ING-TIV-003
/// @req REQ-ING-CMD-001
pub fn build(contract_path: &Path, variant: Option<&str>) -> Result<(Tables, Errors), String> {
    let loaded = load(contract_path, variant)?;
    let mut errors = Errors::new();
    check_variant_entries(&loaded, &mut errors);
    let (keys, commands, enums) = collect_keys(&loaded, variant, &mut errors);
    let known = declared_paths(&loaded);
    let (principals, rights) =
        principal_rights(loaded.contract.principals, &known, &keys, &mut errors);
    let reserved = reserved_ids(&loaded.contract.reserved);
    let core_log = loaded
        .contract
        .core_log
        .map(|l| log_streams::resolve(OWNER_CORE, l, &mut errors));
    let meta = loaded.contract.contract;
    let tables = Tables {
        layout: loaded.namespaces.iter().map(layout).collect(),
        version: Version {
            major: meta.major,
            minor: meta.minor,
        },
        patch: meta.patch,
        name: meta.name,
        variant: variant.unwrap_or_default().to_string(),
        variants: meta.variants,
        units: loaded.units,
        error_groups: error_groups::resolve(&loaded.contract.error_groups, variant),
        model_hash: model_hash(&loaded.files, variant.unwrap_or_default()),
        keys,
        reserved,
        principals,
        core_log,
        rights,
        commands,
        uid_caps: loaded.contract.uid_caps,
        bounds: loaded.contract.bounds,
        shadow: loaded.shadow,
        enums,
    };
    check_resolved(&tables, &mut errors);
    Ok((tables, errors))
}

/// Refuse a per-variant entry for an undeclared variant: in the namespace files, the
/// contract-level shadow rows and the log streams.
///
/// v2: the core's log stream too.
/// @version 2
/// @req REQ-ING-TIV-015
/// @req REQ-ING-LOG-004
/// @req REQ-ING-LOG-006
fn check_variant_entries(loaded: &Loaded, errors: &mut Errors) {
    let c = &loaded.contract;
    let mut uses = loaded.shadow.variant_uses.clone();
    uses.extend(log_streams::variant_uses(
        &c.principals,
        c.core_log.as_ref(),
    ));
    let declared = &loaded.contract.contract.variants;
    variants::check_entries(declared, &loaded.namespaces, &uses, errors);
}

/// The checks of the resolved tables: the shadow mapping and every generated name (enum
/// constants, internal row constants, struct value headers).
///
/// @version 1
/// @req REQ-ING-SHD-001
/// @req REQ-ING-TIV-017
/// @req REQ-ING-DSH-008
/// @req REQ-ING-HDR-002
fn check_resolved(tables: &Tables, errors: &mut Errors) {
    shadow::check(tables, errors);
    super::constants::check(tables, errors);
    super::internal_ids::check(tables, errors);
    super::header::check(tables, errors);
}

/// The principals, their grants checked against every declared key path, and their resolved
/// rights on `keys`.
///
/// @version 1
/// @req REQ-ING-TIV-011
/// @req REQ-ING-PRS-001
fn principal_rights(
    defs: Vec<PrincipalDef>,
    known: &BTreeSet<String>,
    keys: &[Key],
    errors: &mut Errors,
) -> (Vec<Principal>, Vec<Vec<u8>>) {
    let (principals, grants) = rights::principals(defs, errors);
    rights::check_grants(&principals, &grants, known, errors);
    let rights = rights::resolve(&principals, &grants, keys, errors);
    (principals, rights)
}

/// The reserved IDs, sorted, always including ID 0.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn reserved_ids(declared: &[Reserved]) -> Vec<u32> {
    let mut reserved: Vec<u32> = declared.iter().map(|r| r.id).collect();
    reserved.push(0);
    reserved.sort_unstable();
    reserved.dedup();
    reserved
}

/// The layout record of one namespace file.
///
/// @version 1
/// @req REQ-ING-TIV-010
fn layout(ns: &NamespaceFile) -> NsLayout {
    NsLayout {
        name: ns.meta.id.clone(),
        id: ns.meta.namespace_id,
        classes: ns
            .classes
            .iter()
            .map(|c| (c.id.clone(), c.class_index))
            .collect(),
    }
}

/// Read and parse the contract file, every `.toml` in its namespace directory and the unit
/// vocabulary it names; every one is an input of the model hash.
///
/// v2: the unit vocabulary.
/// v3: the contract-level shadow rows, taken out of the contract file; uid caps sorted by uid.
/// v4: the shadow command rows of the variant, from the namespace files.
/// @version 4
/// @req REQ-ING-TIV-002
/// @req REQ-ING-RNG-001
/// @req REQ-ING-SHD-002
/// @req REQ-ING-CMD-001
fn load(contract_path: &Path, variant: Option<&str>) -> Result<Loaded, String> {
    let base = contract_path.parent().unwrap_or(Path::new("."));
    let text = read(contract_path)?;
    let mut contract: ContractFile = parse(contract_path, &text)?;
    contract.uid_caps.sort_by_key(|c| c.uid);
    let mut files = vec![(file_name(contract_path), text.into_bytes())];
    let namespaces = load_namespaces(base, &contract.contract.namespace_dir, &mut files)?;
    let shadow = shadow::rows(&mut contract, &namespaces, variant);
    let units = load_units(base, contract.contract.units.as_deref(), &mut files)?;
    files.sort();
    Ok(Loaded {
        contract,
        namespaces,
        units,
        shadow,
        files,
    })
}

/// Parse every `.toml` of the namespace directory `dir` (relative to `base`); their bytes join
/// the hash inputs under their relative paths.
///
/// @version 1
/// @req REQ-ING-TIV-002
fn load_namespaces(
    base: &Path,
    dir: &str,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<Vec<NamespaceFile>, String> {
    let mut namespaces = Vec::new();
    for path in namespace_files(&base.join(dir))? {
        let text = read(&path)?;
        namespaces.push(parse(&path, &text)?);
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        files.push((rel, text.into_bytes()));
    }
    Ok(namespaces)
}

/// The unit vocabulary `rel` (relative to `base`), when named; its bytes join the hash inputs.
///
/// @version 1
/// @req REQ-ING-RNG-001
fn load_units(
    base: &Path,
    rel: Option<&str>,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<Option<Units>, String> {
    let Some(rel) = rel else {
        return Ok(None);
    };
    let path = base.join(rel);
    let text = read(&path)?;
    let parsed: UnitsFile = parse(&path, &text)?;
    files.push((rel.to_string(), text.into_bytes()));
    Ok(Some(parsed.units))
}

/// The sorted `.toml` files of `dir`.
///
/// @version 1
/// @req REQ-ING-TIV-002
fn namespace_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{}: no namespace .toml files", dir.display()));
    }
    Ok(paths)
}

/// Read a file, naming it in the error.
///
/// @version 1
/// @req REQ-ING-TIV-002
fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parse TOML into `T`, naming the file in the error.
///
/// @version 1
/// @req REQ-ING-TIV-002
fn parse<T: serde::de::DeserializeOwned>(path: &Path, text: &str) -> Result<T, String> {
    toml::from_str(text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The file's name, for the model hash.
///
/// @version 1
/// @req REQ-ING-TIV-013
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Model hash (diagnostic): SHA-256 over every input as `path 0x00 u64le(len) bytes`, in path
/// order, then `0x00 variant`.
///
/// @version 1
/// @req REQ-ING-TIV-013
pub fn model_hash(files: &[(String, Vec<u8>)], variant: &str) -> String {
    let mut hasher = Sha256::new();
    for (path, bytes) in files {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hasher.update([0u8]);
    hasher.update(variant.as_bytes());
    hex(&hasher.finalize())
}

/// Lower-case hex of `bytes`.
///
/// @version 1
/// @req REQ-ING-TIV-013
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every key of the model (declared and command-generated), sorted by ID, and the commands.
///
/// v2: each namespace's enums are resolved once, for the variant.
/// v3: keys see the unit vocabulary.
/// v4: keys and commands limited to other variants are left out.
/// v5: and every resolved enum, by `namespace::name`.
/// @version 5
/// @req REQ-ING-PRS-001
/// @req REQ-ING-TIV-003
/// @req REQ-ING-TIV-005
/// @req REQ-ING-TIV-016
/// @req REQ-ING-TOP-001
fn collect_keys(
    loaded: &Loaded,
    variant: Option<&str>,
    errors: &mut Errors,
) -> (Vec<Key>, Vec<Command>, BTreeMap<String, ResolvedEnum>) {
    let mut keys = Vec::new();
    let mut commands = Vec::new();
    let mut all = BTreeMap::new();
    for ns in &loaded.namespaces {
        let enums = enums::resolve_all(ns, &loaded.contract.contract.variants, variant, errors);
        let named = enums
            .iter()
            .map(|(n, e)| (format!("{}::{n}", ns.meta.id), e));
        all.extend(named.map(|(n, e)| (n, e.clone())));
        for class in &ns.classes {
            let ctx = ClassCtx {
                ns,
                class,
                enums: &enums,
                units: loaded.units.as_ref(),
                variant,
                default_burst: loaded.contract.bounds.default_rate_burst,
            };
            class_keys(&ctx, (&mut keys, &mut commands), errors);
        }
    }
    keys.sort_by_key(|k| k.id);
    commands.sort_by_key(|c| c.keys[0]);
    (keys, commands, all)
}

/// The keys and commands of one class that exist on the context's variant.
///
/// v2: each command's outcome enum.
/// @version 2
/// @req REQ-ING-TIV-003
/// @req REQ-ING-PRS-001
/// @req REQ-ING-TIV-018
fn class_keys(
    ctx: &ClassCtx,
    (keys, commands): (&mut Vec<Key>, &mut Vec<Command>),
    errors: &mut Errors,
) {
    let here = |only: &Option<Vec<String>>| present_on(only.as_deref(), ctx.variant);
    let declared = ctx.class.keys.iter().filter(|d| here(&d.variants));
    keys.extend(declared.map(|def| declared_key(ctx, def, errors)));
    for def in ctx.class.commands.iter().filter(|d| here(&d.variants)) {
        let three = command_keys(ctx, def, errors);
        let outcome = command_outcome(ctx, def, errors);
        commands.push(command_row(def, &three, outcome));
        keys.extend(three);
    }
}

/// The path of every key the model declares, on any variant (command keys included).
///
/// @version 1
/// @req REQ-ING-PRS-001
fn declared_paths(loaded: &Loaded) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for ns in &loaded.namespaces {
        for class in &ns.classes {
            let path = |id: &str| format!("{}.{}.{id}", ns.meta.id, class.id);
            paths.extend(class.keys.iter().map(|k| path(&k.id)));
            for cmd in &class.commands {
                paths.extend(command_key_names(cmd).iter().map(|n| path(n)));
            }
        }
    }
    paths
}
/// Whether a key, command or shadow command row limited to `only` exists on `variant` (the base
/// has every one).
///
/// v2: visible to the shadow rows.
/// @version 2
/// @req REQ-ING-PRS-001
/// @req REQ-ING-CMD-001
pub(super) fn present_on(only: Option<&[String]>, variant: Option<&str>) -> bool {
    match (only, variant) {
        (Some(list), Some(v)) => list.iter().any(|x| x == v),
        _ => true,
    }
}

/// Whether two variant lists share a variant (absent: every variant).
///
/// v2: moved from the command rows, shared with the shadow paths.
/// @version 2
/// @req REQ-ING-CMD-001
/// @req REQ-ING-SHD-004
pub(super) fn variants_overlap(a: Option<&[String]>, b: Option<&[String]>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x.iter().any(|v| y.contains(v)),
        _ => true,
    }
}

/// The `dmt_commands` row of a command type.
///
/// v2: and why it is internal.
/// v3: and its resolved outcome enum.
/// @version 3
/// @req REQ-ING-TIV-005
/// @req REQ-ING-CMD-001
/// @req REQ-ING-TIV-018
fn command_row(def: &CommandDef, keys: &[Key; 3], outcome: Option<ResolvedEnum>) -> Command {
    Command {
        path: keys[0]
            .command
            .as_ref()
            .map(|(p, _)| p.clone())
            .unwrap_or_default(),
        keys: [keys[0].id, keys[1].id, keys[2].id],
        max_payload: def.payload_max,
        max_result: def.result_max,
        max_edits: def.max_edits,
        dedupe_ring: def.dedupe_ring,
        owner: def.owner.clone(),
        persist: def.persistence,
        internal: def.internal.clone(),
        outcome,
    }
}

impl Tables {
    /// Whether these are the base tables of a model that declares variants: the all-variants
    /// view, whose per-variant checks are left to each variant's own pass.
    ///
    /// @version 1
    /// @req REQ-ING-SHD-004
    pub fn all_variants_view(&self) -> bool {
        self.variant.is_empty() && !self.variants.is_empty()
    }

    /// Index of the key with path `path`.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn key_index(&self, path: &str) -> Option<usize> {
        self.keys.iter().position(|k| k.path == path)
    }

    /// Index of the principal named `name`.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-006
    pub fn principal_index(&self, name: &str) -> Option<usize> {
        self.principals.iter().position(|p| p.name == name)
    }

    /// The key ID mapped to each well-known role, `0` when unmapped.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-009
    pub fn wk(&self) -> [u32; WK_RULES.len()] {
        let mut ids = [0u32; WK_RULES.len()];
        for key in &self.keys {
            if let Some(role) = key.wk {
                ids[role] = key.id;
            }
        }
        ids
    }

    /// The key ID behind an optional key path, `0` when absent or unknown.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-009
    pub fn id_of(&self, path: Option<&str>) -> u32 {
        path.and_then(|p| self.key_index(p))
            .map_or(0, |i| self.keys[i].id)
    }

    /// Whether `principal` may write the key at `index`.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-011
    pub fn writes(&self, principal: usize, index: usize) -> bool {
        self.rights[principal][index] & rights::WRITE != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_hash_matches_an_independent_vector() {
        // printf 'a\000\001\000\000\000\000\000\000\000x\000v' | sha256sum
        let files = vec![("a".to_string(), b"x".to_vec())];
        assert_eq!(
            model_hash(&files, "v"),
            "913dffd426bb94722008ef80ff188dab32c28a371ee384dca1dca7a27c1385f4"
        );
    }

    #[test]
    fn reserved_ids_always_hold_zero_sorted_once() {
        let declared = [Reserved { id: 9 }, Reserved { id: 0 }, Reserved { id: 3 }];
        assert_eq!(reserved_ids(&declared), vec![0, 3, 9]);
        assert_eq!(reserved_ids(&[]), vec![0]);
    }
}
