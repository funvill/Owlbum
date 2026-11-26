//! The key table, `ui/src/keys/keymap.json`, compiled into the shell
//! (docs/08-desktop-and-release.md, "The key table").
//!
//! The page and the shell read the same file. The shell takes its menu
//! accelerators from the rows marked `menu`, and the key shown beside Help >
//! Keyboard shortcuts from the `app.shortcuts` row. It defines no key itself.

use std::sync::OnceLock;

use serde::Deserialize;

/// The table as the page has it.
pub const KEYMAP_JSON: &str = include_str!("../../ui/src/keys/keymap.json");

/// Where a row applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    Everywhere,
    Browse,
    Viewer,
    Edit,
}

/// One row of the table. A flag the file leaves out is false.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyRow {
    /// A stable name such as `app.quit`. Menu items use the same id.
    pub id: String,
    pub place: Place,
    /// One or more keys, such as `Ctrl+O` or `F11`. The first is the one
    /// shown beside a menu item.
    pub keys: Vec<String>,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub repeat: bool,
    /// A native menu accelerator.
    #[serde(default)]
    pub menu: bool,
    /// On the menu in development builds only.
    #[serde(default)]
    pub dev: bool,
    #[serde(default)]
    pub typing: bool,
}

impl KeyRow {
    /// The accelerator for Tauri's menu builder, such as `Ctrl+O`, when the
    /// row is a menu accelerator. The table's spelling is the one the
    /// builder parses.
    pub fn accelerator(&self) -> Option<&str> {
        if self.menu {
            self.keys.first().map(String::as_str)
        } else {
            None
        }
    }

    /// The key shown beside a menu item, such as `?` for Keyboard
    /// shortcuts, which the page handles rather than the menu.
    pub fn hint(&self) -> &str {
        self.keys.first().map_or("", String::as_str)
    }
}

/// Parses a key table. Used on the compiled-in file; tests also feed it
/// broken tables.
pub fn parse(json: &str) -> Result<Vec<KeyRow>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Every row, in file order.
///
/// # Panics
///
/// If the compiled-in file does not parse. The tests below and the page's
/// tests check the file, so a build that panics here cannot pass the gate.
pub fn rows() -> &'static [KeyRow] {
    static ROWS: OnceLock<Vec<KeyRow>> = OnceLock::new();
    ROWS.get_or_init(|| parse(KEYMAP_JSON).expect("ui/src/keys/keymap.json does not parse"))
}

/// The row with this id.
pub fn row(id: &str) -> Option<&'static KeyRow> {
    rows().iter().find(|r| r.id == id)
}

/// The accelerator of the menu item with this id, if it has one.
pub fn accelerator(id: &str) -> Option<&'static str> {
    row(id).and_then(KeyRow::accelerator)
}

/// The rows that are menu accelerators. Development-only rows are included
/// only when `dev` is true.
pub fn menu_rows(dev: bool) -> impl Iterator<Item = &'static KeyRow> {
    rows().iter().filter(move |r| r.menu && (dev || !r.dev))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::str::FromStr;

    #[test]
    fn the_compiled_in_table_parses() {
        assert!(rows().len() > 50);
        let ids: HashSet<_> = rows().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), rows().len(), "an id is used twice");
        assert!(rows().iter().all(|r| !r.keys.is_empty()));
    }

    #[test]
    fn flags_left_out_are_false() {
        let hide = row("browse.hide").unwrap();
        assert_eq!(hide.place, Place::Browse);
        assert_eq!(hide.keys, ["H"]);
        assert!(!hide.hold && !hide.repeat && !hide.menu && !hide.dev && !hide.typing);
        assert!(row("browse.before").unwrap().hold);
    }

    #[test]
    fn the_menu_accelerators_are_the_ones_the_menus_name() {
        // docs/08-desktop-and-release.md, "Menus".
        let menu: Vec<_> = menu_rows(true)
            .map(|r| (r.id.as_str(), r.accelerator().unwrap(), r.dev))
            .collect();
        assert_eq!(
            menu,
            [
                ("app.about", "F1", false),
                ("app.feedback", "F2", false),
                ("app.open_folder", "Ctrl+O", false),
                ("app.full_screen", "F11", false),
                ("app.quit", "Ctrl+Q", false),
                ("app.close_folder", "Ctrl+W", false),
                ("app.reload", "Ctrl+R", true),
                ("app.developer_tools", "Ctrl+Shift+I", true),
            ]
        );
        assert_eq!(menu_rows(false).count(), 6);
        assert!(rows().iter().all(|r| !r.dev || r.menu));
        assert!(rows().iter().filter(|r| r.menu).all(|r| r.keys.len() == 1));
        assert!(
            rows()
                .iter()
                .filter(|r| r.menu)
                .all(|r| r.place == Place::Everywhere)
        );
    }

    #[test]
    fn every_menu_accelerator_parses_as_the_menu_builder_parses_it() {
        // Tauri's menu builder parses the string with muda and silently drops
        // an accelerator that does not parse (`.parse().ok()`), so check here.
        assert!(muda::accelerator::Accelerator::from_str("Ctrl+Nonsense").is_err());
        for r in menu_rows(true) {
            let text = r.accelerator().unwrap();
            assert!(
                muda::accelerator::Accelerator::from_str(text).is_ok(),
                "{}: {text} is not an accelerator",
                r.id
            );
        }
    }

    #[test]
    fn keyboard_shortcuts_is_shown_as_question_mark_but_not_an_accelerator() {
        let r = row("app.shortcuts").unwrap();
        assert_eq!(r.hint(), "?");
        assert_eq!(r.accelerator(), None);
        assert_eq!(accelerator("app.shortcuts"), None);
        assert_eq!(accelerator("app.quit"), Some("Ctrl+Q"));
        assert_eq!(accelerator("no.such"), None);
    }

    #[test]
    fn a_broken_table_is_refused() {
        assert!(parse(r#"[{"id":"a.b","place":"grid","keys":["F"]}]"#).is_err());
        assert!(parse(r#"[{"id":"a.b","place":"browse","keys":["F"],"label":"x"}]"#).is_err());
        assert!(parse(r#"[{"id":"a.b","place":"browse"}]"#).is_err());
        assert!(parse(r#"{"rows":[]}"#).is_err());
    }
}
