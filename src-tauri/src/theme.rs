//! The colour theme on the native side (docs/09-visual-design.md, "Colour
//! tokens"; issue 75): the window's own theme, which colours its title bar
//! and tells the webview which scheme the system prefers, and the colour
//! the window is filled with before the page draws.

use owlbum_core::appdata::Theme;
use tauri::window::Color;
use tauri::{Runtime, WebviewWindow};

/// `--bg` of the dark theme, which is also `backgroundColor` in
/// `tauri.conf.json` (the colour before any setting is read).
pub const DARK_BACKGROUND: &str = "#161618";
/// `--bg` of the light theme.
pub const LIGHT_BACKGROUND: &str = "#f4f3f0";

/// The window theme a setting asks for; `None` follows the system.
pub fn native(theme: Theme) -> Option<tauri::Theme> {
    match theme {
        Theme::System => None,
        Theme::Dark => Some(tauri::Theme::Dark),
        Theme::Light => Some(tauri::Theme::Light),
    }
}

/// The window colour (`--bg`) for the theme in use.
pub fn background(shown: tauri::Theme) -> Color {
    let hex = match shown {
        tauri::Theme::Light => LIGHT_BACKGROUND,
        _ => DARK_BACKGROUND,
    };
    let channel = |i: usize| u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0);
    Color(channel(0), channel(1), channel(2), 255)
}

/// Gives the window the theme and fills it with that theme's background.
/// With `system`, the system's current choice decides the colour.
pub fn apply<R: Runtime>(window: &WebviewWindow<R>, theme: Theme) {
    if let Err(e) = window.set_theme(native(theme)) {
        tracing::warn!(target: "app", "could not set the window theme: {e}");
    }
    let shown = window.theme().unwrap_or(tauri::Theme::Dark);
    if let Err(e) = window.set_background_color(Some(background(shown))) {
        tracing::warn!(target: "app", "could not set the window colour: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--bg` in the token file's block for a theme.
    fn token_bg(theme: &str) -> String {
        let tokens = include_str!("../../ui/src/styles/tokens.css");
        let start = tokens
            .find(&format!(":root[data-theme=\"{theme}\"] {{"))
            .unwrap_or_else(|| panic!("tokens.css has a {theme} block"));
        let block = &tokens[start..];
        let block = &block[..block.find('}').unwrap()];
        block
            .lines()
            .find_map(|l| l.trim().strip_prefix("--bg:"))
            .expect("the block sets --bg")
            .trim()
            .trim_end_matches(';')
            .to_owned()
    }

    /// The window is filled with the theme's window colour (`--bg`) before
    /// the page draws, so there is no flash (docs/08, "The window"). The
    /// config holds the dark one, used before the setting is read.
    #[test]
    fn the_window_colours_are_the_theme_colours() {
        assert_eq!(token_bg("dark"), DARK_BACKGROUND);
        assert_eq!(token_bg("light"), LIGHT_BACKGROUND);
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main = &conf["app"]["windows"][0];
        assert_eq!(main["label"], "main");
        assert_eq!(main["backgroundColor"], DARK_BACKGROUND);
    }

    #[test]
    fn each_theme_has_its_colour() {
        assert_eq!(background(tauri::Theme::Dark), Color(0x16, 0x16, 0x18, 255));
        assert_eq!(
            background(tauri::Theme::Light),
            Color(0xf4, 0xf3, 0xf0, 255)
        );
    }

    #[test]
    fn system_leaves_the_window_theme_to_the_system() {
        assert_eq!(native(Theme::System), None);
        assert_eq!(native(Theme::Dark), Some(tauri::Theme::Dark));
        assert_eq!(native(Theme::Light), Some(tauri::Theme::Light));
    }
}
