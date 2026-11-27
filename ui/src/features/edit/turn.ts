// Rotate and flip outside edit mode (docs/06-editing-and-file-actions.md,
// "Rotate and flip outside edit mode").
//
// `[` turns left and `]` turns right in the grid, the timeline, the map,
// the preview (`browse.*`) and the viewer (`viewer.*`). Each press is one
// `edit_turn` call and one undo step for every target. Rust stores the
// turned edits and sends `album_changed` (reason `edit`); the grid's
// re-run brings the new plans to the tiles.

import { useEffect } from "react";
import { api, toAppError } from "../../api";
import type { TurnOp } from "../../api/bindings/TurnOp";
import { showToast } from "../../controls/toastQueue";
import { matchKey, type KeyId } from "../../keys";
import { editText as t } from "../../text/edit";
import { useGridStore } from "../grid";
import { canChange } from "../organise";

/** Presses go one after another, so each turns the edit the last one stored. */
let chain: Promise<unknown> = Promise.resolve();

/** For tests: nothing waiting. */
export function resetTurns(): void {
  chain = Promise.resolve();
}

/**
 * Turns or flips the targets: the whole selection when it has more than
 * one photo, else the current photo; in the viewer, the shown photo only.
 * Does nothing in a view-only album or with no current photo. Videos and
 * animated pictures are skipped, with an info toast.
 */
export function turn(op: TurnOp, where: "selection" | "viewer", ids?: number[]): Promise<void> {
  const job = async () => {
    if (!canChange()) return;
    const grid = useGridStore.getState();
    const current = grid.current;
    const targets =
      ids ??
      (current === null
        ? []
        : where === "viewer" || grid.selection.length <= 1
          ? [current.id]
          : [current.id, ...grid.selection.filter((id) => id !== current.id)]);
    if (targets.length === 0) return;
    try {
      const reply = await api().invoke("edit_turn", { photo_ids: targets, op });
      if (reply.skipped > 0) showToast({ kind: "info", message: t.skipped(reply.skipped) });
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
    }
  };
  const next = chain.then(job, job);
  chain = next.catch(() => {});
  return next;
}

/** The rows of the key table this part handles outside edit mode. */
export const ROTATE_KEY_IDS = [
  "browse.rotate_left",
  "browse.rotate_right",
  "viewer.rotate_left",
  "viewer.rotate_right",
] as const satisfies readonly KeyId[];

export type RotateKeyId = (typeof ROTATE_KEY_IDS)[number];

/** What each row does. */
export function rotateKeyHandlers(): Record<RotateKeyId, () => void> {
  return {
    "browse.rotate_left": () => void turn("rotate_left", "selection"),
    "browse.rotate_right": () => void turn("rotate_right", "selection"),
    "viewer.rotate_left": () => void turn("rotate_left", "viewer"),
    "viewer.rotate_right": () => void turn("rotate_right", "viewer"),
  };
}

export const isRotateKey = (id: KeyId): id is RotateKeyId => (ROTATE_KEY_IDS as readonly KeyId[]).includes(id);

/**
 * Listens for `[` and `]` on the review screen while mounted. The viewer
 * takes every key while it is open and calls `rotateKeyHandlers` itself.
 */
export function useRotateKeys(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const dialog = document.querySelector('[aria-modal="true"]') !== null;
      const match = matchKey(e, { place: "browse", focus: dialog ? "dialog" : undefined });
      if (match === null || !isRotateKey(match.id)) return;
      e.preventDefault();
      if (match.act) rotateKeyHandlers()[match.id]();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
