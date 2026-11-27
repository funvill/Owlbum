// Edit mode's part of the review screen (docs/06-editing-and-file-actions.md,
// "Edit mode", "Before and after", "Ending edit mode"): E opens edit mode
// on the current photo, holding `\` shows the original, and leaving the
// album or the review screen ends edit mode as Done.

import { useEffect } from "react";
import { matchKey } from "../../keys";
import { onLeaveAlbum } from "../album/session";
import { useGridStore } from "../grid";
import { holdBefore, useBeforeRelease } from "./before";
import { finishEdit, isEditing, openEdit } from "./mode";

/** E and `\` on the review screen, Before's release, and ending edit mode when the album is left. */
export function useReviewEdit(): void {
  useBeforeRelease();
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const dialog = document.querySelector('[aria-modal="true"]') !== null;
      const match = matchKey(e, { place: "browse", focus: dialog ? "dialog" : undefined });
      if (match === null || (match.id !== "browse.edit" && match.id !== "browse.before")) return;
      e.preventDefault();
      if (!match.act) return;
      if (match.id === "browse.before") {
        holdBefore();
        return;
      }
      const current = useGridStore.getState().current;
      if (current !== null) void openEdit(current.id);
    };
    window.addEventListener("keydown", onKeyDown);
    const stopLeaving = onLeaveAlbum(() => finishEdit("done", { leaving: true }));
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      stopLeaving();
      if (isEditing()) void finishEdit("done", { leaving: true });
    };
  }, []);
}
