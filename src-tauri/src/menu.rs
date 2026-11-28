//! The menu bar (docs/08-desktop-and-release.md, "Menus").
//!
//! The items are listed once in [`ENTRIES`]. Each item that has a key takes
//! its accelerator from the key table (`ui/src/keys/keymap.json`, through
//! [`crate::keymap`]); the shell defines no key itself. Items that act in the
//! page send `app_menu`; the others act here.

use owlbum_core::app::{AppMenu, MenuItem as PageItem};
use owlbum_core::appdata::AppData;
use owlbum_core::text::menu as text;
use serde_json::json;
use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuBuilder, MenuItem, MenuItemBuilder, Submenu,
    SubmenuBuilder,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::keymap;

/// One of the three menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    File,
    View,
    Help,
}

/// What a line of a menu is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An item. Its id is the key table's id when the item has a key.
    Item,
    /// A checkbox item.
    Check,
    /// A separator.
    Separator,
}

/// One line of a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub bar: Bar,
    pub kind: Kind,
    pub id: &'static str,
    pub label: &'static str,
    /// Only in development builds.
    pub dev: bool,
}

const fn item(bar: Bar, id: &'static str, label: &'static str) -> Entry {
    Entry {
        bar,
        kind: Kind::Item,
        id,
        label,
        dev: false,
    }
}

const fn separator(bar: Bar) -> Entry {
    Entry {
        bar,
        kind: Kind::Separator,
        id: "",
        label: "",
        dev: false,
    }
}

/// The ids of items that have no key.
pub const RESCAN: &str = "file.rescan";
pub const DATA_FOLDER: &str = "file.data_folder";
pub const CHECK_AUTOMATICALLY: &str = "help.check_automatically";
pub const WHATS_NEW: &str = "help.whats_new";
pub const UPDATE: &str = "help.update";

/// Every line of the menu bar, in order.
pub const ENTRIES: &[Entry] = &[
    item(Bar::File, "app.open_folder", text::OPEN_FOLDER),
    item(Bar::File, "app.close_folder", text::CLOSE_FOLDER),
    item(Bar::File, RESCAN, text::RESCAN),
    separator(Bar::File),
    item(Bar::File, DATA_FOLDER, text::OPEN_DATA_FOLDER),
    separator(Bar::File),
    item(Bar::File, "app.quit", text::QUIT),
    item(Bar::View, "app.full_screen", text::FULL_SCREEN),
    Entry {
        dev: true,
        ..item(Bar::View, "app.reload", text::RELOAD)
    },
    Entry {
        dev: true,
        ..item(Bar::View, "app.developer_tools", text::DEVELOPER_TOOLS)
    },
    item(Bar::Help, "app.shortcuts", text::SHORTCUTS),
    separator(Bar::Help),
    item(Bar::Help, UPDATE, text::CHECK_FOR_UPDATES),
    Entry {
        kind: Kind::Check,
        ..item(Bar::Help, CHECK_AUTOMATICALLY, text::CHECK_AUTOMATICALLY)
    },
    item(Bar::Help, WHATS_NEW, text::WHATS_NEW),
    separator(Bar::Help),
    item(Bar::Help, "app.feedback", text::FEEDBACK),
    separator(Bar::Help),
    item(Bar::Help, "app.about", text::ABOUT),
];

/// The entries of a build: development-only items only when `dev`.
pub fn entries(dev: bool) -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter().filter(move |e| dev || !e.dev)
}

/// The label an item shows. Keyboard shortcuts shows `?` beside it, which
/// is not an accelerator because the page handles it (it must not fire
/// while typing). Windows draws the text after a tab as the key column.
pub fn label(entry: &Entry) -> String {
    let hint = keymap::row(entry.id)
        .filter(|r| !r.menu)
        .map(keymap::KeyRow::hint);
    match hint {
        Some(hint) if cfg!(windows) => format!("{}\t{hint}", entry.label),
        _ => entry.label.to_owned(),
    }
}

/// The items whose state follows the program, kept to update them.
pub struct MenuState<R: Runtime> {
    close_folder: MenuItem<R>,
    rescan: MenuItem<R>,
    auto: CheckMenuItem<R>,
    update: MenuItem<R>,
}

/// The items [`MenuState`] keeps, found while building.
struct Found<R: Runtime> {
    close_folder: Option<MenuItem<R>>,
    rescan: Option<MenuItem<R>>,
    auto: Option<CheckMenuItem<R>>,
    update: Option<MenuItem<R>>,
}

