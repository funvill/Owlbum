// What changes on the map when a new result arrives (docs/04-browsing.md,
// "What the map holds"): markers whose id, place and thumbnail are
// unchanged are kept; the rest are added or removed.

import type { MapPoint } from "../../api/bindings/MapPoint";

/** The markers to add and the ids whose markers go. */
export interface PointChange {
  add: MapPoint[];
  remove: number[];
}

/** Whether a marker drawn for `a` still serves `b`. */
export function samePoint(a: MapPoint, b: MapPoint): boolean {
  return a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3] && a[4] === b[4];
}

/**
 * The change from the markers `shown` (by id) to the points `next`. A
 * point whose marker no longer serves it is removed and added again.
 */
export function diffPoints(shown: ReadonlyMap<number, MapPoint>, next: readonly MapPoint[]): PointChange {
  const wanted = new Map<number, MapPoint>();
  for (const p of next) wanted.set(p[0], p);
  const add: MapPoint[] = [];
  const remove: number[] = [];
  for (const [id, old] of shown) {
    const p = wanted.get(id);
    if (p === undefined || !samePoint(old, p)) remove.push(id);
  }
  for (const p of wanted.values()) {
    const old = shown.get(p[0]);
    if (old === undefined || !samePoint(old, p)) add.push(p);
  }
  return { add, remove };
}
