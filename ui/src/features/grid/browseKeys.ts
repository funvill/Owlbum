// The review screen's own keys (docs/04-browsing.md, "Keys", the rows
// marked 04): stepping through the result, Enter and Space, Esc, Ctrl+A
// and Ctrl+D. Which key reaches which row is the key table's business
// (`ui/src/keys`); this file says only what each row does.

import { useEffect, useRef } from "react";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { matchKey, type KeyId } from "../../keys";
import { reduceToCurrent, selectAll, selectNone, step, stepToEnd } from "./selection";
import { useGridStore } from "./store";

/** The rows of the key table this part handles on the review screen. */
export const BROWSE_KEY_IDS = [
  "browse.previous",
  "browse.next",
  "browse.extend_previous",
  "browse.extend_next",
  "browse.first",
  "browse.last",
  "browse.open_viewer",
  "browse.escape",
  "browse.select_all",
  "browse.select_none",
] as const satisfies readonly KeyId[];

export type BrowseKeyId = (typeof BROWSE_KEY_IDS)[number];

export interface BrowseKeyOptions {
  /** Open the viewer on the current photo (#22). Without it, Enter and Space do nothing. */
  onOpen?: (at: PhotoAt) => void;
}

/**
 * What each row does. A handler answers false when the key did nothing
 * and should keep its default (Enter with no current photo).
 */
export function browseKeyHandlers(options: BrowseKeyOptions): Record<BrowseKeyId, () => boolean | void> {
  return {
    "browse.previous": () => step(-1),
    "browse.next": () => step(1),
    "browse.extend_previous": () => step(-1, true),
    "browse.extend_next": () => step(1, true),
    "browse.first": () => stepToEnd("first"),
    "browse.last": () => stepToEnd("last"),
    "browse.open_viewer": () => {
      const current = useGridStore.getState().current;
      if (current === null || options.onOpen === undefined) return false;
      options.onOpen(current);
    },
    // The photo menu (#38) closes itself on Esc while it has focus.
    "browse.escape": () => reduceToCurrent(),
    "browse.select_all": () => void selectAll(),
    "browse.select_none": () => selectNone(),
  };
}

const isBrowseKey = (id: KeyId): id is BrowseKeyId => (BROWSE_KEY_IDS as readonly KeyId[]).includes(id);

/** Whether a modal dialog is open anywhere on the page, which stops every review key. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/**
 * Listens for the review screen's keys while mounted. Keys the key table
 * gives to other parts are left alone; a key this part owns has its
 * default blocked, even an auto-repeat that does not act.
 */
export function useBrowseKeys(options: BrowseKeyOptions = {}): void {
  const latest = useRef(options);
  useEffect(() => {
    latest.current = options;
  });
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const match = matchKey(e, { place: "browse", focus: dialogOpen() ? "dialog" : undefined });
      if (match === null || !isBrowseKey(match.id)) return;
      if (match.act && browseKeyHandlers(latest.current)[match.id]() === false) return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
