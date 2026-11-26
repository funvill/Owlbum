use std::path::Path;

fn main() {
    // Tauri would put its manifest into the program only. Test binaries link
    // Tauri too, and without the manifest's common-controls v6 dependency
    // they fail to start on Windows (STATUS_ENTRYPOINT_NOT_FOUND). So the
    // manifest is linked into every binary of this crate instead, as the
    // tauri-build documentation describes.
    let mut windows = tauri_build::WindowsAttributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by Cargo");
        let manifest = Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build failed");
}
