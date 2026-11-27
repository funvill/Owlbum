import type { AlbumCommands } from "../bindings/album";
import type { AlbumStatus } from "../bindings/AlbumStatus";
import type { ScanReport } from "../bindings/ScanReport";
import type { AreaHandlers } from "./types";

/** The most items one page of `album_scan_errors` or `album_unsupported` holds, as in Rust. */
export const PAGE_MAX = 500;

/** The scan report before any walk has ended: everything empty. */
export const emptyScanReport: ScanReport = {
  unreadable_folders: [],
  unreadable_folder_count: 0,
  nested_albums: [],
  nested_album_count: 0,
  links: 0,
  online_only: 0,
  unreadable_names: 0,
  unsupported: [],
  notices: [],
};

/**
 * One page of `items`, as Rust answers `album_scan_errors` and `album_unsupported`:
 * from `offset`, at most `limit` and at most 500, with the total.
 */
export function pageOf<T>(items: readonly T[], offset: number, limit: number): { total: number; items: T[] } {
  return { total: items.length, items: items.slice(offset, offset + Math.min(limit, PAGE_MAX)) };
}

/** The last part of a path, as Rust names an opened folder. */
function lastPart(path: string): string {
  const parts = path.split(/[\\/]/).filter((p) => p !== "");
  return parts[parts.length - 1] ?? path;
}

/**
 * The status of a writable album opened at `folder` (its own root), with no
 * photos yet and its first scan running. `overrides` replaces any field.
 */
export function fakeStatus(folder: string, overrides: Partial<AlbumStatus> = {}): AlbumStatus {
  return {
    root: folder,
    scope: "",
    folder,
    name: lastPart(folder),
    view_only: null,
    in_memory: false,
    lock_host: null,
    conflict_file: null,
    scanning: true,
    photos: 0,
    unsupported: { count: 0, labels: [] },
    unreadable: 0,
    scan: { phase: "walking", found: 0, done: 0, failed: 0, watching: true, unsupported: 0, skipped: 0 },
    revision: 0,
    ...overrides,
  };
}

/**
 * The fake backend's `album_` commands. The scan commands only answer: a
 * test pushes `scan_progress` events itself with `emit`, and serves its own
 * report or lists with `handle` (and `pageOf`). `album_open` opens any
 * path as a new album (`fakeStatus`); `album_status` answers null, as
 * when no album is open; `album_undo` has nothing to undo; `album_tidy_list`
 * finds nothing missing and `album_tidy_remove` removes nothing.
 */
export const albumHandlers: AreaHandlers<AlbumCommands> = {
  album_people: () => [],
  album_open: ({ path }) => ({ status: fakeStatus(path), session: null }),
  album_close: () => null,
  album_copy_not_shown: () => 0,
  album_status: () => null,
  album_undo: () => ({ outcome: "nothing", label: null, ids: [], revision: 0 }),
  album_tidy_list: () => [],
  album_tidy_remove: () => ({ removed: 0, kept: 0, revision: 0 }),
  album_rescan: () => null,
  album_scan_cancel: () => null,
  album_scan_report: () => structuredClone(emptyScanReport),
  album_scan_errors: ({ offset, limit }) => pageOf([], offset, limit),
  album_unsupported: ({ offset, limit }) => pageOf([], offset, limit),
};
