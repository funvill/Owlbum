//! Owlbum's desktop shell, the crate `owlbum`.
//!
//! It is thin: each command validates its arguments, calls one core function
//! and maps the result or error to a reply. It also owns what only a desktop
//! shell can do: windows, menus, dialogs, drag-out, trash, clipboard, reveal,
//! the updater and single-instance handling.

pub mod bindings;
pub mod commands;
pub mod events;
pub mod keymap;

use std::sync::Arc;

use owlbum_core::appdata::log::{self, LogFile, Logger};
use owlbum_core::appdata::{self, AppData};
use tauri::Runtime;

/// Adds Owlbum's commands to a Tauri builder. `run` uses it, and shell tests
/// use it with Tauri's mock runtime.
pub fn setup<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(commands::registry::invoke_handler(commands::modules()))
}

/// Gives the app its open app data folder, which the `app_` settings and
/// recent-list commands use. `run` passes the real folder; shell tests pass
/// one in a temp folder, so no test touches a real install's data.
pub fn with_app_data<R: Runtime>(builder: tauri::Builder<R>, data: AppData) -> tauri::Builder<R> {
    builder.manage(data)
}

/// The identifier of a development build (`src-tauri/tauri.dev.conf.json`).
pub const DEV_IDENTIFIER: &str = "com.funvill.owlbum.dev";

/// Whether this is a development build: one compiled with debug assertions
/// (docs/08-desktop-and-release.md), or one carrying the development
/// identifier, such as a release-profile build started with the dev config.
/// A development build uses the `Dev` app data folder, so it never touches
/// an installed copy's data.
pub fn is_dev_build(debug_assertions: bool, identifier: &str) -> bool {
    debug_assertions || identifier == DEV_IDENTIFIER || identifier.ends_with(".dev")
}

/// Starts the program.
///
/// # Panics
///
/// When the app data folder cannot be found, created or opened, or Tauri
/// cannot start. Failure handling replaces these with the fatal error
/// dialog (roadmap issue 61).
pub fn run() {
    let context = tauri::generate_context!();
    let dev = is_dev_build(cfg!(debug_assertions), &context.config().identifier);

    // Startup sequence, docs/08-desktop-and-release.md, steps 1, 2, 4, 5.
    let location = appdata::locate(appdata::Platform::CURRENT, dev, |k| std::env::var_os(k))
        .expect("Owlbum could not find a folder for its settings");
    let file = Arc::new(LogFile::open(
        &log::folders(&location.dir, &std::env::temp_dir()),
        dev,
    ));
    Logger::new(
        Arc::clone(&file),
        log::debug_requested(std::env::var_os(log::LEVEL_VAR).as_deref()),
    )
    .install();
    let data = AppData::open(&location.dir).expect("Owlbum could not open its settings");
    tracing::info!(
        target: "app",
        "start version={} platform={} arch={} dev={} identifier={} data={} data_overridden={}",
        owlbum_core::app::version().version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        dev,
        context.config().identifier,
        data.dir().display(),
        location.overridden,
    );
    if let Some(moved) = data.set_aside() {
        tracing::warn!(
            target: "app",
            "app.sqlite was damaged; set aside as {} and remade",
            moved.display()
        );
    }

    with_app_data(setup(tauri::Builder::default()), data)
        .run(context)
        .expect("Owlbum could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debug_build_is_a_development_build_whatever_its_identifier() {
        assert!(is_dev_build(true, "com.funvill.owlbum"));
        assert!(is_dev_build(true, DEV_IDENTIFIER));
    }

    #[test]
    fn a_release_build_with_the_dev_identifier_is_a_development_build() {
        assert!(is_dev_build(false, DEV_IDENTIFIER));
        assert!(is_dev_build(false, "com.example.other.dev"));
    }

    #[test]
    fn only_an_installed_release_build_uses_the_real_folder() {
        assert!(!is_dev_build(false, "com.funvill.owlbum"));
        assert!(!is_dev_build(false, "com.funvill.owlbum.devtools"));
    }

    #[test]
    fn the_dev_identifier_matches_the_dev_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.dev.conf.json")).unwrap();
        assert_eq!(conf["identifier"], DEV_IDENTIFIER);
        let main: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(!is_dev_build(false, main["identifier"].as_str().unwrap()));
    }
}
