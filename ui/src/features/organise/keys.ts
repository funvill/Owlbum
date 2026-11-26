// The star and hide keys (docs/05-organising.md, "Keys and controls"): F or
// S stars, H hides, in the grid and the preview (`browse.*`) and in the
// full-screen viewer (`viewer.*`). Matching (typing, dialogs, held keys)
// is the key table's business (`ui/src/keys`).

import { useEffect } from "react";
import { matchKey, type KeyId } from "../../keys";
import { toggleFlag } from "./flags";

/** The rows of the key table this part handles. */
export const FLAG_KEY_IDS = ["browse.star", "browse.hide", "viewer.star", "viewer.hide"] as const satisfies readonly KeyId[];

export type FlagKeyId = (typeof FLAG_KEY_IDS)[number];

/** What each row does. */
export function flagKeyHandlers(): Record<FlagKeyId, () => void> {
  return {
    "browse.star": () => void toggleFlag("star", "selection"),
    "browse.hide": () => void toggleFlag("hidden", "selection"),
    "viewer.star": () => void toggleFlag("star", "viewer"),
    "viewer.hide": () => void toggleFlag("hidden", "viewer"),
  };
}

export const isFlagKey = (id: KeyId): id is FlagKeyId => (FLAG_KEY_IDS as readonly KeyId[]).includes(id);

/** Whether a modal dialog is open anywhere on the page, which stops every review key. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/**
 * Listens for F, S and H on the review screen while mounted. The viewer
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
