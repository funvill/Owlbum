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

/** The fake backend's `app_` commands. */
export const appHandlers: AreaHandlers<AppCommands> = {
  // An empty cache at the default limit. A test that needs a size replaces these.
  app_cache_clear: () => ({ bytes_freed: 0 }),
  app_cache_usage: () => ({ bytes: 0, limit_bytes: defaultSettings["cache.limit_gb"] * 1_000_000_000 }),
  app_formats: () => fakeFormats.map((f) => ({ ...f, extensions: [...f.extensions] })),
  app_recent_list: () => [],
  app_recent_remove: () => [],
  app_settings_get: () => structuredClone(defaultSettings),
  // Stateless: answers the defaults with the patch applied. A test that
  // needs settings to persist between calls replaces these handlers.
  app_settings_set: ({ patch }) => {
    for (const [key, value] of Object.entries(patch)) {
      const range = ranges[key as keyof Settings];
      if (range !== undefined && (typeof value !== "number" || value < range[0] || value > range[1])) {
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
  app_version: () => ({ version: "0.0.0-fake" }),
};
