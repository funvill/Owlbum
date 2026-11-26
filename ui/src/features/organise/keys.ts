// The review keys of organising (docs/05-organising.md, "Keys and
// controls" and "Nudges"): F or S stars, H hides, Up and Down nudge the
// score, in the grid and the preview (`browse.*`) and in the full-screen
// viewer (`viewer.*`). Matching (typing, dialogs, held keys)
// is the key table's business (`ui/src/keys`).

import { useEffect } from "react";
import { matchKey, type KeyId } from "../../keys";
import { toggleFlag } from "./flags";
import { nudge } from "./nudge";

/** The rows of the key table this part handles. */
export const FLAG_KEY_IDS = [
  "browse.star",
  "browse.hide",
  "browse.score_up",
  "browse.score_down",
  "viewer.star",
  "viewer.hide",
  "viewer.score_up",
  "viewer.score_down",
] as const satisfies readonly KeyId[];

export type FlagKeyId = (typeof FLAG_KEY_IDS)[number];

/** What each row does. */
export function flagKeyHandlers(): Record<FlagKeyId, () => void> {
  return {
    "browse.star": () => void toggleFlag("star", "selection"),
    "browse.hide": () => void toggleFlag("hidden", "selection"),
    "viewer.star": () => void toggleFlag("star", "viewer"),
    "viewer.hide": () => void toggleFlag("hidden", "viewer"),
    "browse.score_up": () => void nudge(1, "selection"),
    "browse.score_down": () => void nudge(-1, "selection"),
    "viewer.score_up": () => void nudge(1, "viewer"),
    "viewer.score_down": () => void nudge(-1, "viewer"),
  };
}

export const isFlagKey = (id: KeyId): id is FlagKeyId => (FLAG_KEY_IDS as readonly KeyId[]).includes(id);

/** Whether a modal dialog is open anywhere on the page, which stops every review key. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/**
 * Listens for F, S, H, Up and Down on the review screen while mounted. The viewer
 * takes every key while it is open and calls `flagKeyHandlers` itself.
 */
export function useFlagKeys(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const match = matchKey(e, { place: "browse", focus: dialogOpen() ? "dialog" : undefined });
      if (match === null || !isFlagKey(match.id)) return;
      e.preventDefault();
      if (match.act) flagKeyHandlers()[match.id]();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
