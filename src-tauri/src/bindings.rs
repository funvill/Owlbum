//! Generates the page's TypeScript bindings from the command registry.
//!
//! The output is `ui/src/api/bindings/`:
//!
//! - one file per Rust type, written by ts-rs (`AppVersion.ts`, ...);
//! - one file per area (`app.ts`, ...) with that area's `Commands` and
//!   `Events` maps: command name to `{ args, result }`, event name to payload;
//! - `index.ts`, which joins the areas into `Commands` and `Events`.
//!
//! The files are committed. The test `src-tauri/tests/bindings.rs` fails when
//! they are stale; `scripts/bindings.sh` rewrites them.

use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use owlbum_core::AppError;
use tauri::Runtime;
use ts_rs::{Config, TS, TypeVisitor};

use crate::commands::registry::{Module, TypeRef};

/// The first line of every file this module writes itself.
const NOTE: &str = "// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;\n// run scripts/bindings.sh after changing a command, an event or a type.\n";

/// The ts-rs settings. 64-bit integers are plain numbers: ids and sizes stay
/// below 2^53, and JSON carries them as numbers.
pub fn config() -> Config {
    Config::new().with_large_int("number")
}

/// The generated files, keyed by path relative to the bindings folder with
/// `/` separators.
pub struct Files {
    cfg: Config,
    seen: HashSet<TypeId>,
    /// Path to contents.
    pub files: BTreeMap<String, String>,
}

impl TypeVisitor for Files {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        if let Some(path) = T::output_path() {
            let key = slashes(&path);
            let text = T::export_to_string(&self.cfg)
                .unwrap_or_else(|e| panic!("cannot generate {key}: {e}"));
            match self.files.get(&key) {
                Some(existing) if *existing != text => panic!(
                    "two Rust types generate {key}; rename one, or give it #[ts(rename = \"...\")]"
                ),
                _ => {
                    self.files.insert(key, text);
                }
            }
        }
        T::visit_dependencies(self);
        T::visit_generics(self);
    }
}

/// Adds `T` and everything it depends on to `files`.
pub fn collect<T: TS + 'static + ?Sized>(files: &mut Files) {
    files.visit::<T>();
}

/// The types a module file imports: name to path relative to the bindings
/// folder, without `.ts`.
pub struct Imports {
    cfg: Config,
    /// Type name to import path.
    pub names: BTreeMap<String, String>,
}

impl TypeVisitor for Imports {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if let Some(path) = T::output_path() {
            let path = slashes(&path);
            let path = path.trim_end_matches(".ts").to_owned();
            self.names.insert(T::ident(&self.cfg), path);
        }
        T::visit_generics(self);
    }
}

/// Adds the types a module file must import to name `T`.
pub fn imports<T: TS + 'static + ?Sized>(imports: &mut Imports) {
    imports.visit::<T>();
}

