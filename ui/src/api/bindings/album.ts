// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AlbumChanged } from "./AlbumChanged";
import type { ScanProgress } from "./ScanProgress";

export type AlbumCommands = {
  album_rescan: { args: Record<string, never>; result: null };
  album_scan_cancel: { args: Record<string, never>; result: null };
};

export type AlbumEvents = {
  album_changed: AlbumChanged;
  scan_progress: ScanProgress;
};
