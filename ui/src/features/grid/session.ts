// The album session in the page's form (docs/04-browsing.md, "Reopening
// where I was"): the query's own fields that belong to one album, the
// current photo and the scroll index. Rust stores it against the folder
// and keeps the current photo as its path.

import type { Query } from "../../api/bindings/QueryShape";
import { defaultQuery } from "../../api/query";

/**
 * What `app_session_save` takes and `album_open` answers. Every field of the
 * `Query` that belongs to one album travels under its own name, so a field
 * added to the `Query` later (the search text, for one) is saved and
 * restored with no change here.
 */
export type SavedSession = Partial<Query> & {
  /** The current photo; null when nothing is current or it is gone. */
  selected_id: number | null;
  /** The index of the first photo in view. */
  scroll?: number;
};

/**
 * Query fields that are not part of one album's session: the open stacks
 * and whether the filter panel asks for counts are forgotten on close, and
 * the timeline unit and stacking are settings for every album.
 */
const NOT_KEPT: readonly string[] = [
  "open_stacks",
  "want_facets",
  "timeline_unit",
  "stack",
];

/** The session to save for `query`, the current photo and the first photo in view. */
export function sessionOf(
  query: Query,
  current: number | null,
  top: number,
): SavedSession {
  const kept = Object.fromEntries(
    Object.entries(query).filter(([k]) => !NOT_KEPT.includes(k)),
  ) as Partial<Query>;
  return { ...kept, selected_id: current, scroll: top };
}

/**
 * `query` with a restored session's fields put over it. Only fields the
 * `Query` has are taken, and only when their kind matches; Rust has already
 * dropped values it does not allow. The map is not built yet (#36): a saved
 * map view shows the grid.
 */
export function withSession(query: Query, saved: SavedSession): Query {
  const out: Record<string, unknown> = { ...query };
  const known = defaultQuery() as unknown as Record<string, unknown>;
  for (const [k, v] of Object.entries(saved)) {
    if (NOT_KEPT.includes(k) || !(k in known) || v === undefined) continue;
    const want = known[k];
    // A field whose default is null (the search) takes whatever Rust let through.
    const sameKind = want === null || typeof v === typeof want;
    if (sameKind) out[k] = v;
  }
  const q = out as unknown as Query;
  return q.view === "map" ? { ...q, view: "grid" } : q;
}
