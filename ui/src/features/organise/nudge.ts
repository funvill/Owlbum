// Nudges (docs/05-organising.md, "Nudges (Up and Down)").
//
// Up adds 1 to the score of the targets, Down takes 1 away. The page sends
// one `photo_nudge` per tap, current photo first, and shows the score toast
// from the reply. Rust rescores and sends `album_changed` (reason `info`);
// the grid's re-run brings the new score to the tiles. The selection never
// moves.

import { create } from "zustand";
import { api, toAppError } from "../../api";
import { showToast } from "../../controls/toastQueue";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";
import { canChange } from "./flags";

export type NudgeDelta = 1 | -1;

/** Where the toast shows: over the preview pane, or over the viewer's picture. */
export type ToastPlace = "preview" | "viewer";

export interface ScoreToastState {
  text: string;
  delta: NudgeDelta;
  place: ToastPlace;
  /** Moves on with each press, so a new press restarts the toast. */
  seq: number;
}

export const useScoreToastStore = create<{ toast: ScoreToastState | null }>(() => ({ toast: null }));

/** Removes the toast `seq`, unless a newer press replaced it. */
export function endScoreToast(seq: number): void {
  if (useScoreToastStore.getState().toast?.seq === seq) useScoreToastStore.setState({ toast: null });
}

/** Taps go one after another, so each toast shows the total after the tap before. */
let chain: Promise<unknown> = Promise.resolve();

/** For tests: no toast, nothing waiting. */
export function resetNudges(): void {
  chain = Promise.resolve();
  useScoreToastStore.setState({ toast: null });
}

/**
 * The toast's text (docs/05-organising.md, "The score toast"): "+1 · 74"
 * for one photo, "−1 · 4 photos" for several, "Can't nudge any further"
 * when nothing changed. One photo whose score is not worked out yet shows
 * the change alone.
 */
export function toastText(delta: NudgeDelta, targets: number, changed: number, total: number | null): string {
  if (changed === 0) return t.nudgeLimit;
  const change = delta > 0 ? "+1" : "−1";
  if (targets > 1) return `${change} · ${t.photos(changed)}`;
  return total === null ? change : `${change} · ${total < 0 ? `−${-total}` : total}`;
}

/**
 * Up or Down on the review screen (`selection`) or in the viewer
 * (`viewer`). Targets the whole selection when it has more than one
 * photo, else the current photo; in the viewer, the shown photo only.
 * Does nothing in a view-only album or with no current photo.
 */
export function nudge(delta: NudgeDelta, where: "selection" | "viewer"): Promise<void> {
  const job = async () => {
    if (!canChange()) return;
    const grid = useGridStore.getState();
    const current = grid.current;
    if (current === null) return;
    const ids =
      where === "viewer" || grid.selection.length <= 1
        ? [current.id]
        : [current.id, ...grid.selection.filter((id) => id !== current.id)];
    try {
      const reply = await api().invoke("photo_nudge", { ids, delta });
      const seq = (useScoreToastStore.getState().toast?.seq ?? 0) + 1;
      useScoreToastStore.setState({
        toast: {
          text: toastText(delta, ids.length, reply.changed.length, reply.total),
          delta,
          place: where === "viewer" ? "viewer" : "preview",
          seq,
        },
      });
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
    }
  };
  const next = chain.then(job, job);
  chain = next.catch(() => {});
  return next;
}