/// Builds the menu bar. Close folder and Rescan start disabled: no album is
/// open yet.
pub fn build<R: Runtime>(app: &AppHandle<R>, dev: bool, auto: bool) -> tauri::Result<Menu<R>> {
    let mut found = Found {
        close_folder: None,
        rescan: None,
        auto: None,
        update: None,
    };
    let file = submenu(app, Bar::File, text::FILE, dev, auto, &mut found)?;
    let view = submenu(app, Bar::View, text::VIEW, dev, auto, &mut found)?;
    let help = submenu(app, Bar::Help, text::HELP, dev, auto, &mut found)?;
    let menu = MenuBuilder::new(app)
        .item(&file)
        .item(&view)
        .item(&help)
        .build()?;
    if let Found {
        close_folder: Some(close_folder),
        rescan: Some(rescan),
        auto: Some(auto),
        update: Some(update),
    } = found
    {
        app.manage(MenuState {
            close_folder,
            rescan,
            auto,
            update,
        });
    }
    Ok(menu)
}

fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    bar: Bar,
    title: &str,
    dev: bool,
    auto: bool,
    found: &mut Found<R>,
) -> tauri::Result<Submenu<R>> {
    let mut sub = SubmenuBuilder::new(app, title);
    for entry in entries(dev).filter(|e| e.bar == bar) {
        sub = match entry.kind {
            Kind::Separator => sub.separator(),
            Kind::Check => {
                let built = CheckMenuItemBuilder::with_id(entry.id, label(entry))
                    .checked(auto)
                    .build(app)?;
                let sub = sub.item(&built);
                found.auto = Some(built);
                sub
            }
            Kind::Item => {
                let mut b = MenuItemBuilder::with_id(entry.id, label(entry));
                if let Some(acc) = keymap::accelerator(entry.id) {
                    b = b.accelerator(acc);
                }
                let needs_album = entry.id == "app.close_folder" || entry.id == RESCAN;
                let built = b.enabled(!needs_album).build(app)?;
                let sub = sub.item(&built);
                match entry.id {
                    "app.close_folder" => found.close_folder = Some(built),
                    RESCAN => found.rescan = Some(built),
                    UPDATE => found.update = Some(built),
                    _ => {}
                }
                sub
            }
        };
    }
    sub.build()
}

/// An album opened or closed: Close folder and Rescan follow.
pub fn album_open<R: Runtime>(app: &AppHandle<R>, open: bool) {
    if let Some(state) = app.try_state::<MenuState<R>>() {
        let _ = state.close_folder.set_enabled(open);
        let _ = state.rescan.set_enabled(open);
    }
}

/// `updates.auto` changed (here or in Settings): the checkbox follows.
pub fn updates_auto<R: Runtime>(app: &AppHandle<R>, auto: bool) {
    if let Some(state) = app.try_state::<MenuState<R>>() {
        let _ = state.auto.set_checked(auto);
    }
}

/// The update state changed: Help's update item follows
/// (docs/08-desktop-and-release.md, "States").
pub fn update_item<R: Runtime>(app: &AppHandle<R>, state: &owlbum_core::app::update::UpdateState) {
    if let Some(menu) = app.try_state::<MenuState<R>>() {
        let (label, enabled) = owlbum_core::app::update::menu_label(state);
        let _ = menu.update.set_text(label);
        let _ = menu.update.set_enabled(enabled);
    }
}

/// What choosing an item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The native folder dialog; a chosen folder is handed to the page.
    PickFolder,
    /// Sent to the page as `app_menu`.
    Page(PageItem),
    OpenDataFolder,
    /// The project's issues page in the user's browser.
    OpenFeedback,
    /// Through the close handshake (`closing::requested`).
    Quit,
    FullScreen,
    Reload,
    DeveloperTools,
    ToggleUpdatesAuto,
    /// Help's update item: check, download or install, by the state.
    Update,
}

/// The action of the item with this id.
pub fn action(id: &str) -> Option<Action> {
    Some(match id {
        "app.open_folder" => Action::PickFolder,
        "app.close_folder" => Action::Page(PageItem::CloseFolder),
        RESCAN => Action::Page(PageItem::Rescan),
        DATA_FOLDER => Action::OpenDataFolder,
        "app.quit" => Action::Quit,
        "app.full_screen" => Action::FullScreen,
        "app.reload" => Action::Reload,
        "app.developer_tools" => Action::DeveloperTools,
        "app.shortcuts" => Action::Page(PageItem::Shortcuts),
        CHECK_AUTOMATICALLY => Action::ToggleUpdatesAuto,
        WHATS_NEW => Action::Page(PageItem::WhatsNew),
        UPDATE => Action::Update,
        "app.feedback" => Action::OpenFeedback,
        "app.about" => Action::Page(PageItem::About),
        _ => return None,
    })
}

