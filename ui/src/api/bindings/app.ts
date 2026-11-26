// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AppVersion } from "./AppVersion";
import type { CacheCleared } from "./CacheCleared";
import type { CacheUsage } from "./CacheUsage";
import type { CountryMatch } from "./CountryMatch";
import type { FolderDrop } from "./FolderDrop";
import type { FormatInfo } from "./FormatInfo";
import type { JsonValue } from "./serde_json/JsonValue";
import type { OpenFolder } from "./OpenFolder";
import type { Recent } from "./Recent";
import type { Settings } from "./Settings";
import type { Startup } from "./Startup";

export type AppCommands = {
  app_cache_clear: { args: Record<string, never>; result: CacheCleared };
  app_cache_usage: { args: Record<string, never>; result: CacheUsage };
  app_formats: { args: Record<string, never>; result: Array<FormatInfo> };
  app_pick_folder: { args: Record<string, never>; result: string | null };
  app_place_search: { args: { text: string }; result: Array<CountryMatch> };
  app_recent_list: { args: Record<string, never>; result: Array<Recent> };
  app_recent_remove: { args: { root: string }; result: Array<Recent> };
  app_settings_get: { args: Record<string, never>; result: Settings };
  app_settings_set: { args: { patch: { [key in string]: JsonValue } }; result: Settings };
  app_startup: { args: Record<string, never>; result: Startup };
  app_version: { args: Record<string, never>; result: AppVersion };
};

export type AppEvents = {
  folder_drop: FolderDrop;
  open_folder: OpenFolder;
};
