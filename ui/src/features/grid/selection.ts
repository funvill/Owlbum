// The selection model (docs/04-browsing.md, "Selection"): clicks, the
// keys that move or change the selection, Ctrl+A and Ctrl+D. The current
// photo, the anchor and the selection live in the grid store, where every
// `query_run` settles them from `kept`; these functions change them.
//
// Ids are read only from rows of the result on screen (`blocks.fresh`), never
// from the previous result's rows that are drawn until their replacements
// arrive. Anything else comes from `query_ids`. An answer is used only if
// no newer selection change was made and the result is still the same.

import { api, isAppError } from "../../api";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { gridBlocks, useGridStore, type Selected } from "./store";

/** The Ctrl and Shift state of a click. */
export interface ClickKeys {
  ctrlKey: boolean;
  shiftKey: boolean;
}

/** Moves on with every selection change, so an older change's late answer is thrown away. */
let op = 0;

const state = () => useGridStore.getState();
const token = () => state().result?.token ?? null;
const count = () => state().result?.count ?? 0;

/** The id at `index` in the result on screen, if its row is held. */
function idAt(index: number): number | undefined {
  const blocks = gridBlocks();
  return blocks.fresh(index) ? blocks.row(index)?.id : undefined;
}

/** Whether a change started as `mine` is still the newest, on the same result. */
function stillMine(mine: number, held: string): boolean {
  return mine === op && token() === held;
}

/** `query_ids` for the result on screen. An expired token starts the store's re-run. */
async function fetchIds(held: string, offset: number, limit: number): Promise<number[] | null> {
  try {
    return await api().invoke("query_ids", { token: held, offset, limit });
  } catch (e) {
    if (isAppError(e) && e.code === "query_expired" && token() === held) void state().refresh();
    return null;
  }
}

/** The ids at `first`..`last`: from held rows when all are held, else one `query_ids` call. */
function heldIds(first: number, last: number): number[] | null {
  const ids: number[] = [];
  for (let i = first; i <= last; i++) {
    const id = idAt(i);
    if (id === undefined) return null;
    ids.push(id);
  }
  return ids;
}

function commit(selected: Selected) {
  state().setSelection(selected);
}

/** The photo becomes the current photo, the whole selection and the anchor. */
function selectOnly(at: PhotoAt) {
  op++;
  commit({ current: at, anchor: at, selection: [at.id] });
}

/**
 * Selects `first`..`last` around the anchor, with `index` becoming the
 * current photo; `add` keeps the rest of the selection (Ctrl+Shift).
 * Reads held rows when it can, else asks `query_ids` once and shows the
 * range by index meanwhile.
 */
function selectRange(anchor: PhotoAt, index: number, add: boolean) {
  const mine = ++op;
  const first = Math.min(anchor.index, index);
  const last = Math.max(anchor.index, index);
  const base = add ? state().selection : [];
  const finish = (ids: number[]) => {
    const id = ids[index - first];
    if (id === undefined) return;
    commit({ current: { id, index }, anchor, selection: [...new Set([...base, ...ids])] });
  };
  const ids = heldIds(first, last);
  if (ids !== null) {
    finish(ids);
    return;
  }
  const held = token();
  if (held === null) return;
  state().setPending({ index, first, last, add, wait: "ids" });
  void fetchIds(held, first, last - first + 1).then((fetched) => {
    if (!stillMine(mine, held)) return;
    if (fetched === null) useGridStore.setState({ pending: null });
    else finish(fetched);
  });
}

/**
 * The nearest selected photo to `index` other than it: the next one after
 * it, else the one before. Reads held rows outward and falls back to one
 * `query_ids` call per direction when a row in the way is not held.
 */
async function nearestSelected(index: number, selected: Set<number>, held: string): Promise<PhotoAt | null | "stale"> {
  const n = count();
  for (const direction of [1, -1] as const) {
    let unknown = false;
    for (let i = index + direction; i >= 0 && i < n; i += direction) {
      const id = idAt(i);
      if (id === undefined) {
        unknown = true;
        break;
      }
      if (selected.has(id)) return { id, index: i };
    }
    if (!unknown) continue;
    const offset = direction === 1 ? index + 1 : 0;
    const ids = await fetchIds(held, offset, direction === 1 ? n - offset : index);
    if (ids === null) return "stale";
    const order = direction === 1 ? ids.keys() : [...ids.keys()].reverse();
    for (const k of order) {
      const id = ids[k];
      if (id !== undefined && selected.has(id)) return { id, index: offset + k };
    }
  }
  return null;
}

