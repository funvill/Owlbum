import type { AppCommands } from "../bindings/app";
import type { CountryMatch } from "../bindings/CountryMatch";
import type { FormatInfo } from "../bindings/FormatInfo";
import type { Settings } from "../bindings/Settings";
import type { AppError } from "../bindings/AppError";
import type { AreaHandlers } from "./types";

/** Every setting's default, as `owlbum_core::appdata::Settings::default()` has them. */
export const defaultSettings: Settings = {
  thumb_size: 160,
  "panel.grid_width": 420,
  "panel.details_width": 340,
  sections: {},
  view: "grid",
  timeline_unit: null,
  stack_similar: true,
  maps_online: null,
  edit_target: "album",
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

/**
 * A few countries with their central points, enough for page tests. The
 * real list is Natural Earth's, bundled in Rust (owlbum_core::info::country).
 */
export const fakeCountries: CountryMatch[] = [
  { name: "Algeria", lat: 28.1586, lon: 2.6174 },
  { name: "Australia", lat: -25.0498, lon: 134.0482 },
  { name: "Canada", lat: 59.0816, lon: -101.6684 },
  { name: "Finland", lat: 63.2524, lon: 27.2764 },
  { name: "France", lat: 46.6966, lon: 2.4351 },
  { name: "Germany", lat: 50.9616, lon: 10.2117 },
  { name: "Italy", lat: 44.7329, lon: 11.0761 },
  { name: "Japan", lat: 36.9324, lon: 138.4442 },
  { name: "Niger", lat: 17.4469, lon: 9.5047 },
  { name: "Nigeria", lat: 9.4396, lon: 7.4958 },
  { name: "United States of America", lat: 39.5385, lon: -97.4826 },
  { name: "Åland", lat: 60.1565, lon: 19.8697 },
];

/** Rough boxes standing in for a few country outlines. */
const fakeOutlines = [
  { name: "France", south: 42, north: 51, west: -5, east: 8 },
  { name: "Canada", south: 49, north: 83, west: -141, east: -53 },
  { name: "Australia", south: -44, north: -10, west: 113, east: 154 },
  { name: "Japan", south: 24, north: 46, west: 123, east: 146 },
];

/** Lower case without accents, as Rust's `info::country` compares names. */
const fold = (text: string) =>
  text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** `app_place_search`'s rule (docs/04-browsing.md, "How typed fields save"). */
export function placeSearch(
  countries: CountryMatch[],
  text: string,
): CountryMatch[] {
  const wanted = fold(text.trim());
  if (wanted === "") return [];
  const rank = (name: string) => {
    const folded = fold(name);
    if (folded.startsWith(wanted)) return 0;
    if (folded.split(/[^\p{L}\p{N}]+/u).some((word) => word.startsWith(wanted)))
      return 1;
    return folded.includes(wanted) ? 2 : -1;
  };
  return countries
    .map((c) => ({ c, r: rank(c.name) }))
    .filter(({ r }) => r >= 0)
    .sort((a, b) => a.r - b.r || compare(fold(a.c.name), fold(b.c.name)))
    .slice(0, 6)
    .map(({ c }) => ({ ...c }));
}

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
  }),
  // An empty cache at the default limit. A test that needs a size replaces these.
  app_cache_clear: () => ({ bytes_freed: 0 }),
  app_cache_usage: () => ({
    bytes: 0,
    limit_bytes: defaultSettings["cache.limit_gb"] * 1_000_000_000,
  }),
  // Windows: files are dragged out and copied as files.
  app_capabilities: () => ({ drag_out: true, copy_files: "files" }),
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
  app_open_repository: () => null,
  app_place_search: ({ text }) => placeSearch(fakeCountries, text),
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
    reopen: null,
    show_update_notice: false,
  }),
  app_version: () => ({ version: "0.0.0-fake" }),
};
