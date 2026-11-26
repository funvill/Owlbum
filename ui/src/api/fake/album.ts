import type { AlbumCommands } from "../bindings/album";
import type { AreaHandlers } from "./types";

/**
 * The fake backend's `album_` commands. The scan commands only answer: a
 * test pushes `scan_progress` events itself with `emit`.
 */
export const albumHandlers: AreaHandlers<AlbumCommands> = {
  album_rescan: () => null,
  album_scan_cancel: () => null,
};
