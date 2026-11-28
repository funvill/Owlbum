import type { AppCommands } from "../bindings/app";
import type { FormatInfo } from "../bindings/FormatInfo";
import type { Settings } from "../bindings/Settings";
import type { AppError } from "../bindings/AppError";
import type { AreaHandlers } from "./types";

/** Every setting's default, as `owlbum_core::appdata::Settings::default()` has them. */
export const defaultSettings: Settings = {
  thumb_size: 160,
  "panel.grid_width": 420,
  "panel.details_width": 340,
  "panel.grid_shown": true,
  "panel.details_shown": true,
  "panel.grid_place": "left",
  "panel.strip_height": 200,
  sections: {},
  view: "grid",
  timeline_unit: null,
  stack_similar: true,
  maps_online: null,
  edit_target: "album",
  "save_all.ask": true,
  reopen_last: true,
  "updates.auto": true,
  "updates.last_check": null,
  "updates.notice_seen": false,
  "tagging.describe": null,
  "tagging.keywords": null,
  "cache.limit_gb": 20,
  window: { x: null, y: null, width: 1440, height: 900, maximised: false },
  "whats_new.last_seen": null,
};

/** The ranges the fake checks, like Rust's `setting_out_of_range`. */
const ranges: Partial<Record<keyof Settings, [number, number]>> = {
  thumb_size: [96, 320],
  "cache.limit_gb": [1, 1000],
};

/**
 * A few rows of the media format registry (`owlbum_core::media::FORMATS`),
 * enough for page tests: one editable still, one animated still, one RAW
 * and one video. The real list comes from Rust; the page never hard-codes it.
 */
export const fakeFormats: FormatInfo[] = [
  {
    id: "jpeg",
    extensions: [".jpg", ".jpeg", ".jpe", ".jfif"],
    kind: "image",
    label: "JPEG",
    edit_in_place: true,
    alpha: false,
    save_as: "jpeg",
    animated: false,
  },
  {
    id: "gif",
    extensions: [".gif"],
    kind: "image",
    label: "GIF",
    edit_in_place: false,
    alpha: true,
    save_as: "png",
    animated: true,
  },
  {
    id: "cr3",
    extensions: [".cr3"],
    kind: "raw",
    label: "Canon RAW",
    edit_in_place: false,
    alpha: false,
    save_as: "jpeg",
    animated: false,
  },
  {
    id: "mp4",
    extensions: [".mp4", ".m4v"],
    kind: "video",
    label: "MP4 video",
    edit_in_place: false,
    alpha: false,
    save_as: "jpeg",
    animated: false,
  },
];

/** Rough boxes standing in for a few country outlines. */
const fakeOutlines = [
  { name: "France", south: 42, north: 51, west: -5, east: 8 },
  { name: "Canada", south: 49, north: 83, west: -141, east: -53 },
  { name: "Australia", south: -44, north: -10, west: 113, east: 154 },
  { name: "Japan", south: 24, north: 46, west: 123, east: 146 },
];

/** The fake backend's `app_` commands. */
export const appHandlers: AreaHandlers<AppCommands> = {
  app_about: () => ({
    version: "0.0.0-fake",
    platform: "windows",
    arch: "x86_64",
    package: "dev",
    data_dir: "C:\\Users\\sam\\AppData\\Local\\OwlbumData",
    copyright: "Copyright (c) 2026 Owlbum contributors",
    repository: "https://github.com/funvill/Owlbum",
    has_notices: true,
    onnx_runtime: "1.31.1",
  }),
  // An empty cache at the default limit. A test that needs a size replaces these.
  app_cache_clear: () => ({ bytes_freed: 0 }),
  app_cache_usage: () => ({
    bytes: 0,
    limit_bytes: defaultSettings["cache.limit_gb"] * 1_000_000_000,
  }),
  // Windows: files are dragged out and copied as files.
  app_capabilities: () => ({ drag_out: true, copy_files: "files" }),
  // Windows, with Owlbum not yet opening pictures. Turning it on would open
  // Windows' Default apps page; here it only answers.
  app_associate_status: () => ({ available: true, on: false }),
  app_associate: ({ on }) => ({ available: true, on }),
  app_close_ready: () => null,
  // Four countries by a box around them, enough for page tests; the real
  // lookup is the bundled outlines in Rust. At sea there is none.
  app_country_at: ({ lat, lon }) =>
    fakeOutlines.find(
      (b) => lat >= b.south && lat <= b.north && lon >= b.west && lon <= b.east,
    )?.name ?? null,
  app_formats: () =>
    fakeFormats.map((f) => ({ ...f, extensions: [...f.extensions] })),
  // The dialog was cancelled. A test that picks a folder replaces this.
  app_pick_folder: () => null,
  // Opening the browser does nothing here; tests read the calls.
  app_open_feedback: () => null,
  app_open_data_folder: () => null,
  app_full_screen: () => null,
  app_developer_tools: () => null,
  app_update_choose: () => null,
  app_open_repository: () => null,
  app_open_licence: () => null,
  app_open_notices: () => null,
  app_open_log_folder: () => null,
  app_open_release_page: () => null,
  app_open_release_notes: () => null,
  app_copy_details: () => null,
  app_restart: () => null,
  app_log: () => null,
  // A development build: no updater (docs/08, "States").
  app_check_updates: () => null,
  app_update_state: () => ({ state: "unavailable", version: null, percent: null }),
  app_update_download: () => null,
  app_update_install: () => null,
  app_update_notice_answer: ({ keep_on }) => ({
    ...structuredClone(defaultSettings),
    "updates.notice_seen": true,
    "updates.auto": keep_on,
  }),
  app_whats_new: () => [],
  app_whats_new_seen: () => null,
  app_quit: () => null,
  app_recent_list: () => [],
  app_recent_remove: () => [],
  app_session_save: () => null,
  app_settings_get: () => structuredClone(defaultSettings),
  // Stateless: answers the defaults with the patch applied. A test that
  // needs settings to persist between calls replaces these handlers.
  app_settings_set: ({ patch }) => {
    for (const [key, value] of Object.entries(patch)) {
      const range = ranges[key as keyof Settings];
      if (
        range !== undefined &&
        (typeof value !== "number" || value < range[0] || value > range[1])
      ) {
        const refused: AppError = {
          code: "setting_out_of_range",
          message: `That value can't be used for the setting ${key}.`,
          details: { key },
        };
        throw refused;
      }
    }
    return { ...structuredClone(defaultSettings), ...patch } as Settings;
  },
  // A first start: nothing recent, nothing to open.
  app_startup: () => ({
    version: "0.0.0-fake",
    platform: "windows",
    arch: "x86_64",
    package: "dev",
    dev: true,
    settings: structuredClone(defaultSettings),
    recent: [],
    launch_folder: null,
    launch_photo: null,
    reopen: null,
    show_update_notice: false,
    whats_new: [],
  }),
  app_version: () => ({ version: "0.0.0-fake" }),
};
