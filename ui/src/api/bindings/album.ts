// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AlbumChanged } from "./AlbumChanged";
import type { ScanErrors } from "./ScanErrors";
import type { ScanProgress } from "./ScanProgress";
import type { ScanReport } from "./ScanReport";
import type { UnsupportedFiles } from "./UnsupportedFiles";

export type AlbumCommands = {
  album_rescan: { args: Record<string, never>; result: null };
  album_scan_cancel: { args: Record<string, never>; result: null };
  album_scan_errors: { args: { offset: number; limit: number }; result: ScanErrors };
  album_scan_report: { args: Record<string, never>; result: ScanReport };
  album_unsupported: { args: { offset: number; limit: number }; result: UnsupportedFiles };
};

export type AlbumEvents = {
  album_changed: AlbumChanged;
  scan_progress: ScanProgress;
};
