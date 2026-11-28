//! Settings: one row per key in `app.sqlite`'s `settings` table, each value
//! JSON (docs/02-data.md, "`app.sqlite`").
//!
//! Every key has a default in code. Reading fills in the default for a key
//! with no row, and also for a stored value that this version cannot use (so
//! a bad row never stops the program). A key this version does not know is
//! ignored when reading and kept in the table.
//!
//! Writing takes a patch of keys to values. The whole patch is checked
//! first; one value outside its setting's range answers
//! `setting_out_of_range` and nothing is written.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::appdata::db::sqlite_error;
use crate::error::AppResult;
use crate::{AppError, ErrorCode, text};

/// The view used for an album with no saved session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SettingsView {
    /// The grid.
    #[default]
    Grid,
    /// The timeline.
    Timeline,
    /// The map.
    Map,
}

/// The timeline's unit, once the user has picked one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SettingsTimelineUnit {
    /// One group per day.
    Day,
    /// One group per month.
    Month,
}

/// Where an edit ends up (docs/06-editing-and-file-actions.md, "Saving").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EditTarget {
    /// Kept in the album database; the file is never touched.
    #[default]
    Album,
    /// Save writes it into the photo file.
    File,
}

/// Where the thumbnails sit (`panel.grid_place`, docs/04-browsing.md,
/// "Hiding and moving the panels").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GridPlace {
    /// A column at the left of the window.
    #[default]
    Left,
    /// A strip along the bottom, under the preview, that scrolls sideways.
    Bottom,
}

/// The colour theme (docs/09-visual-design.md, "Colour tokens").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the system's light or dark setting.
    #[default]
    System,
    /// Always dark.
    Dark,
    /// Always light.
    Light,
}

/// The `window` setting: the window's bounds when it is not maximised, in
/// physical pixels, and whether it was maximised
/// (docs/08-desktop-and-release.md, "The window"). A missing number means
/// "the default size, centred".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WindowSetting {
    /// Left edge.
    pub x: Option<i32>,
    /// Top edge.
    pub y: Option<i32>,
    /// Width.
    pub width: Option<u32>,
    /// Height.
    pub height: Option<u32>,
    /// Whether the window was maximised.
    #[serde(default)]
    pub maximised: bool,
}

impl Default for WindowSetting {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: Some(1440),
            height: Some(900),
            maximised: false,
        }
    }
}

