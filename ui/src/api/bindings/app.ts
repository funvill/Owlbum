// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AppVersion } from "./AppVersion";
import type { FormatInfo } from "./FormatInfo";
import type { JsonValue } from "./serde_json/JsonValue";
import type { Recent } from "./Recent";
import type { Settings } from "./Settings";

export type AppCommands = {
  app_formats: { args: Record<string, never>; result: Array<FormatInfo> };
  app_recent_list: { args: Record<string, never>; result: Array<Recent> };
  app_recent_remove: { args: { root: string }; result: Array<Recent> };
  app_settings_get: { args: Record<string, never>; result: Settings };
  app_settings_set: { args: { patch: { [key in string]: JsonValue } }; result: Settings };
  app_version: { args: Record<string, never>; result: AppVersion };
};

export type AppEvents = {
};
