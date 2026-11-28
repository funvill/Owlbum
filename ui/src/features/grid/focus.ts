// The photos to tag first (docs/07-tagging.md, "Ordering"): the selected
// photo, then the photos in the rows on screen, at most 200. Sent with
// `tagging_focus` 500 ms after the selection, the scroll position or the
// query last changed, and only when the list differs from the last one sent.

import { api } from "../../api";
import { gridBlocks, useGridStore } from "./store";

/** The list is sent this long after the last change. */
export const FOCUS_DELAY_MS = 500;
/** The most ids sent. */
export const FOCUS_MAX = 200;

let shown = { first: 0, last: -1 };
let timer: ReturnType<typeof setTimeout> | undefined;
let lastSent: string | null = null;

/** The grid reports the tiles it draws (the rows on screen). */
export function reportShown(first: number, last: number): void {
  if (shown.first === first && shown.last === last) return;
  shown = { first, last };
  schedule();
}

function schedule(): void {
  clearTimeout(timer);
  timer = setTimeout(send, FOCUS_DELAY_MS);
}

/** The focus list now: the current photo, then the rows on screen that are held. */
export function focusIds(): number[] {
  const ids: number[] = [];
  const seen = new Set<number>();
  const add = (id: number) => {
    if (seen.has(id) || ids.length >= FOCUS_MAX) return;
    seen.add(id);
    ids.push(id);
  };
  const current = useGridStore.getState().current;
  if (current !== null) add(current.id);
  const blocks = gridBlocks();
  for (let i = shown.first; i <= shown.last && ids.length < FOCUS_MAX; i++) {
    const row = blocks.row(i);
    if (row !== undefined) add(row.id);
  }
  return ids;
}

function send(): void {
  timer = undefined;
  const ids = focusIds();
  if (ids.length === 0) return;
  const key = ids.join(",");
  if (key === lastSent) return;
  lastSent = key;
  // Rust ignores ids it does not know; a failure only loses the order.
  api()
    .invoke("tagging_focus", { ids })
    .catch(() => {});
}

/**
 * Follows the selection, the result and the rows that arrive, while a grid
 * is shown. A new album starts a new list. Answers the function that stops.
 */
export function startFocus(): () => void {
  lastSent = null;
  const unsubscribe = useGridStore.subscribe((s, before) => {
    if (!s.restored && before.restored) lastSent = null;
    if (s.current?.id !== before.current?.id || s.result !== before.result) schedule();
  });
  let blocks = gridBlocks();
  let unblock = blocks.subscribe(schedule);
  const swap = useGridStore.subscribe(() => {
    // `resetGrid` makes a new block store.
    if (gridBlocks() === blocks) return;
    unblock();
    blocks = gridBlocks();
    unblock = blocks.subscribe(schedule);
  });
  return () => {
    unsubscribe();
    swap();
    unblock();
    clearTimeout(timer);
    timer = undefined;
    shown = { first: 0, last: -1 };
  };
}