/// A menu item was chosen.
pub fn chosen<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(action) = action(id) else {
        return;
    };
    let window = app.get_webview_window("main");
    match action {
        Action::PickFolder => {
            let Some(picker) = app.try_state::<crate::commands::app::FolderPicker>() else {
                return;
            };
            let picker = std::sync::Arc::clone(&picker.0);
            let app = app.clone();
            // The dialog blocks; menu events arrive on the main thread.
            let _ = std::thread::Builder::new()
                .name("owlbum-pick".into())
                .spawn(move || {
                    if let Some(folder) = picker() {
                        crate::handoff::deliver(&app, &folder);
                    }
                });
        }
        Action::Page(item) => {
            let _ = crate::events::emit(app, &AppMenu { item });
        }
        Action::OpenDataFolder => {
            if let Some(data) = app.try_state::<AppData>()
                && let Err(e) = tauri_plugin_opener::open_path(data.dir(), None::<&str>)
            {
                tracing::warn!(target: "app", "the data folder could not be opened: {e}");
            }
        }
        Action::OpenFeedback => {
            let _ = crate::commands::app::open_fixed(app, owlbum_core::app::FEEDBACK_URL);
        }
        Action::Quit => {
            if !crate::closing::requested(app) {
                app.exit(0);
            }
        }
        Action::FullScreen => {
            if let Some(w) = window {
                let full = w.is_fullscreen().unwrap_or(false);
                let _ = w.set_fullscreen(!full);
            }
        }
        Action::Reload => {
            if let Some(w) = window {
                let _ = w.eval("location.reload()");
            }
        }
        Action::DeveloperTools =>
        {
            #[cfg(debug_assertions)]
            if let Some(w) = window {
                if w.is_devtools_open() {
                    w.close_devtools();
                } else {
                    w.open_devtools();
                }
            }
        }
        Action::Update => crate::update::menu_chosen(app),
        Action::ToggleUpdatesAuto => {
            if let Some(data) = app.try_state::<AppData>() {
                let auto = data.settings().map(|s| s.updates_auto).unwrap_or(true);
                let patch = json!({ "updates.auto": !auto });
                let now = data
                    .set_settings(patch.as_object().expect("an object"))
                    .map_or(auto, |s| s.updates_auto);
                updates_auto(app, now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_menu_row_of_the_key_table_has_a_menu_item() {
        let ids: HashSet<_> = ENTRIES.iter().map(|e| e.id).collect();
        for r in keymap::menu_rows(true) {
            assert!(ids.contains(r.id.as_str()), "{} has no menu item", r.id);
            let entry = ENTRIES.iter().find(|e| e.id == r.id).unwrap();
            assert_eq!(entry.dev, r.dev, "{}: dev differs from the key table", r.id);
        }
    }

    #[test]
    fn every_accelerator_comes_from_a_menu_row() {
        for e in ENTRIES {
            if let Some(r) = keymap::row(e.id) {
                // A row that is not `menu` (Keyboard shortcuts) is only shown.
                assert_eq!(keymap::accelerator(e.id).is_some(), r.menu, "{}", e.id);
            } else {
                assert!(
                    !e.id.starts_with("app."),
                    "{}: an app. id must be a row of the key table",
                    e.id
                );
            }
        }
        assert_eq!(keymap::accelerator("app.shortcuts"), None);
    }

    #[test]
    fn development_items_are_left_out_of_a_release_build() {
        assert!(entries(false).all(|e| e.id != "app.reload" && e.id != "app.developer_tools"));
        assert_eq!(entries(true).count(), ENTRIES.len());
    }

    #[test]
    fn every_item_has_an_action() {
        for e in ENTRIES.iter().filter(|e| e.kind != Kind::Separator) {
            assert!(action(e.id).is_some(), "{} does nothing", e.id);
        }
        assert_eq!(action("app.quit"), Some(Action::Quit));
        assert_eq!(
            action("help.whats_new"),
            Some(Action::Page(PageItem::WhatsNew))
        );
        assert_eq!(action("app.feedback"), Some(Action::OpenFeedback));
        assert_eq!(action("app.about"), Some(Action::Page(PageItem::About)));
        assert_eq!(action("nothing"), None);
    }

    #[test]
    fn keyboard_shortcuts_shows_its_key_on_windows() {
        let entry = ENTRIES.iter().find(|e| e.id == "app.shortcuts").unwrap();
        if cfg!(windows) {
            assert_eq!(label(entry), "&Keyboard shortcuts\t?");
        }
        let quit = ENTRIES.iter().find(|e| e.id == "app.quit").unwrap();
        assert_eq!(label(quit), "&Quit");
    }
}
