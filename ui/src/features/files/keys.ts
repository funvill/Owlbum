// The keys of file actions (docs/06-editing-and-file-actions.md, "Move to
// the Recycle Bin or Trash" and "Copy image"): Delete and Ctrl+C in the
// grid, the timeline, the map and the preview (`browse.*`), Ctrl+C in the
// viewer (`viewer.copy`). Delete is left out of the viewer on purpose.

import { useEffect } from "react";
import { focusOf, matchKey, type KeyId } from "../../keys";
import { canChange } from "../organise";
import { useViewerStore } from "../viewer/store";
import { copyPhotos, trashPhotos } from "./actions";
import { loadCapabilities } from "./capabilities";
import { selectedTargets } from "./targets";
import { useGridStore } from "../grid";

/** The rows of the key table this part handles. */
export const FILE_KEY_IDS = ["browse.copy", "browse.trash", "viewer.copy"] as const satisfies readonly KeyId[];

export type FileKeyId = (typeof FILE_KEY_IDS)[number];

export const isFileKey = (id: KeyId): id is FileKeyId => (FILE_KEY_IDS as readonly KeyId[]).includes(id);

/** What each row does. */
export function fileKeyHandlers(): Record<FileKeyId, () => void> {
  return {
    "browse.copy": () => void copyPhotos(selectedTargets()),
    "browse.trash": () => {
      if (!canChange()) return;
      void trashPhotos(selectedTargets());
    },
    // The viewer copies the photo it shows.
    "viewer.copy": () => {
      const current = useGridStore.getState().current;
      if (current !== null) void copyPhotos([current.id]);
    },
  };
}

/** Whether a modal dialog is open anywhere on the page, which stops every review key. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/**
 * Listens for Delete and Ctrl+C on the review screen while mounted. Not
 * while typing, in a dialog or a menu, or while the viewer is open (the
 * viewer takes its keys and calls `fileKeyHandlers` for Ctrl+C itself).
 */
export function useFileKeys(): void {
  useEffect(() => {
    // Asked once, so the first drag knows whether it may start.
    void loadCapabilities();
    const onKeyDown = (e: KeyboardEvent) => {
      if (useViewerStore.getState().open) return;
      const focus = dialogOpen() ? "dialog" : focusOf(e.target);
      if (focus === "menu") return;
      const match = matchKey(e, { place: "browse", focus });
      if (match === null || !isFileKey(match.id)) return;
      e.preventDefault();
      if (match.act) fileKeyHandlers()[match.id]();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
