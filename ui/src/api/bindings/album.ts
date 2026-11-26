// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { AlbumChanged } from "./AlbumChanged";
import type { AlbumOpened } from "./AlbumOpened";
import type { AlbumStatus } from "./AlbumStatus";
import type { OpenOptions } from "./OpenOptions";
import type { PersonCount } from "./PersonCount";
import type { ScanErrors } from "./ScanErrors";
import type { ScanProgress } from "./ScanProgress";
import type { ScanReport } from "./ScanReport";
import type { TidyFolder } from "./TidyFolder";
import type { TidyRemoved } from "./TidyRemoved";
import type { Undone } from "./Undone";
import type { UnsupportedFiles } from "./UnsupportedFiles";

export type AlbumCommands = {
  album_close: { args: Record<string, never>; result: null };
  album_open: { args: { path: string; options: OpenOptions | null }; result: AlbumOpened };
  album_people: { args: Record<string, never>; result: Array<PersonCount> };
  album_rescan: { args: Record<string, never>; result: null };
  album_scan_cancel: { args: Record<string, never>; result: null };
  album_scan_errors: { args: { offset: number; limit: number }; result: ScanErrors };
  album_scan_report: { args: Record<string, never>; result: ScanReport };
  album_status: { args: Record<string, never>; result: AlbumStatus | null };
  album_tidy_list: { args: Record<string, never>; result: Array<TidyFolder> };
  album_tidy_remove: { args: { ids: Array<number> }; result: TidyRemoved };
  album_undo: { args: Record<string, never>; result: Undone };
  album_unsupported: { args: { offset: number; limit: number }; result: UnsupportedFiles };
};

export type AlbumEvents = {
  album_changed: AlbumChanged;
  scan_progress: ScanProgress;
};
