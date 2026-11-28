// The keyboard shortcut overlay's state and contents
// (docs/08-desktop-and-release.md, "The overlay"). Everything it lists
// comes from the key table.

import { useEffect } from "react";
import { create } from "zustand";
import { KEYMAP, keyMatches, matchKey, type KeyId, type KeyRow, type Place } from "../../keys";

export const useShortcutsStore = create<{ open: boolean }>(() => ({
  open: false,
}));

export function openShortcuts(): void {
  useShortcutsStore.setState({ open: true });
}

export function closeShortcuts(): void {
  useShortcutsStore.setState({ open: false });
}

export function toggleShortcuts(): void {
  useShortcutsStore.setState((s) => ({ open: !s.open }));
}

/** Rows whose action does not exist yet, left out of the overlay. None now. */
export const NOT_YET: readonly KeyId[] = [];

/** A place the overlay lists, as it names it. */
export type Where = Exclude<Place, "everywhere">;

/** One group of the overlay. */
export interface Section {
  place: Place;
  /** The place the user is in. */
  here: boolean;
  rows: KeyRow[];
}

/**
 * The overlay's groups: the place the user is in first, "Everywhere"
 * beside it, then the other places. Development-only rows, and rows
 * whose action does not exist yet, are left out.
 */
export function sections(here: Where): Section[] {
  const order: Place[] = [here, "everywhere", ...(["browse", "viewer", "edit"] as const).filter((p) => p !== here)];
  return order.map((place) => ({
    place,
    here: place === here,
    rows: KEYMAP.filter((r) => r.place === place && !r.dev && !NOT_YET.includes(r.id)),
  }));
}

/** Whether a modal layer other than a full-page surface (the viewer) is open. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]:not([data-keys="surface"])') !== null;
}

/**
 * Listens for `?` while mounted: it opens the overlay from every place, and
 * closes it again. It does not open while typing or over another dialog.
 * `place` says where the user is now.
 */
export function useShortcutKey(place: () => Where): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (useShortcutsStore.getState().open) {
        if (!e.repeat && !e.isComposing && keyMatches(e, "app.shortcuts")) {
          e.preventDefault();
          closeShortcuts();
        }
        return;
      }
      const match = matchKey(e, {
        place: place(),
        focus: dialogOpen() ? "dialog" : undefined,
      });
      if (match === null || match.id !== "app.shortcuts") return;
      e.preventDefault();
      if (match.act) openShortcuts();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [place]);
}
