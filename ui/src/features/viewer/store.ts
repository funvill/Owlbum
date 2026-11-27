// Whether the viewer is open, and whether its details panel is (kept for
// the session, in memory only). docs/04-browsing.md, "Full-screen viewer".
//
// The viewer has no photo of its own: it shows the grid store's current
// photo, so each step moves the grid behind it and closing leaves the grid
// already in the right place.

import { create } from "zustand";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { isEditing } from "../edit/mode";
import { useGridStore } from "../grid";

export interface ViewerState {
  open: boolean;
  /** The details panel (I). Kept while the page runs, not stored. */
  details: boolean;
}

export const useViewerStore = create<ViewerState>(() => ({ open: false, details: false }));

/** What had focus when the viewer opened, to give it back on close. */
let opener: HTMLElement | null = null;

/**
 * Opens the viewer on `at`. Focus first leaves any field, so text being
 * typed is saved; the photo becomes the current photo if it is not already.
 */
export function openViewer(at: PhotoAt): void {
  // The viewer cannot open over edit mode (docs/06-editing-and-file-actions.md, "Edit mode").
  if (useViewerStore.getState().open || isEditing()) return;
  const active = document.activeElement;
  opener = active instanceof HTMLElement && active !== document.body ? active : null;
  opener?.blur();
  const grid = useGridStore.getState();
  if (grid.current?.id !== at.id) grid.setSelection({ current: at, anchor: at, selection: [at.id] });
  useViewerStore.setState({ open: true });
}

/** Closes the viewer: the last photo shown becomes the current photo and the whole selection. */
export function closeViewer(): void {
  if (!useViewerStore.getState().open) return;
  useViewerStore.setState({ open: false });
  const grid = useGridStore.getState();
  // A step still waiting for its row settles to that photo by itself.
  if (grid.pending === null && grid.current !== null) {
    const current = grid.current;
    grid.setSelection({ current, anchor: current, selection: [current.id] });
  }
  const back = opener;
  opener = null;
  if (back?.isConnected) back.focus();
}

export function toggleDetails(): void {
  useViewerStore.setState((s) => ({ details: !s.details }));
}

/** For tests: closed, details off. */
export function resetViewer(): void {
  opener = null;
  useViewerStore.setState({ open: false, details: false });
}