/// Every setting, with defaults filled in. The JSON keys are the setting
/// keys of docs/02-data.md, dots included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Settings {
    /// Tile size in the grid, 96 to 320.
    pub thumb_size: u32,
    /// The thumbnail column's width.
    #[serde(rename = "panel.grid_width")]
    pub panel_grid_width: u32,
    /// The details panel's width.
    #[serde(rename = "panel.details_width")]
    pub panel_details_width: u32,
    /// The thumbnails are shown (false: hidden, the preview takes the room).
    #[serde(rename = "panel.grid_shown")]
    pub panel_grid_shown: bool,
    /// The details panel is shown.
    #[serde(rename = "panel.details_shown")]
    pub panel_details_shown: bool,
    /// Where the thumbnails sit: left or bottom.
    #[serde(rename = "panel.grid_place")]
    pub panel_grid_place: GridPlace,
    /// The bottom strip's height.
    #[serde(rename = "panel.strip_height")]
    pub panel_strip_height: u32,
    /// Which details sections are folded: section id to open (true) or
    /// folded (false). A missing id counts as open.
    pub sections: BTreeMap<String, bool>,
    /// The view for an album with no saved session.
    pub view: SettingsView,
    /// The timeline's unit, or null until the user picks one.
    pub timeline_unit: Option<SettingsTimelineUnit>,
    /// Stack similar photos.
    pub stack_similar: bool,
    /// The answer to "Load maps from the internet?", or null before it was
    /// asked.
    pub maps_online: Option<bool>,
    /// Where an edit ends up.
    pub edit_target: EditTarget,
    /// Save all asks before it overwrites the photo files (false once the
    /// user ticked "Don't ask again, always overwrite").
    #[serde(rename = "save_all.ask")]
    pub save_all_ask: bool,
    /// Reopen the last album at start.
    pub reopen_last: bool,
    /// Check for updates once a day.
    #[serde(rename = "updates.auto")]
    pub updates_auto: bool,
    /// The last update check, in milliseconds since the epoch.
    #[serde(rename = "updates.last_check")]
    pub updates_last_check: Option<i64>,
    /// The first-start note about the update check has been shown.
    #[serde(rename = "updates.notice_seen")]
    pub updates_notice_seen: bool,
    /// The chosen describe model's id, or null for Off.
    #[serde(rename = "tagging.describe")]
    pub tagging_describe: Option<String>,
    /// The chosen keywords model's id, or null for Off.
    #[serde(rename = "tagging.keywords")]
    pub tagging_keywords: Option<String>,
    /// The thumbnail cache's size limit in whole gigabytes, 1 to 1000.
    #[serde(rename = "cache.limit_gb")]
    pub cache_limit_gb: u32,
    /// The window's bounds and maximised state.
    pub window: WindowSetting,
    /// The last version whose What's new notes were shown.
    #[serde(rename = "whats_new.last_seen")]
    pub whats_new_last_seen: Option<String>,
    /// The colour theme: system, dark or light.
    pub theme: Theme,
    /// The interface language as a BCP 47 tag (`fr`, `pt-BR`), or null to
    /// follow the system's language when a translation exists
    /// (docs/09-visual-design.md, "Languages").
    pub language: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            thumb_size: 160,
            panel_grid_width: 420,
            panel_details_width: 340,
            panel_grid_shown: true,
            panel_details_shown: true,
            panel_grid_place: GridPlace::Left,
            panel_strip_height: 200,
            sections: BTreeMap::new(),
            view: SettingsView::Grid,
            timeline_unit: None,
            stack_similar: true,
            maps_online: None,
            edit_target: EditTarget::Album,
            save_all_ask: true,
            reopen_last: true,
            updates_auto: true,
            updates_last_check: None,
            updates_notice_seen: false,
            tagging_describe: None,
            tagging_keywords: None,
            cache_limit_gb: 20,
            window: WindowSetting::default(),
            whats_new_last_seen: None,
            theme: Theme::System,
            language: None,
        }
    }
}

/// A check that a JSON value is one the setting can hold.
type Check = fn(&Value) -> bool;

/// Every known key and its check, in the order of the table in
/// docs/02-data.md.
pub const KEYS: &[(&str, Check)] = &[
    ("thumb_size", |v| int_in(v, 96, 320)),
    ("panel.grid_width", |v| int_in(v, 1, MAX_PIXELS)),
    ("panel.details_width", |v| int_in(v, 1, MAX_PIXELS)),
    ("panel.grid_shown", Value::is_boolean),
    ("panel.details_shown", Value::is_boolean),
    ("panel.grid_place", |v| one_of(v, &["left", "bottom"])),
    ("panel.strip_height", |v| int_in(v, 1, MAX_PIXELS)),
    ("sections", |v| {
        v.as_object()
            .is_some_and(|m| m.values().all(Value::is_boolean))
    }),
    ("view", |v| one_of(v, &["grid", "timeline", "map"])),
    ("timeline_unit", |v| {
        v.is_null() || one_of(v, &["day", "month"])
    }),
    ("stack_similar", Value::is_boolean),
    ("maps_online", |v| v.is_null() || v.is_boolean()),
    ("edit_target", |v| one_of(v, &["album", "file"])),
    ("save_all.ask", Value::is_boolean),
    ("reopen_last", Value::is_boolean),
    ("updates.auto", Value::is_boolean),
    ("updates.last_check", |v| {
        v.is_null() || int_in(v, 0, i64::MAX)
    }),
    ("updates.notice_seen", Value::is_boolean),
    ("tagging.describe", model_id),
    ("tagging.keywords", model_id),
    ("cache.limit_gb", |v| int_in(v, 1, 1000)),
    ("window", window),
    ("whats_new.last_seen", |v| v.is_null() || v.is_string()),
    ("theme", |v| one_of(v, &["system", "dark", "light"])),
    ("language", |v| {
        v.is_null() || v.as_str().is_some_and(language_tag)
    }),
];

/// A BCP 47 language tag of the simple kind a locale file is named by:
/// a two- or three-letter language, then subtags of letters and digits
/// (`fr`, `pt-BR`, `qps-ploc`).
fn language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let first = parts.next().unwrap_or_default();
    (2..=3).contains(&first.len())
        && first.bytes().all(|b| b.is_ascii_alphabetic())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// The largest width in pixels a panel setting may hold.