fn slashes(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `app` to `App`, `tagging` to `Tagging`.
fn pascal(prefix: &str) -> String {
    prefix
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// Renders every binding file for `modules`.
pub fn render<R: Runtime>(modules: &[Module<R>]) -> BTreeMap<String, String> {
    let cfg = config();
    let mut files = Files {
        cfg: config(),
        seen: HashSet::new(),
        files: BTreeMap::new(),
    };
    // Every command can fail with AppError, so the page always has it.
    let always = [TypeRef::of::<AppError>()];
    for t in &always {
        (t.collect)(&mut files);
    }

    let mut index_imports = String::new();
    let mut commands = Vec::new();
    let mut events = Vec::new();
    let mut modules: Vec<&Module<R>> = modules.iter().collect();
    modules.sort_by_key(|m| m.prefix);

    for m in modules {
        let mut imports = Imports {
            cfg: config(),
            names: BTreeMap::new(),
        };
        let mut body = String::new();
        let area = pascal(m.prefix);

        let mut cmds = m.commands.clone();
        cmds.sort_by_key(|c| c.name);
        body.push_str(&format!("export type {area}Commands = {{\n"));
        for c in &cmds {
            let args = if c.args.is_empty() {
                "Record<string, never>".to_owned()
            } else {
                let fields: Vec<String> = c
                    .args
                    .iter()
                    .map(|(name, t)| format!("{name}: {}", (t.name)(&cfg)))
                    .collect();
                format!("{{ {} }}", fields.join("; "))
            };
            for (_, t) in &c.args {
                (t.collect)(&mut files);
                (t.imports)(&mut imports);
            }
            (c.result.collect)(&mut files);
            (c.result.imports)(&mut imports);
            body.push_str(&format!(
                "  {}: {{ args: {args}; result: {} }};\n",
                c.name,
                (c.result.name)(&cfg)
            ));
        }
        body.push_str("};\n\n");

        let mut evs = m.events.clone();
        evs.sort_by_key(|e| e.name);
        body.push_str(&format!("export type {area}Events = {{\n"));
        for e in &evs {
            (e.payload.collect)(&mut files);
            (e.payload.imports)(&mut imports);
            body.push_str(&format!("  {}: {};\n", e.name, (e.payload.name)(&cfg)));
        }
        body.push_str("};\n");

        let mut text = String::from(NOTE);
        if !imports.names.is_empty() {
            text.push('\n');
        }
        for (name, path) in &imports.names {
            text.push_str(&format!("import type {{ {name} }} from \"./{path}\";\n"));
        }
        text.push('\n');
        text.push_str(&body);
        files.files.insert(format!("{}.ts", m.prefix), text);

        index_imports.push_str(&format!(
            "import type {{ {area}Commands, {area}Events }} from \"./{}\";\n",
            m.prefix
        ));
        commands.push(format!("{area}Commands"));
        events.push(format!("{area}Events"));
    }

    let mut index = String::from(NOTE);
    index.push('\n');
    index.push_str(&index_imports);
    index.push_str("\n/** Every command: name to `{ args, result }`. */\n");
    index.push_str(&format!(
        "export type Commands = {};\n",
        commands.join(" &\n  ")
    ));
    index.push_str("\n/** Every event: name to payload. */\n");
    index.push_str(&format!(
        "export type Events = {};\n",
        events.join(" &\n  ")
    ));
    files.files.insert("index.ts".to_owned(), index);
    assert_no_case_clash(&files.files);
    files.files
}

/// Panics when two files differ only in case (`Query.ts` and `query.ts`):
/// on Windows and macOS they are one file, and one would overwrite the
/// other.
fn assert_no_case_clash(files: &BTreeMap<String, String>) {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    for key in files.keys() {
        if let Some(other) = seen.insert(key.to_lowercase(), key) {
            panic!("{other} and {key} differ only in case; give one #[ts(export_to = \"...\")]");
        }
    }
}

/// The page's one capability file, `src-tauri/capabilities/main.json`
/// (docs/08-desktop-and-release.md, "Capabilities"): the window `main`, local
/// content only, every command in the registry by name, and listening for
/// events. Nothing else, and no plugin. `build.rs` reads the command names
/// back from it for the app manifest, so a command left out of the file
/// cannot be called. `scripts/bindings.sh` writes it with the bindings.
pub fn capability<R: Runtime>(modules: &[Module<R>]) -> String {
    let mut names: Vec<&str> = modules
        .iter()
        .flat_map(|m| m.commands.iter().map(|c| c.name))
        .collect();
    names.sort_unstable();
    let mut permissions = vec![
        "core:event:allow-listen".to_string(),
        "core:event:allow-unlisten".to_string(),
    ];
    permissions.extend(
        names
            .iter()
            .map(|n| format!("allow-{}", n.replace('_', "-"))),
    );
    let value = serde_json::json!({
        "$schema": "../gen/schemas/desktop-schema.json",
        "identifier": "main",
        "description": "Generated by scripts/bindings.sh from the command registry. The main window: the app's own commands, listed by name, and listening for its events. No plugin is granted to the page.",
        "windows": ["main"],
        "local": true,
        "permissions": permissions,
    });
    let mut text = serde_json::to_string_pretty(&value).expect("a capability is JSON");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::registry::{CommandSig, EventSig};
    use owlbum_core::Event;
    use serde::Serialize;
    use tauri::test::MockRuntime;

    #[derive(Serialize, TS)]
    struct Ping {
        n: u32,
    }
    impl Event for Ping {
        const NAME: &'static str = "ping";
    }

    #[derive(Serialize, TS)]
    struct Row {
        id: i64,
    }

    fn demo() -> Module<MockRuntime> {
        Module {
            prefix: "demo",
            handler: Box::new(|_| false),
            commands: vec![
                CommandSig {
                    name: "demo_rows",
                    args: vec![
                        ("token", TypeRef::of::<u64>()),
                        ("ids", TypeRef::of::<Vec<i64>>()),
                    ],
                    result: TypeRef::of::<Vec<Row>>(),
                },
                CommandSig {
                    name: "demo_clear",
                    args: vec![],
                    result: TypeRef::of::<()>(),
                },
            ],
            events: vec![EventSig::of::<Ping>()],
        }
    }

    #[test]
    #[should_panic(expected = "differ only in case")]
    fn files_that_differ_only_in_case_are_refused() {
        let mut files = BTreeMap::new();
        files.insert("Query.ts".to_owned(), String::new());
        files.insert("query.ts".to_owned(), String::new());
        assert_no_case_clash(&files);
    }

    #[test]
    fn an_area_file_maps_commands_and_events_to_types() {
        let files = render(&[demo()]);
        let want = [
            NOTE,
            "\n",
            "import type { Ping } from \"./Ping\";\n",
            "import type { Row } from \"./Row\";\n",
            "\n",
            "export type DemoCommands = {\n",
            "  demo_clear: { args: Record<string, never>; result: null };\n",
            "  demo_rows: { args: { token: number; ids: Array<number> }; result: Array<Row> };\n",
            "};\n",
            "\n",
            "export type DemoEvents = {\n",
            "  ping: Ping;\n",
            "};\n",
        ]
        .concat();
        assert_eq!(files["demo.ts"], want);
    }

    #[test]
    fn every_reachable_type_gets_a_file() {
        let files = render(&[demo()]);
        let names: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "AppError.ts",
                "ErrorCode.ts",
                "Ping.ts",
                "Row.ts",
                "demo.ts",
                "index.ts",
                "serde_json/JsonValue.ts"
            ]
        );
        assert!(
            files["Row.ts"].contains("export type Row = { id: number, };"),
            "{}",
            files["Row.ts"]
        );
    }

    #[test]
    fn the_index_joins_the_areas() {
        let files = render(&[demo()]);
        let index = &files["index.ts"];
        assert!(index.contains("import type { DemoCommands, DemoEvents } from \"./demo\";"));
        assert!(index.contains("export type Commands = DemoCommands;"));
        assert!(index.contains("export type Events = DemoEvents;"));
    }

    #[test]
    fn pascal_case_of_prefixes() {
        assert_eq!(pascal("app"), "App");
        assert_eq!(pascal("tagging"), "Tagging");
        assert_eq!(pascal("two_words"), "TwoWords");
    }
}
