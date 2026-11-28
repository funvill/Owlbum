// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { About } from "./About";
import type { AppClosing } from "./AppClosing";
import type { AppMenu } from "./AppMenu";
import type { AppVersion } from "./AppVersion";
import type { CacheCleared } from "./CacheCleared";
import type { CacheUsage } from "./CacheUsage";
import type { Capabilities } from "./Capabilities";
import type { CountryMatch } from "./CountryMatch";
import type { FolderDrop } from "./FolderDrop";
import type { FormatInfo } from "./FormatInfo";
import type { JsonValue } from "./serde_json/JsonValue";
import type { OpenFolder } from "./OpenFolder";
import type { Recent } from "./Recent";
import type { Settings } from "./Settings";
import type { Startup } from "./Startup";

export type AppCommands = {
  app_about: { args: Record<string, never>; result: About };
  app_cache_clear: { args: Record<string, never>; result: CacheCleared };
  app_cache_usage: { args: Record<string, never>; result: CacheUsage };
  app_capabilities: { args: Record<string, never>; result: Capabilities };
  app_close_ready: { args: Record<string, never>; result: null };
  app_country_at: { args: { lat: number; lon: number }; result: string | null };
  app_formats: { args: Record<string, never>; result: Array<FormatInfo> };
  app_open_feedback: { args: Record<string, never>; result: null };
  app_open_repository: { args: Record<string, never>; result: null };
  app_pick_folder: { args: Record<string, never>; result: string | null };
  app_place_search: { args: { text: string }; result: Array<CountryMatch> };
  app_quit: { args: Record<string, never>; result: null };
  app_recent_list: { args: Record<string, never>; result: Array<Recent> };
  app_recent_remove: { args: { root: string }; result: Array<Recent> };
  app_session_save: { args: { session: JsonValue }; result: null };
  app_settings_get: { args: Record<string, never>; result: Settings };
  app_settings_set: { args: { patch: { [key in string]: JsonValue } }; result: Settings };
  app_startup: { args: Record<string, never>; result: Startup };
  app_version: { args: Record<string, never>; result: AppVersion };
};

export type AppEvents = {
  app_closing: AppClosing;
  app_menu: AppMenu;
  folder_drop: FolderDrop;
  open_folder: OpenFolder;
};