const MAX_PIXELS: i64 = 100_000;

fn int_in(v: &Value, lo: i64, hi: i64) -> bool {
    v.as_i64().is_some_and(|n| (lo..=hi).contains(&n))
}

fn one_of(v: &Value, choices: &[&str]) -> bool {
    v.as_str().is_some_and(|s| choices.contains(&s))
}

fn model_id(v: &Value) -> bool {
    v.is_null() || v.as_str().is_some_and(|s| !s.trim().is_empty())
}

fn window(v: &Value) -> bool {
    let Some(m) = v.as_object() else {
        return false;
    };
    let number =
        |key: &str, lo: i64, hi: i64| m.get(key).is_none_or(|n| n.is_null() || int_in(n, lo, hi));
    number("x", i64::from(i32::MIN), i64::from(i32::MAX))
        && number("y", i64::from(i32::MIN), i64::from(i32::MAX))
        && number("width", 1, i64::from(u32::MAX))
        && number("height", 1, i64::from(u32::MAX))
        && m.get("maximised").is_none_or(Value::is_boolean)
}

/// The check for `key`, or `None` for a key this version does not know.
fn check_of(key: &str) -> Option<Check> {
    KEYS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
}

/// Reads every setting, filling in defaults.
pub fn get(conn: &Connection) -> AppResult<Settings> {
    let read = sqlite_error(text::appdata::READING_SETTINGS);
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings")
        .map_err(&read)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(&read)?;
    let Value::Object(mut merged) = serde_json::to_value(Settings::default()).unwrap_or_default()
    else {
        return Ok(Settings::default());
    };
    for row in rows {
        let (key, text) = row.map_err(&read)?;
        let Some(check) = check_of(&key) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&text)
            && check(&value)
        {
            merged.insert(key, value);
        }
    }
    Ok(serde_json::from_value(Value::Object(merged)).unwrap_or_default())
}

