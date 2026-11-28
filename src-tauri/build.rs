use std::path::Path;

/// The app's command names, read back from the capability file that
/// `scripts/bindings.sh` writes from the command registry
/// (`allow-album-open` is the command `album_open`). Declaring them in the
/// app manifest means a command the capability does not grant cannot be
/// called (docs/08-desktop-and-release.md, "Capabilities").
fn app_commands(capability: &str) -> &'static [&'static str] {
    let mut names = Vec::new();
    for part in capability.split('"').skip(1).step_by(2) {
        if let Some(name) = part.strip_prefix("allow-") {
            let name: &'static str = Box::leak(name.replace('-', "_").into_boxed_str());
            names.push(name);
        }
    }
    Box::leak(names.into_boxed_slice())
}

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by Cargo");
    // Tauri would put its manifest into the program only. Test binaries link
    // Tauri too, and without the manifest's common-controls v6 dependency
    // they fail to start on Windows (STATUS_ENTRYPOINT_NOT_FOUND). So the
    // manifest is linked into every binary of this crate instead, as the
    // tauri-build documentation describes.
    let mut windows = tauri_build::WindowsAttributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let manifest = Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    println!("cargo:rerun-if-changed=capabilities/main.json");
    let capability = std::fs::read_to_string(Path::new(&dir).join("capabilities/main.json"))
        .expect("src-tauri/capabilities/main.json is missing; run scripts/bindings.sh");
    let commands = app_commands(&capability);
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(windows)
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("tauri-build failed");
}
