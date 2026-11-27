// Stacks (docs/05-organising.md, "Stacks" and "Selection and stacks"):
// opening and folding a stack, and the Stack similar setting. Rust folds
// the bursts and settles the selection; the page keeps only the list of
// open stack ids in its query, which is forgotten when the album closes.

import type { Row } from "../../api/bindings/Row";
import { useSettingsStore } from "../app";
import { useGridStore } from "../grid";

/**
 * The "×N" badge: opens the row's stack, or folds it. Folding while one of
 * its other members is current gives the face the selection, which Rust's
 * `kept` answers on the re-run.
 */
export function toggleStack(row: Row): void {
  if (row.stack === null) return;
  const id = row.stack.id;
  const grid = useGridStore.getState();
  const open = grid.query.open_stacks;
  grid.setQuery({ ...grid.query, open_stacks: open.includes(id) ? open.filter((s) => s !== id) : [...open, id] });
}

/** The Stack similar checkbox: re-runs the query and remembers the choice as the `stack_similar` setting. */
export function setStackSimilar(on: boolean): void {
  const grid = useGridStore.getState();
  if (grid.query.stack === on) return;
  grid.setQuery({ ...grid.query, stack: on });
  void useSettingsStore.getState().update({ stack_similar: on });
}