/// Writes `patch` (keys to values) and answers every setting.
///
/// # Errors
///
/// `setting_out_of_range` when a known key's value is outside its range or
/// of the wrong kind; `details.key` names it. Nothing is written then.
pub fn set(conn: &Connection, patch: &Map<String, Value>) -> AppResult<Settings> {
    for (key, value) in patch {
        if let Some(check) = check_of(key)
            && !check(value)
        {
            return Err(AppError::new(
                ErrorCode::SettingOutOfRange,
                text::appdata::setting_out_of_range(key),
            )
            .with_details(serde_json::json!({ "key": key })));
        }
    }
    let write = sqlite_error(text::appdata::SAVING_SETTINGS);
    let tx = conn.unchecked_transaction().map_err(&write)?;
    for (key, value) in patch {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            (key, value.to_string()),
        )
        .map_err(&write)?;
    }
    tx.commit().map_err(&write)?;
    get(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(crate::appdata::db::MIGRATIONS[0].sql)
            .unwrap();
        c
    }

    fn patch(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn rows(c: &Connection) -> Vec<(String, String)> {
        let mut s = c
            .prepare("SELECT key, value FROM settings ORDER BY key")
            .unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn defaults_match_the_table_in_the_data_document() {
        assert_eq!(
            serde_json::to_value(Settings::default()).unwrap(),
            json!({
                "thumb_size": 160,
                "panel.grid_width": 420,
                "panel.details_width": 340,
                "panel.grid_shown": true,
                "panel.details_shown": true,
                "panel.grid_place": "left",
                "panel.strip_height": 200,
                "sections": {},
                "view": "grid",
                "timeline_unit": null,
                "stack_similar": true,
                "maps_online": null,
                "edit_target": "album",
                "save_all.ask": true,
                "reopen_last": true,
                "updates.auto": true,
                "updates.last_check": null,
                "updates.notice_seen": false,
                "tagging.describe": null,
                "tagging.keywords": null,
                "cache.limit_gb": 20,
                "window": { "x": null, "y": null, "width": 1440, "height": 900, "maximised": false },
                "whats_new.last_seen": null,
                "theme": "system",
                "language": null,
            })
        );
    }

    #[test]
    fn every_setting_has_a_check_and_every_check_a_setting() {
        let Value::Object(defaults) = serde_json::to_value(Settings::default()).unwrap() else {
            panic!()
        };
        let mut keys: Vec<&str> = defaults.keys().map(String::as_str).collect();
        let mut checked: Vec<&str> = KEYS.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        checked.sort_unstable();
        assert_eq!(keys, checked);
        for (key, value) in &defaults {
            assert!(check_of(key).unwrap()(value), "the default of {key} passes");
        }
    }

    #[test]
    fn an_empty_table_reads_as_the_defaults() {
        assert_eq!(get(&conn()).unwrap(), Settings::default());
    }

    #[test]
    fn a_patch_is_saved_and_read_back() {
        let c = conn();
        let s = set(
            &c,
            &patch(json!({
                "thumb_size": 240,
                "panel.details_width": 300,
                "panel.grid_shown": false,
                "panel.grid_place": "bottom",
                "panel.strip_height": 260,
                "sections": { "histogram": false },
                "view": "map",
                "timeline_unit": "month",
                "maps_online": false,
                "edit_target": "file",
                "save_all.ask": false,
                "tagging.describe": "florence-2-base",
                "cache.limit_gb": 1000,
                "window": { "x": -1920, "y": 10, "width": 1200, "height": 800, "maximised": true },
                "updates.last_check": 1_795_683_903_007_i64,
                "whats_new.last_seen": "0.1.0",
                "theme": "light",
                "language": "pt-BR",
            })),
        )
        .unwrap();
        assert_eq!(s.thumb_size, 240);
        assert_eq!(s.panel_details_width, 300);
        assert_eq!(s.panel_grid_width, 420, "untouched keys keep their default");
        assert!(!s.panel_grid_shown);
        assert!(s.panel_details_shown, "untouched keys keep their default");
        assert_eq!(s.panel_grid_place, GridPlace::Bottom);
        assert_eq!(s.panel_strip_height, 260);
        assert_eq!(s.sections.get("histogram"), Some(&false));
        assert_eq!(s.view, SettingsView::Map);
        assert_eq!(s.timeline_unit, Some(SettingsTimelineUnit::Month));
        assert_eq!(s.maps_online, Some(false));
        assert_eq!(s.edit_target, EditTarget::File);
        assert!(!s.save_all_ask);
        assert_eq!(s.tagging_describe.as_deref(), Some("florence-2-base"));
        assert_eq!(s.cache_limit_gb, 1000);
        assert_eq!(
            s.window,
            WindowSetting {
                x: Some(-1920),
                y: Some(10),
                width: Some(1200),
                height: Some(800),
                maximised: true
            }
        );
        assert_eq!(s.updates_last_check, Some(1_795_683_903_007));
        assert_eq!(s.whats_new_last_seen.as_deref(), Some("0.1.0"));
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.language.as_deref(), Some("pt-BR"));
        assert_eq!(get(&c).unwrap(), s);
    }

    #[test]
    fn a_later_patch_changes_only_its_own_keys() {
        let c = conn();
        set(&c, &patch(json!({ "thumb_size": 200, "view": "timeline" }))).unwrap();
        let s = set(&c, &patch(json!({ "thumb_size": 100 }))).unwrap();
        assert_eq!(s.thumb_size, 100);
        assert_eq!(s.view, SettingsView::Timeline);
    }

    #[test]
    fn nullable_settings_can_be_set_back_to_null() {
        let c = conn();
        set(
            &c,
            &patch(json!({ "tagging.keywords": "clip", "timeline_unit": "day" })),
        )
        .unwrap();
        let s = set(
            &c,
            &patch(json!({ "tagging.keywords": null, "timeline_unit": null })),
        )
        .unwrap();
        assert_eq!(s.tagging_keywords, None);
        assert_eq!(s.timeline_unit, None);
    }

    #[test]
    fn values_outside_their_range_are_refused() {
        for (key, bad) in [
            ("thumb_size", json!(95)),
            ("thumb_size", json!(321)),
            ("thumb_size", json!(160.5)),
            ("thumb_size", json!("160")),
            ("thumb_size", json!(null)),
            ("panel.grid_width", json!(0)),
            ("panel.details_width", json!(-5)),
            ("panel.grid_shown", json!("no")),
            ("panel.details_shown", json!(null)),
            ("panel.grid_place", json!("right")),
            ("panel.strip_height", json!(0)),
            ("sections", json!({ "histogram": "no" })),
            ("sections", json!(["histogram"])),
            ("view", json!("list")),
            ("timeline_unit", json!("auto")),
            ("stack_similar", json!(1)),
            ("maps_online", json!("yes")),
            ("edit_target", json!("disk")),
            ("reopen_last", json!(null)),
            ("updates.auto", json!("true")),
            ("updates.last_check", json!(-1)),
            ("updates.notice_seen", json!(0)),
            ("tagging.describe", json!("")),
            ("tagging.keywords", json!(7)),
            ("theme", json!("blue")),
            ("theme", json!(null)),
            ("language", json!("")),
            ("language", json!("english")),
            ("language", json!("fr_FR")),
            ("cache.limit_gb", json!(0)),
            ("cache.limit_gb", json!(1001)),
            ("window", json!(null)),
            ("window", json!({ "width": 0 })),
            ("window", json!({ "x": 1.5 })),
            ("window", json!({ "maximised": "yes" })),
            ("whats_new.last_seen", json!(1)),
        ] {
            let c = conn();
            let e = set(&c, &patch(json!({ key: bad.clone() }))).unwrap_err();
            assert_eq!(e.code, ErrorCode::SettingOutOfRange, "{key} = {bad}");
            assert_eq!(e.details, Some(json!({ "key": key })), "{key}");
            assert!(e.message.contains(key), "{}", e.message);
            assert!(rows(&c).is_empty(), "{key} = {bad} wrote nothing");
        }
    }

    #[test]
    fn the_range_ends_are_allowed() {
        let c = conn();
        for (key, ok) in [
            ("thumb_size", json!(96)),
            ("thumb_size", json!(320)),
            ("cache.limit_gb", json!(1)),
            ("cache.limit_gb", json!(1000)),
            ("window", json!({})),
            ("window", json!({ "width": 800, "height": 600 })),
        ] {
            set(&c, &patch(json!({ key: ok }))).unwrap_or_else(|e| panic!("{key}: {e}"));
        }
    }

    #[test]
    fn one_bad_value_writes_nothing_from_the_patch() {
        let c = conn();
        set(&c, &patch(json!({ "thumb_size": 200 }))).unwrap();
        let e = set(
            &c,
            &patch(json!({ "thumb_size": 300, "view": "map", "cache.limit_gb": 0 })),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::SettingOutOfRange);
        assert_eq!(e.details, Some(json!({ "key": "cache.limit_gb" })));
        let s = get(&c).unwrap();
        assert_eq!(s.thumb_size, 200);
        assert_eq!(s.view, SettingsView::Grid);
    }

    #[test]
    fn unknown_keys_are_kept_but_not_answered() {
        let c = conn();
        let s = set(
            &c,
            &patch(json!({ "future.thing": { "a": [1, 2] }, "thumb_size": 128 })),
        )
        .unwrap();
        assert_eq!(s.thumb_size, 128);
        assert!(
            !serde_json::to_value(&s)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("future.thing")
        );
        assert!(rows(&c).contains(&("future.thing".to_owned(), r#"{"a":[1,2]}"#.to_owned())));
    }

    #[test]
    fn a_stored_value_this_version_cannot_use_reads_as_the_default() {
        let c = conn();
        c.execute_batch(
            r#"INSERT INTO settings VALUES ('thumb_size', '5000');
               INSERT INTO settings VALUES ('view', '"list"');
               INSERT INTO settings VALUES ('cache.limit_gb', 'not json');
               INSERT INTO settings VALUES ('reopen_last', 'false');"#,
        )
        .unwrap();
        let s = get(&c).unwrap();
        assert_eq!(s.thumb_size, 160);
        assert_eq!(s.view, SettingsView::Grid);
        assert_eq!(s.cache_limit_gb, 20);
        assert!(!s.reopen_last, "the good row is still used");
        assert_eq!(rows(&c).len(), 4, "bad rows are not deleted");
    }

    #[test]
    fn values_are_stored_as_json() {
        let c = conn();
        set(
            &c,
            &patch(json!({ "view": "map", "thumb_size": 200, "maps_online": null })),
        )
        .unwrap();
        assert_eq!(
            rows(&c),
            [
                ("maps_online".to_owned(), "null".to_owned()),
                ("thumb_size".to_owned(), "200".to_owned()),
                ("view".to_owned(), r#""map""#.to_owned()),
            ]
        );
    }
}
