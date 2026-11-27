// Saving the album session (docs/04-browsing.md, "Saving"): 1,000 ms after
// the last change to the view, sort, filter, search, current photo or
// scroll index, and at once before the album closes or another opens.
// Restoring it is the grid store's (`restoreSession`).

import { api } from "../../api";
import { sessionOf, useGridStore, type GridState } from "../grid";

/** A change is saved this long after the last one. */
export const SESSION_SAVE_MS = 1000;

let timer: ReturnType<typeof setTimeout> | undefined;
let unsubscribe: (() => void) | null = null;
/** The session last saved or restored, as JSON, so an unchanged one is not saved again. */
let last: string | null = null;

const keyOf = (g: GridState) =>
  JSON.stringify(sessionOf(g.query, g.current?.id ?? null, g.top));

/** Starts saving the session as the grid changes. Calling it again does nothing. */
export function followSession(): void {
  if (unsubscribe !== null) return;
  unsubscribe = useGridStore.subscribe((g, before) => {
    if (!g.restored) {
      last = null;
      clearTimeout(timer);
      timer = undefined;
      return;
    }
    const key = keyOf(g);
    // The session as restored (or as the album opened) is not a change.
    if (!before.restored || last === null) {
      last = key;
      return;
    }
    if (key === last) return;
    clearTimeout(timer);
    timer = setTimeout(() => void flushSession(), SESSION_SAVE_MS);
  });
}

/** Stops saving. For tests and closing the page. */
export function stopSession(): void {
  unsubscribe?.();
  unsubscribe = null;
  clearTimeout(timer);
  timer = undefined;
  last = null;
}

/**
 * Saves the session now if it changed since it was restored or last saved,
 * and drops any save that was waiting. Called before the album closes or
 * another opens, while Rust still has this album open, and on the way out
 * of the window (`app_closing`, once the close handshake exists).
 */
export async function flushSession(): Promise<void> {
  clearTimeout(timer);
  timer = undefined;
  const g = useGridStore.getState();
  if (!g.restored) return;
  const key = keyOf(g);
  if (key === last) return;
  last = key;
  try {
    await api().invoke("app_session_save", { session: JSON.parse(key) });
  } catch {
    // Losing where the person was is not worth a message; the next change saves again.
  }
}
