// File actions from the page's side (docs/06-editing-and-file-actions.md,
// "File actions"): each sends photo ids to Rust and turns the reply into
// a toast. Rust resolves the ids, shows the confirm and writes the
// clipboard; the page never handles a path.

import { api, mediaPlatform, toAppError } from "../../api";
import { showToast } from "../../controls/toastQueue";
import { binName, filesText } from "../../text/files";

const t = filesText.toast;

/** The bin's name here. */
export function bin(): string {
  return binName(mediaPlatform());
}

function failed(e: unknown): void {
  showToast({ kind: "error", message: toAppError(e).message });
}

let editsNoted = false;

/** Once per session: the files dragged or copied do not carry album edits. */
export function noteEditsNotInFiles(withEdits: boolean): void {
  if (!withEdits || editsNoted) return;
  editsNoted = true;
  showToast({ kind: "info", message: t.editsNotInFiles });
}

/** For tests: the note shows again. */
export function resetFileNotes(): void {
  editsNoted = false;
}

/**
 * Moves `ids` to the Recycle Bin or Trash. Rust asks first. Moved photos
 * leave the result when the grid re-runs for `album_changed`; failures
 * show one error toast. Answers the ids moved.
 */
export async function trashPhotos(ids: readonly number[]): Promise<number[]> {
  if (ids.length === 0) return [];
  try {
    const out = await api().invoke("file_trash", { photo_ids: [...ids] });
    const first = out.failed[0];
    if (first !== undefined) {
      showToast({
        kind: "error",
        message:
          out.failed.length === 1
            ? t.trashFailedOne(bin(), first.name, first.message)
            : t.trashFailedMany(bin(), out.failed.length, first.message),
      });
    }
    return out.trashed;
  } catch (e) {
    failed(e);
    return [];
  }
}

/** Ctrl+C or Copy: one photo copies its picture, several copy their files. */
export async function copyPhotos(ids: readonly number[]): Promise<void> {
  if (ids.length === 0) return;
  try {
    if (ids.length === 1) {
      const out = await api().invoke("file_copy_image", { photo_id: ids[0]! });
      showToast({ kind: "success", message: out.scaled ? t.copiedScaled : t.copied });
      return;
    }
    const out = await api().invoke("file_copy_files", { photo_ids: [...ids] });
    const edits = out.with_edits ? t.withoutEdits : "";
    const message =
      out.as === "paths" ? t.copiedPathsInstead(out.copied) : t.copiedFiles(out.copied, out.dropped) + edits;
    showToast({ kind: "success", message });
    if (out.as === "files") noteEditsNotInFiles(out.with_edits);
  } catch (e) {
    failed(e);
  }
}

/** Copy path: the paths of `ids` as text, one per line. */
export async function copyPaths(ids: readonly number[]): Promise<void> {
  if (ids.length === 0) return;
  try {
    const out = await api().invoke("file_copy_paths", { photo_ids: [...ids] });
    showToast({ kind: "success", message: t.copiedPaths(out.copied, out.dropped) });
  } catch (e) {
    failed(e);
  }
}

/** Show in folder: the file manager with the photo's file selected. */
export async function revealPhoto(id: number): Promise<void> {
  try {
    await api().invoke("file_reveal", { photo_id: id });
  } catch (e) {
    failed(e);
  }
}
