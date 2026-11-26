import type { AlbumCommands } from "../bindings/album";
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

/**
 * The fake backend's `album_` commands. The scan commands only answer: a
 * test pushes `scan_progress` events itself with `emit`, and serves its own
 * report or lists with `handle` (and `pageOf`).
 */
export const albumHandlers: AreaHandlers<AlbumCommands> = {
  album_rescan: () => null,
  album_scan_cancel: () => null,
  album_scan_report: () => structuredClone(emptyScanReport),
  album_scan_errors: ({ offset, limit }) => pageOf([], offset, limit),
  album_unsupported: ({ offset, limit }) => pageOf([], offset, limit),
};
