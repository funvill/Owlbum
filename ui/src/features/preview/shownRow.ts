// The row of the page's one current photo, as the preview and the viewer
// show it (docs/04-browsing.md, "Which picture is shown", rule 4).

import { useCallback, useState, useSyncExternalStore } from "react";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import type { Row } from "../../api/bindings/Row";
import { gridBlocks, useGridStore, type Pending } from "../grid";

export interface ShownRow {
  current: PhotoAt | null;
  pending: Pending | null;
  /**
   * The current photo's row, or undefined while it is not held. Only a row
   * that is this photo's: the index may be an old result's, and a step
   * waiting for its row shows a placeholder, never the photo it left.
   */
  row: Row | undefined;
}

/** Follows the grid store and its blocks; keeps the last row seen for the current photo. */
export function useShownRow(): ShownRow {
  const current = useGridStore((s) => s.current);
  const pending = useGridStore((s) => s.pending);
  useSyncExternalStore(
    useCallback((listener: () => void) => gridBlocks().subscribe(listener), []),
    () => gridBlocks().version,
  );
  // A re-run that moves the photo, or a scroll that lets its block go, must not blank it.
  const [held, setHeld] = useState<Row | null>(null);
  const candidate = current === null || pending !== null ? undefined : gridBlocks().row(current.index);
  const found = candidate !== undefined && candidate.id === current?.id ? candidate : undefined;
  if (found !== undefined && found !== held) setHeld(found);
  const row = found ?? (pending === null && held !== null && held.id === current?.id ? held : undefined);
  return { current, pending, row };
}
