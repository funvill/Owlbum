// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AppVersion } from "./AppVersion";
import type { FormatInfo } from "./FormatInfo";

export type AppCommands = {
  app_formats: { args: Record<string, never>; result: Array<FormatInfo> };
  app_version: { args: Record<string, never>; result: AppVersion };
};

export type AppEvents = {
};
