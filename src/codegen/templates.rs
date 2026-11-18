//! Templates compiled into the binary, and the template directories ingot searches.
//!
//! `--templates` / `INGOT_TEMPLATES_DIR` replaces the embedded set with a directory. Model mode
//! without either searches upstream's template directories first and falls back to the
//! embedded set; contract mode and --table-types use the embedded set (so there the ingot
//! commit alone determines the output).

use std::path::{Path, PathBuf};
use tera::Tera;

/// Every file in `templates/`, under its file name. A test checks the list is complete.
const EMBEDDED: &[(&str, &str)] = &[
    (
        "CMakeLists.txt",
        include_str!("../../templates/CMakeLists.txt"),
    ),
    (
        "boolean_storage.c",
        include_str!("../../templates/boolean_storage.c"),
    ),
    (
        "boolean_storage.h",
        include_str!("../../templates/boolean_storage.h"),
    ),
    ("dm.c", include_str!("../../templates/dm.c")),
    ("dm.h", include_str!("../../templates/dm.h")),
    (
        "dm_boolean_storage.h",
        include_str!("../../templates/dm_boolean_storage.h"),
    ),
    ("dm_enums.h", include_str!("../../templates/dm_enums.h")),
    ("dm_helpers.c", include_str!("../../templates/dm_helpers.c")),
    ("dm_helpers.h", include_str!("../../templates/dm_helpers.h")),
    (
        "dm_integer_storage.h",
        include_str!("../../templates/dm_integer_storage.h"),
    ),
    ("dm_key.h", include_str!("../../templates/dm_key.h")),
    (
        "dm_key_definitions.h",
        include_str!("../../templates/dm_key_definitions.h"),
    ),
    (
        "dm_key_event_list.hpp",
        include_str!("../../templates/dm_key_event_list.hpp"),
    ),
    (
        "dm_key_events.c",
        include_str!("../../templates/dm_key_events.c"),
    ),
    (
        "dm_key_events.h",
        include_str!("../../templates/dm_key_events.h"),
    ),
    (
        "dm_key_events.hpp",
        include_str!("../../templates/dm_key_events.hpp"),
    ),
    (
        "dm_key_events_wrapper.cpp",
        include_str!("../../templates/dm_key_events_wrapper.cpp"),
    ),
    (
        "dm_key_events_wrapper.hpp",
        include_str!("../../templates/dm_key_events_wrapper.hpp"),
    ),
    (
        "dm_key_has_event.hpp",
        include_str!("../../templates/dm_key_has_event.hpp"),
    ),
    (
        "dm_namespace_definitions.h",
        include_str!("../../templates/dm_namespace_definitions.h"),
    ),
    (
        "dm_persistence_storage.h",
        include_str!("../../templates/dm_persistence_storage.h"),
    ),
    ("dm_rust.rs", include_str!("../../templates/dm_rust.rs")),
    (
        "dm_string_storage.h",
        include_str!("../../templates/dm_string_storage.h"),
    ),
    (
        "dmt_constants.h",
        include_str!("../../templates/dmt_constants.h"),
    ),
    ("dmt_tables.c", include_str!("../../templates/dmt_tables.c")),
    ("dmt_tables.h", include_str!("../../templates/dmt_tables.h")),
    ("dmt_types.h", include_str!("../../templates/dmt_types.h")),
    ("dsh_tables.c", include_str!("../../templates/dsh_tables.c")),
    ("dsh_tables.h", include_str!("../../templates/dsh_tables.h")),
    ("dsh_types.h", include_str!("../../templates/dsh_types.h")),
    (
        "keyspace.schema.json",
        include_str!("../../templates/keyspace.schema.json"),
    ),
    (
        "integer_storage.c",
        include_str!("../../templates/integer_storage.c"),
    ),
    (
        "integer_storage.h",
        include_str!("../../templates/integer_storage.h"),
    ),
    (
        "jenkins_hash.c",
        include_str!("../../templates/jenkins_hash.c"),
    ),
    (
        "jenkins_hash.h",
        include_str!("../../templates/jenkins_hash.h"),
    ),
    (
        "key_definitions.h",
        include_str!("../../templates/key_definitions.h"),
    ),
    (
        "persistence_storage.c",
        include_str!("../../templates/persistence_storage.c"),
    ),
    (
        "persistence_storage.h",
        include_str!("../../templates/persistence_storage.h"),
    ),
    (
        "string_storage.c",
        include_str!("../../templates/string_storage.c"),
    ),
    (
        "string_storage.h",
        include_str!("../../templates/string_storage.h"),
    ),
    ("test_dm.c", include_str!("../../templates/test_dm.c")),
];

/// Load the templates: the embedded set, or every file in `dir` when one is given.
///
/// @version 1
/// @req REQ-ING-CLI-002
pub fn load(dir: Option<&Path>) -> Result<Tera, Box<dyn std::error::Error>> {
    match dir {
        Some(dir) => from_dir(dir),
        None => embedded(),
    }
}

/// Load the model-mode templates: `dir` when given (the whole set), else the first template
/// directory on upstream ingot's search path with every template it lacks taken from the
/// embedded set, else the embedded set.
///
/// v2: a searched directory is completed from the embedded set.
/// @version 2
/// @req REQ-ING-CLI-002
/// @return the loaded templates, or the error of an explicit directory that does not exist
pub fn load_searching(dir: Option<&Path>) -> Result<Tera, Box<dyn std::error::Error>> {
    if let Some(dir) = dir {
        log::info!("Templates: {}", dir.display());
        return from_dir(dir);
    }
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent);
    let cwd = std::env::current_dir().ok();
    match first_dir(&search_dirs(exe_dir, cwd.as_deref())) {
        Some(found) => {
            log::info!("Templates: {}", found.display());
            completed(from_dir(&found)?, &found)
        }
        None => {
            log::info!("Templates: built in (no template directory on the search path)");
            embedded()
        }
    }
}

