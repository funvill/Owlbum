// Undo (docs/05-organising.md, "Undo (Ctrl+Z)").
//
// Rust keeps the undo list; the page only asks for `album_undo` and shows
// the toast for its outcome. Rust sends `album_changed` (reason `info`)
// when it put anything back. One photo put back becomes the selection (its
// stack opened if it is folded away); otherwise the grid's re-run keeps
// the selection.

import { api, toAppError } from "../../api";
import type { Undone } from "../../api/bindings/Undone";
import { showToast, type ToastInput } from "../../controls/toastQueue";
import { organiseText as t } from "../../text/organise";
import { selectById, useGridStore } from "../grid";
import { canChange, forgetSent, queue } from "./flags";

/** The toast for an `album_undo` answer. */
export function undoToast(undone: Undone): ToastInput {
  const label = undone.label ?? "";
  switch (undone.outcome) {
    case "nothing":
      return { kind: "info", message: t.undo.nothing };
    case "done":
      return { kind: "info", message: t.undo.done(label) };
    case "partial":
      return { kind: "error", message: t.undo.partial(label) };
    case "gone":
      return { kind: "error", message: t.undo.gone(label) };
  }
}

/**
 * Ctrl+Z on the review screen or in the viewer. Runs after any star or
 * hide press already made. Does nothing in a view-only album.
 */
export function undo(): Promise<void> {
  return queue(async () => {
    if (!canChange()) return;
    let undone: Undone;
    try {
      undone = await api().invoke("album_undo");
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
      return;
    }
    forgetSent(undone.ids);
    showToast(undoToast(undone));
    const [only] = undone.ids;
    if (undone.ids.length === 1 && only !== undefined) {
      // A photo that was hidden is not in the held result until the re-run.
      await useGridStore.getState().refresh();
      await selectById(only);
    }
  });
}
