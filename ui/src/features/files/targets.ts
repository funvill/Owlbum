// Which photos a file action acts on (docs/06-editing-and-file-actions.md,
// "The photo menu", "Targets"): the selected photos that are shown, in the
// order of the result, else the current photo.

import { BLOCK, gridBlocks, useGridStore } from "../grid";

/**
 * `ids` in the order of the result, as far as the rows the grid holds
 * tell; ids of rows not held keep their order after them.
 */
export function inResultOrder(ids: readonly number[]): number[] {
  const blocks = gridBlocks();
  const wanted = new Set(ids);
  const at = new Map<number, number>();
  for (const b of blocks.held()) {
    for (let i = b * BLOCK; i < (b + 1) * BLOCK; i++) {
      const row = blocks.row(i);
      if (row !== undefined && wanted.has(row.id)) at.set(row.id, i);
    }
  }
  const known = ids.filter((id) => at.has(id)).sort((a, b) => at.get(a)! - at.get(b)!);
  return [...known, ...ids.filter((id) => !at.has(id))];
}

/** The selected photos in the order of the result, else the current photo, else none. */
export function selectedTargets(): number[] {
  const { selection, current } = useGridStore.getState();
  if (selection.length > 0) return inResultOrder(selection);
  return current === null ? [] : [current.id];
}

/** A drag from `id`: the selection when it is selected, else that photo only. */
export function dragTargets(id: number): number[] {
  const { selection } = useGridStore.getState();
  return selection.includes(id) ? inResultOrder(selection) : [id];
}
