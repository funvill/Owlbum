// Saving edits from the page's side (docs/06-editing-and-file-actions.md,
// "Saving"): Save, Save all, Save as… and Revert to original send photo
// ids to Rust and turn the reply into a toast. Rust counts the edits
// (`edit_status`); the page never reports a count.

import { create } from "zustand";
import { api, toAppError, type Unlisten } from "../../api";
import type { EditStatus } from "../../api/bindings/EditStatus";
import type { SaveProgress } from "../../api/bindings/SaveProgress";
import type { SavedEdits } from "../../api/bindings/SavedEdits";
import { showToast } from "../../controls/toastQueue";
import { editText } from "../../text/edit";

const t = editText;

export interface SaveState {
  /** The counts from `edit_status`; null before the first answer. */
  status: EditStatus | null;
  /** A save started from this page is running. */
  running: boolean;
  /** The last `edit_save_progress` of a running save of several. */
  progress: SaveProgress | null;
}

const idle: SaveState = { status: null, running: false, progress: null };

export const useSaveStore = create<SaveState>(() => idle);

/** For tests: forgets the counts and any running save. */
export function resetSaves(): void {
  useSaveStore.setState(idle, true);
}

/** Whether any save is running now (here or, by Rust's count, anywhere). */
export function saving(): boolean {
  const s = useSaveStore.getState();
  return s.running || (s.status?.saving ?? 0) > 0;
}

/** Asks Rust for the counts again. */
export async function refreshEditStatus(): Promise<void> {
  try {
    const status = await api().invoke("edit_status");
    useSaveStore.setState({ status });
  } catch {
    // No album open: nothing waits.
    useSaveStore.setState({ status: null });
  }
}

let listening: Unlisten[] = [];

/**
 * Keeps the counts current: read now, and again when an edit or a file
 * changes. Answers the function that stops it.
 */
export async function followEditStatus(): Promise<() => void> {
  listening.forEach((u) => u());
  listening = await Promise.all([
    api().listen("album_changed", (e) => {
      if (e.reason !== "info" && e.reason !== "tags") void refreshEditStatus();
    }),
    api().listen("edit_save_progress", (progress) =>
      useSaveStore.setState({
        progress: progress.current === null ? null : progress,
      }),
    ),
  ]);
  void refreshEditStatus();
  const mine = listening;
  return () => {
    mine.forEach((u) => u());
    if (listening === mine) listening = [];
  };
}

/** The one toast for a finished save (docs/06, "Save all"). */
export function savedToast(out: SavedEdits): void {
  const skipped = out.skipped > 0 ? t.skippedCopyOnly(out.skipped) : "";
  const only = out.failed.length === 1 && out.saved.length === 0 ? out.failed[0] : undefined;
  if (only !== undefined) {
    showToast({ kind: "error", message: only.message });
  } else if (out.failed.length > 0) {
    showToast({
      kind: "error",
      message:
        t.savedSomeFailed(
          out.saved.length,
          out.failed.map((f) => f.name),
        ) + skipped,
    });
  } else if (out.saved.length > 0 || out.skipped > 0) {
    showToast({ kind: "info", message: t.saved(out.saved.length) + skipped });
  }
  for (const warning of out.warnings) showToast({ kind: "info", message: warning });
}

async function run(job: () => Promise<SavedEdits>): Promise<void> {
  if (saving()) return;
  useSaveStore.setState({ running: true, progress: null });
  try {
    savedToast(await job());
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
  } finally {
    useSaveStore.setState({ running: false, progress: null });
    void refreshEditStatus();
  }
}

/** Save: writes the edits of `ids` into their files. */
export function saveToFile(ids: readonly number[]): Promise<void> {
  return run(() => api().invoke("edit_save_to_file", { photo_ids: [...ids] }));
}

/** Save all: every photo in the album with an edit. */
export function saveAll(): Promise<void> {
  return run(() => api().invoke("edit_save_all"));
}

/** Save as…: Rust opens the dialog and writes the copy. */
export async function saveAs(id: number): Promise<void> {
  if (saving()) return;
  try {
    const out = await api().invoke("edit_save_as", { photo_id: id });
    if (out.cancelled || out.saved_name === null) return;
    if (out.warning !== null) showToast({ kind: "error", message: out.warning });
    else showToast({ kind: "info", message: t.savedCopy(out.saved_name) });
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
  }
}

/** Revert to original: removes the edits of `ids`, as one undo step. */
export async function revertToOriginal(ids: readonly number[]): Promise<void> {
  try {
    await api().invoke("edit_revert", { photo_ids: [...ids] });
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
  }
}