/// `tera`, loaded from the searched directory `dir`, with each embedded template it lacks
/// added from the embedded set and named in a warning.
///
/// @version 1
/// @req REQ-ING-CLI-002
/// @return the completed templates, or the error of an embedded template that fails to parse
fn completed(mut tera: Tera, dir: &Path) -> Result<Tera, Box<dyn std::error::Error>> {
    let present: std::collections::HashSet<String> =
        tera.get_template_names().map(str::to_string).collect();
    for (name, source) in EMBEDDED.iter().filter(|(n, _)| !present.contains(*n)) {
        log::warn!(
            "Template {name} is not in {}; using the built-in copy",
            dir.display()
        );
        tera.add_raw_template(name, source)?;
    }
    Ok(tera)
}

/// The template directories upstream ingot searched without `--templates`, in its order:
/// next to the executable, the FHS share directory beside its bin/, the crate root of a
/// `target/<profile>/` build, the working directory, and `/usr/share/ingot/templates`.
///
/// @version 1
/// @req REQ-ING-CLI-002
/// @return the candidate directories, existing or not
pub(crate) fn search_dirs(exe_dir: Option<&Path>, cwd: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = exe_dir {
        dirs.push(exe_dir.join("templates"));
        dirs.push(exe_dir.join("../share/ingot/templates"));
        if let Some(root) = exe_dir.parent().and_then(Path::parent) {
            dirs.push(root.join("templates"));
        }
    }
    if let Some(cwd) = cwd {
        dirs.push(cwd.join("templates"));
    }
    dirs.push(PathBuf::from("/usr/share/ingot/templates"));
    dirs
}

/// The first of `candidates` that is a directory.
///
/// @version 1
/// @req REQ-ING-CLI-002
/// @return that directory, or None when none exists
fn first_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|d| d.is_dir()).cloned()
}

/// The templates compiled into this binary.
///
/// @version 1
/// @req REQ-ING-CLI-002
fn embedded() -> Result<Tera, Box<dyn std::error::Error>> {
    let mut tera = Tera::default();
    tera.add_raw_templates(EMBEDDED.iter().copied())?;
    Ok(tera)
}

/// Every file in `dir`, which must exist.
///
/// @version 1
/// @req REQ-ING-CLI-002
fn from_dir(dir: &Path) -> Result<Tera, Box<dyn std::error::Error>> {
    if !dir.is_dir() {
        return Err(format!(
            "Specified templates directory does not exist: {}",
            dir.display()
        )
        .into());
    }
    let glob = dir.join("*");
    Ok(Tera::new(glob.to_str().ok_or("invalid template path")?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_template_file_is_embedded() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let on_disk: BTreeSet<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let embedded: BTreeSet<String> = EMBEDDED.iter().map(|(n, _)| n.to_string()).collect();
        assert_eq!(embedded, on_disk, "EMBEDDED must list exactly templates/");
    }

    #[test]
    fn embedded_set_loads_every_template() {
        let tera = load(None).unwrap();
        let names: BTreeSet<&str> = tera.get_template_names().collect();
        assert_eq!(names.len(), EMBEDDED.len());
        assert!(names.contains("dmt_types.h"));
    }

    #[test]
    fn missing_directory_is_an_error() {
        let err = load(Some(Path::new("/nonexistent/ingot-templates"))).unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    #[test]
    fn directory_replaces_the_embedded_set() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("only.h"), "x").unwrap();
        let tera = load(Some(tmp.path())).unwrap();
        let names: Vec<&str> = tera.get_template_names().collect();
        assert_eq!(names, vec!["only.h"]);
    }

    #[test]
    fn search_dirs_follow_upstream_order() {
        let dirs = search_dirs(Some(Path::new("/opt/x/bin")), Some(Path::new("/work")));
        let expected = [
            "/opt/x/bin/templates",
            "/opt/x/bin/../share/ingot/templates",
            "/opt/templates",
            "/work/templates",
            "/usr/share/ingot/templates",
        ];
        assert_eq!(dirs, expected.map(PathBuf::from));
    }

    #[test]
    fn first_existing_directory_wins_and_none_falls_back() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let missing = a.path().join("missing");
        let picked = first_dir(&[missing.clone(), b.path().into(), a.path().into()]);
        assert_eq!(picked.as_deref(), Some(b.path()));
        assert_eq!(first_dir(&[missing]), None);
    }

    /// @brief A searched directory holding `only.h` and its own `dm.h`, completed.
    /// @version 1
    fn completed_dir() -> Tera {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("only.h"), "x").unwrap();
        std::fs::write(tmp.path().join("dm.h"), "from the directory").unwrap();
        completed(from_dir(tmp.path()).unwrap(), tmp.path()).unwrap()
    }

    #[test]
    fn a_searched_directory_is_completed_from_the_embedded_set() {
        let tera = completed_dir();
        let names: BTreeSet<&str> = tera.get_template_names().collect();
        assert_eq!(names.len(), EMBEDDED.len() + 1);
        assert!(names.contains("only.h") && names.contains("dmt_types.h"));
    }

    #[test]
    fn a_template_the_directory_has_wins_over_the_embedded_copy() {
        let dm_h = completed_dir()
            .render("dm.h", &tera::Context::new())
            .unwrap();
        assert_eq!(dm_h, "from the directory");
    }
}
