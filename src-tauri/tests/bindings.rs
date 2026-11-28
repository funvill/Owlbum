//! Fails when the committed TypeScript bindings (ui/src/api/bindings) do not
//! match the Rust command registry. With `OWLBUM_WRITE_BINDINGS=1` it
//! rewrites them instead; `scripts/bindings.sh` does that.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::test::MockRuntime;

fn bindings_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/api/bindings")
}

fn read_tree(dir: &Path, prefix: &str, out: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let key = format!("{prefix}{name}");
        let path = entry.path();
        if path.is_dir() {
            read_tree(&path, &format!("{key}/"), out);
        } else {
            let text = fs::read_to_string(&path).unwrap_or_default();
            out.insert(key, text.replace("\r\n", "\n"));
        }
    }
}

#[test]
fn typescript_bindings_are_current() {
    let want = owlbum_lib::bindings::render(&owlbum_lib::commands::modules::<MockRuntime>());
    let dir = bindings_dir();

    if std::env::var_os("OWLBUM_WRITE_BINDINGS").is_some_and(|v| !v.is_empty()) {
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        for (path, text) in &want {
            let file = dir.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, text).unwrap();
        }
        return;
    }

    let mut have = BTreeMap::new();
    read_tree(&dir, "", &mut have);
    let mut stale: Vec<String> = Vec::new();
    for (path, text) in &want {
        match have.get(path) {
            None => stale.push(format!("{path} is missing")),
            Some(t) if t != text => stale.push(format!("{path} differs")),
            Some(_) => {}
        }
    }
    for path in have.keys().filter(|p| !want.contains_key(*p)) {
        stale.push(format!("{path} should not exist"));
    }
    assert!(
        stale.is_empty(),
        "The TypeScript bindings in ui/src/api/bindings are stale:\n  {}\nRun scripts/bindings.sh and commit the result.",
        stale.join("\n  ")
    );
}

/// The capability file grants the page exactly the registry's commands
/// (docs/08-desktop-and-release.md, "Capabilities"). `scripts/bindings.sh`
/// rewrites it with the bindings.
#[test]
fn the_capability_file_is_current() {
    let want = owlbum_lib::bindings::capability(&owlbum_lib::commands::modules::<MockRuntime>());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/main.json");
    if std::env::var_os("OWLBUM_WRITE_BINDINGS").is_some_and(|v| !v.is_empty()) {
        fs::write(&path, &want).unwrap();
        return;
    }
    let have = fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "src-tauri/capabilities/main.json is stale. Run scripts/bindings.sh and commit the result."
    );
}
