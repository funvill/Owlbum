// Nudges (docs/05-organising.md, "Nudges (Up and Down)").
//
// Up adds 1 to the score of the targets, Down takes 1 away. The page sends
// one `photo_nudge` per tap, current photo first, and shows the score toast
// from the reply: the change alone, never the new total (owner's feedback,
// issue 168). Rust rescores and sends `album_changed` (reason `info`);
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
  /** The change the toast adds up while it shows; 0 for "Can't nudge any further". */
  change: number;
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
 * The toast's text (docs/05-organising.md, "The score toast"): the change
 * alone, "+1" or "−5" with a real minus sign, or "Can't nudge any further"
 * when the tap changed nothing.
 */
export function toastText(change: number): string {
  if (change === 0) return t.nudgeLimit;
  return change < 0 ? `−${-change}` : `+${change}`;
}

/**
 * The change a toast shows after a tap of `delta` that changed `changed`
 * photos: taps in the same direction add up while the toast is still
 * showing in the same place, so five quick Ups read "+5".
 */
export function toastChange(
  before: ScoreToastState | null,
  delta: NudgeDelta,
  changed: number,
  place: ToastPlace,
): number {
  if (changed === 0) return 0;
  if (before !== null && before.place === place && before.change !== 0 && Math.sign(before.change) === delta) {
    return before.change + delta;
  }
  return delta;
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
      const before = useScoreToastStore.getState().toast;
      const place: ToastPlace = where === "viewer" ? "viewer" : "preview";
      const change = toastChange(before, delta, reply.changed.length, place);
      useScoreToastStore.setState({
        toast: { text: toastText(change), delta, change, place, seq: (before?.seq ?? 0) + 1 },
      });
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
    }
  };
  const next = chain.then(job, job);
  chain = next.catch(() => {});
  return next;
}