/** A click on a tile, with its Ctrl and Shift state (docs/04, "Clicking"). */
export function clickPhoto(at: PhotoAt, keys: ClickKeys): void {
  // A tile still showing the previous result's row is not a photo of this one.
  if (idAt(at.index) !== at.id) return;
  const { anchor, selection, current } = state();
  if (keys.shiftKey && anchor !== null) {
    selectRange(anchor, at.index, keys.ctrlKey);
    return;
  }
  if (!keys.ctrlKey) {
    selectOnly(at);
    return;
  }
  if (!selection.includes(at.id)) {
    op++;
    commit({ current: at, anchor: at, selection: [...selection, at.id] });
    return;
  }
  // Ctrl on a selected photo removes it.
  const rest = selection.filter((id) => id !== at.id);
  const mine = ++op;
  if (rest.length === 0) {
    commit({ current: null, anchor: at, selection: [] });
    return;
  }
  if (current?.id !== at.id) {
    commit({ current, anchor: at, selection: rest });
    return;
  }
  const held = token();
  if (held === null) return;
  commit({ current, anchor: at, selection: rest });
  void nearestSelected(at.index, new Set(rest), held).then((next) => {
    if (next === "stale" || !stillMine(mine, held)) return;
    commit({ current: next, anchor: at, selection: rest });
  });
}

/** A right-click: selects the photo unless it is already in the selection. */
export function menuPhoto(at: PhotoAt): void {
  if (idAt(at.index) !== at.id) return;
  if (!state().selection.includes(at.id)) selectOnly(at);
}

/** Moves the current photo to `index`, making it the whole selection; waits for its row if needed. */
function stepTo(index: number) {
  const id = idAt(index);
  if (id !== undefined) {
    selectOnly({ id, index });
    return;
  }
  op++;
  state().setPending({ index, first: index, last: index, add: false, wait: "row" });
}

/** Where the current photo is, or is going. */
function currentIndex(): number | null {
  const { pending, current } = state();
  return pending?.index ?? current?.index ?? null;
}

/**
 * Left or Right (`delta` −1 or +1): the previous or next photo, stopping
 * at the ends. With `extend` (Shift), the range from the anchor to it is
 * selected instead. With no current photo, the first photo.
 */
export function step(delta: -1 | 1, extend = false): void {
  const n = count();
  if (n === 0) return;
  const from = currentIndex();
  const index = from === null ? 0 : Math.min(n - 1, Math.max(0, from + delta));
  if (!extend || from === null) {
    if (from !== index || state().pending === null) stepTo(index);
    return;
  }
  const { anchor, current } = state();
  const base = anchor ?? current;
  if (base === null) {
    stepTo(index);
    return;
  }
  selectRange(base, index, false);
}

/** Home or End: the first or last photo. */
export function stepToEnd(end: "first" | "last"): void {
  const n = count();
  if (n > 0) stepTo(end === "first" ? 0 : n - 1);
}

/**
 * Ctrl+A: every photo shown in the result, from one `query_ids` call. The
 * current photo stays, or the first photo becomes current when there was
 * none.
 */
export async function selectAll(): Promise<void> {
  const held = token();
  const n = count();
  if (held === null || n === 0) return;
  const mine = ++op;
  const ids = await fetchIds(held, 0, n);
  if (ids === null || !stillMine(mine, held)) return;
  const { current, anchor } = state();
  const first = ids[0];
  const now = current ?? (first === undefined ? null : { id: first, index: 0 });
  commit({ current: now, anchor: anchor ?? now, selection: ids });
}

/**
 * A click on a timeline heading (docs/04-browsing.md, "Timeline view"):
 * every photo of the group, `count` tiles from `start`, with the first one
 * current. With `add` (Ctrl), the group joins the selection.
 */
export async function selectGroup(start: number, count: number, add: boolean): Promise<void> {
  const held = token();
  if (held === null || count <= 0) return;
  const mine = ++op;
  const ids = heldIds(start, start + count - 1) ?? (await fetchIds(held, start, count));
  if (ids === null || !stillMine(mine, held)) return;
  const first = ids[0];
  if (first === undefined) return;
  const at = { id: first, index: start };
  const selection = add ? [...new Set([...state().selection, ...ids])] : ids;
  commit({ current: at, anchor: at, selection });
}

/** Ctrl+D, and the details panel's close button: nothing selected, no current photo, no anchor. */
export function selectNone(): void {
  op++;
  commit({ current: null, anchor: null, selection: [] });
}

/**
 * Esc: with more than one photo selected, the selection becomes the
 * current photo, which is also the anchor. Answers whether it did anything.
 */
export function reduceToCurrent(): boolean {
  const { current, selection } = state();
  if (current === null || selection.length <= 1) return false;
  op++;
  commit({ current, anchor: current, selection: [current.id] });
  return true;
}

/**
 * Makes photo `id` the current photo and the selection when only its id
 * is known (a restored session, a pick on the map): `query_locate` finds
 * its index. A photo folded in a closed stack opens that stack and is
 * looked for once more; a photo not in the result selects nothing new.
 */
export async function selectById(id: number): Promise<void> {
  const mine = ++op;
  for (let attempt = 0; attempt < 2; attempt++) {
    const held = token();
    if (held === null) return;
    let located;
    try {
      located = await api().invoke("query_locate", { token: held, id });
    } catch {
      return;
    }
    if (!stillMine(mine, held) || located === null) return;
    if ("index" in located) {
      commit({ current: { id, index: located.index }, anchor: { id, index: located.index }, selection: [id] });
      return;
    }
    const query = state().query;
    if (query.open_stacks.includes(located.folded_in)) return;
    useGridStore.setState({ query: { ...query, open_stacks: [...query.open_stacks, located.folded_in] } });
    await state().refresh();
    if (mine !== op) return;
  }
}
