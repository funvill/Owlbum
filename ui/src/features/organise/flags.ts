// Star and hide (docs/05-organising.md, "Star and hide").
//
// The page works out which photos change and the new value, and sends one
// `photo_set_info`. Rust writes it, records the undo step and sends
// `album_changed` (reason `info`); the grid's re-run for that event keeps
// the selection by the rules of "Where the selection goes afterwards", so
// nothing here moves the selection itself.

import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { InfoPatch } from "../../api/bindings/InfoPatch";
import type { Row } from "../../api/bindings/Row";
import { showToast } from "../../controls/toastQueue";
import { useAlbumStore, viewOnlyReason } from "../album";
import { gridBlocks, useGridStore } from "../grid";

export type Flag = "star" | "hidden";

/** Which mark plays: one per change (docs/05-organising.md, "The mark"). */
export type MarkKind = "starred" | "unstarred" | "hidden" | "unhidden";

/** Where a mark plays: over the preview pane, or over the viewer's picture. */
export type MarkPlace = "preview" | "viewer";

export interface Mark {
  kind: MarkKind;
  place: MarkPlace;
  /** Moves on with each press, so a new press restarts the mark. */
  seq: number;
}

export const useMarkStore = create<{ mark: Mark | null }>(() => ({ mark: null }));

/** Removes the mark `seq`, unless a newer press replaced it. */
export function endMark(seq: number): void {
  if (useMarkStore.getState().mark?.seq === seq) useMarkStore.setState({ mark: null });
}

function playMark(flag: Flag, value: boolean, place: MarkPlace): void {
  const kind: MarkKind = flag === "star" ? (value ? "starred" : "unstarred") : value ? "hidden" : "unhidden";
  const seq = (useMarkStore.getState().mark?.seq ?? 0) + 1;
  useMarkStore.setState({ mark: { kind, place, seq } });
}

/**
 * The values sent, by flag and photo, with the result token they were sent
 * under. A row read before the re-run lands still shows the old value, so a
 * second press soon after the first must read what the first sent.
 */
const sent = new Map<string, { value: boolean; token: string | null }>();

/** Presses go one after another, so each reads the value the one before sent. */
let chain: Promise<unknown> = Promise.resolve();

/** For tests: nothing sent, no mark. */
export function resetFlags(): void {
  sent.clear();
  chain = Promise.resolve();
  useMarkStore.setState({ mark: null });
}

/** Whether the open album takes changes. With no status read yet, it does. */
export function canChange(): boolean {
  const status = useAlbumStore.getState().status;
  return status === null || viewOnlyReason(status) === null;
}

/**
 * The flag's value for `row`, at `index` of the result held now (null when
 * not known). What was sent wins until a row of a later result is held.
 */
function valueOf(flag: Flag, row: Row, index: number | null): boolean {
  const blocks = gridBlocks();
  const known = sent.get(`${flag}:${row.id}`);
  if (known === undefined) return row[flag];
  const later = known.token !== blocks.token && (index === null || blocks.fresh(index));
  if (!later) return known.value;
  sent.delete(`${flag}:${row.id}`);
  return row[flag];
}

async function send(flag: Flag, ids: number[], value: boolean): Promise<void> {
  const token = gridBlocks().token;
  for (const id of ids) sent.set(`${flag}:${id}`, { value, token });
  const patch: InfoPatch = flag === "star" ? { star: value } : { hidden: value };
  try {
    await api().invoke("photo_set_info", { ids, patch });
  } catch (e) {
    for (const id of ids) sent.delete(`${flag}:${id}`);
    showToast({ kind: "error", message: toAppError(e).message });
  }
}

/**
 * Forgets what was sent for `ids`, after something else (undo) changed
 * their flags: the next press reads the row again.
 */
export function forgetSent(ids: readonly number[]): void {
  for (const id of ids) {
    sent.delete(`star:${id}`);
    sent.delete(`hidden:${id}`);
  }
}

/** Runs `job` after every press already made, as the presses themselves are run. */
export function queue(job: () => Promise<void> | void): Promise<void> {
  const next = chain.then(job, job);
  chain = next.catch(() => {});
  return next;
}

/**
 * F, S or H on the review screen (`selection`) or in the viewer (`viewer`).
 *
 * - In the grid and the preview: one photo selected changes that photo;
 *   several change every selected photo, all given the opposite of the
 *   current photo's value.
 * - In the viewer: the shown photo only.
 *
 * Does nothing in a view-only album, with no current photo, or while a
 * step is still waiting for its row. A mark plays over the place.
 */
export function toggleFlag(flag: Flag, where: "selection" | "viewer"): Promise<void> {
  return queue(() => {
    if (!canChange()) return;
    const grid = useGridStore.getState();
    const current = grid.current;
    if (current === null || grid.pending !== null) return;
    const row = gridBlocks().row(current.index);
    if (row === undefined || row.id !== current.id) return;
    const value = !valueOf(flag, row, current.index);
    const ids =
      where === "viewer" || grid.selection.length <= 1
        ? [current.id]
        : [current.id, ...grid.selection.filter((id) => id !== current.id)];
    playMark(flag, value, where === "viewer" ? "viewer" : "preview");
    return send(flag, ids, value);
  });
}

/** The star button on a tile: that photo only, whatever is selected. No mark. */
export function toggleTileStar(row: Row, index: number | null = null): Promise<void> {
  return queue(() => {
    if (!canChange()) return;
    return send("star", [row.id], !valueOf("star", row, index));
  });
}
