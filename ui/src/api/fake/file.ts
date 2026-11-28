import type { FileCommands } from "../bindings/file";
import type { AreaHandlers } from "./types";

/** The most photos copy files, copy path and dragging act on, as Rust's `files::LIMIT`. */
export const FILE_LIMIT = 500;

const unique = (ids: number[]) => [...new Set(ids)];

let asked: number[] = [];

/**
 * The fake backend's `file_` commands. Every id is taken to be a photo
 * whose file exists, and the confirm is always accepted. Nothing is kept:
 * a test that needs the result to change replaces these and emits
 * `album_changed` itself.
 */
export const fileHandlers: AreaHandlers<FileCommands> = {
  file_copy_files: ({ photo_ids }) => {
    const ids = unique(photo_ids);
    return {
      copied: Math.min(ids.length, FILE_LIMIT),
      dropped: Math.max(0, ids.length - FILE_LIMIT),
      as: "files",
      with_edits: false,
    };
  },
  file_copy_image: () => ({ scaled: false, file: false }),
  file_copy_text: () => null,
  file_copy_paths: ({ photo_ids }) => {
    const ids = unique(photo_ids);
    return { copied: Math.min(ids.length, FILE_LIMIT), dropped: Math.max(0, ids.length - FILE_LIMIT) };
  },
  file_prepare_drag: () => null,
  file_reveal: () => null,
  file_start_drag: ({ photo_ids }) => ({ dragged: Math.min(unique(photo_ids).length, FILE_LIMIT), with_edits: false }),
  file_trash_ask: ({ photo_ids }) => {
    asked = unique(photo_ids);
    const n = asked.length;
    if (n === 0) return null;
    return {
      ticket: asked[0]!,
      count: n,
      message: n === 1 ? `Move "p${asked[0]}.jpg" to the Recycle Bin?` : `Move ${n} photos to the Recycle Bin?`,
      detail: `You can restore ${n === 1 ? "it" : "them"} from the Recycle Bin.`,
      button: "Move to Recycle Bin",
    };
  },
  file_trash: ({ ticket }) => {
    const ids = asked[0] === ticket ? asked : [];
    asked = [];
    return { trashed: ids, failed: [], cancelled: ids.length === 0, revision: 0 };
  },
};
