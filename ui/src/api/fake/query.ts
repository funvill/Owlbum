import type { Keep } from "../bindings/Keep";
import type { Kept } from "../bindings/Kept";
import type { MapPoint } from "../bindings/MapPoint";
import type { MediaKind } from "../bindings/MediaKind";
import type { PhotoAt } from "../bindings/PhotoAt";
import type { QueryCommands } from "../bindings/query";
import type { Query } from "../bindings/QueryShape";
import type { Row } from "../bindings/Row";
import type { RowStack } from "../bindings/RowStack";
import type { EditPlan } from "../bindings/EditPlan";
import type { AppError } from "../bindings/AppError";
import type { Group } from "../bindings/Group";
import type { GroupUnit } from "../bindings/GroupUnit";
import type { AreaHandlers } from "./types";
import { chips, folderOf, normalised, pass, validate } from "./queryFilter";
import { isSearch, parseBox, textMatches } from "./querySearch";

/** How many results are kept, as in Rust. An older token answers `query_expired`. */
export const TOKENS_KEPT = 4;

/** The most rows one `query_window` answers, as in Rust. */
export const WINDOW_MAX = 500;

/** A photo of the fake album. Only `id` and `rel_path` are required. */
export interface FakePhoto {
  id: number;
  /** Relative to the album root, `/` separators. */
  rel_path: string;
  /** The effective date, `YYYY-MM-DDTHH:MM:SS`; absent or null when there is none. */
  date?: string | null;
  hidden?: boolean;
  star?: boolean;
  kind?: MediaKind;
  width?: number;
  height?: number;
  duration_ms?: number | null;
  score?: number | null;
  /** "Interesting", 0 to 1; absent or null when the photo is not analysed. */
  interesting?: number | null;
  /**
   * The Main colour: the bucket's place in the order red (0) to neutral (8),
   * and the lightness; absent or null when there is no average colour.
   */
  colour?: { bucket: number; lightness: number } | null;
  /** The plan for the stored edit; null (no edit) when absent. */
  edit_plan?: EditPlan | null;
  /** Has a typed place or GPS. */
  has_location?: boolean;
  /** The effective place, `[lat, lon]`, for `query_map`; absent or null when there is none. */
  place?: [number, number] | null;
  /** The names of the people on it. */
  people?: string[];
  /** Its keyword labels. */
  keywords?: string[];
  /** Its object labels. */
  objects?: string[];
  /** At least one quality flag is set. */
  low_quality?: boolean;
  /** In a burst. */
  burst?: boolean;
  /** The id of its burst, which is the id of the stack it folds into; implies `burst`. */
  burst_id?: number;
  /** The keeper of its burst. */
  keeper?: boolean;
}

/** The stacks of one result, as Rust keeps them (owlbum_core::query::stack). */
interface Stacks {
  /** Per tile index, its stack as the row shows it. */
  rows: Map<number, RowStack>;
  /** A photo folded inside a closed stack: the stack's id and its face tile. */
  folded: Map<number, { stack: number; face: PhotoAt }>;
  /** The face of each stack, to its other members, for rule 2. */
  faces: Map<number, number[]>;
}

function noStacks(): Stacks {
  return { rows: new Map(), folded: new Map(), faces: new Map() };
}

/** Folds `sorted` as Rust does: filter, sort, then fold. */
function fold(sorted: FakePhoto[], open: number[]): { tiles: number[]; stacks: Stacks } {
  const bursts = new Map<number, FakePhoto[]>();
  for (const p of sorted) {
    if (p.burst_id === undefined) continue;
    const members = bursts.get(p.burst_id) ?? [];
    members.push(p);
    bursts.set(p.burst_id, members);
  }
  const faceOf = new Map<number, FakePhoto>();
  for (const [id, members] of bursts) {
    if (members.length < 2) continue;
    faceOf.set(id, members.find((m) => m.star === true) ?? members.find((m) => m.keeper === true) ?? members[0]!);
  }
  const tiles: number[] = [];
  const stacks = noStacks();
  const later: { stack: number; id: number }[] = [];
  for (const p of sorted) {
    const id = p.burst_id;
    const face = id === undefined ? undefined : faceOf.get(id);
    if (id === undefined || face === undefined) {
      tiles.push(p.id);
      continue;
    }
    if (face !== p) {
      if (!open.includes(id)) later.push({ stack: id, id: p.id });
      continue;
    }
    const members = [face, ...bursts.get(id)!.filter((m) => m !== face)];
    stacks.faces.set(
      face.id,
      members.slice(1).map((m) => m.id),
    );
    const isOpen = open.includes(id);
    (isOpen ? members : [face]).forEach((m, i) => {
      stacks.rows.set(tiles.length, { id, size: members.length, open: isOpen, face: i === 0, place: i + 1 });
      tiles.push(m.id);
    });
  }
  for (const { stack, id } of later) {
    const face = faceOf.get(stack)!;
    stacks.folded.set(id, { stack, face: { id: face.id, index: tiles.indexOf(face.id) } });
  }
  return { tiles, stacks };
}

/** The fake album behind the `query_` commands, with what a test needs to change it. */
export interface QueryFake {
  readonly handlers: AreaHandlers<QueryCommands>;
  /** Replaces the album's photos and moves the revision on; returns the new revision. */
  change(edit: (photos: FakePhoto[]) => FakePhoto[]): number;
}

function appError(code: AppError["code"], message: string): AppError {
  return { code, message };
}

/** Numbers every token, so no fake backend's token names another's result. */
let nextToken = 1;

function fileName(relPath: string): string {
  return relPath.slice(relPath.lastIndexOf("/") + 1);
}

const folder = folderOf;

/** Splits text into runs of ASCII digits and runs of everything else. */
function runs(text: string): string[] {
  return text.match(/[0-9]+|[^0-9]+/g) ?? [];
}

/**
 * The natural comparison of docs/03-media-and-scanning.md, "Order", steps 2
 * to 5: digit runs by number (fewer leading zeros first when equal), text
 * runs by code point after lower-casing, a digit run before a text run.
 */
export function naturalCompare(a: string, b: string): number {
  const ra = runs(a);
  const rb = runs(b);
  for (let i = 0; i < Math.min(ra.length, rb.length); i++) {
    const x = ra[i]!;
    const y = rb[i]!;
    const dx = /^[0-9]/.test(x);
    const dy = /^[0-9]/.test(y);
    if (dx !== dy) return dx ? -1 : 1;
    if (dx) {
      const nx = x.replace(/^0+/, "");
      const ny = y.replace(/^0+/, "");
      if (nx.length !== ny.length) return nx.length - ny.length;
      if (nx !== ny) return nx < ny ? -1 : 1;
      if (x.length !== y.length) return x.length - y.length;
    } else {
      const lx = [...x.toLowerCase()];
      const ly = [...y.toLowerCase()];
      for (let j = 0; j < Math.min(lx.length, ly.length); j++) {
        const cx = lx[j]!.codePointAt(0)!;
        const cy = ly[j]!.codePointAt(0)!;
        if (cx !== cy) return cx - cy;
      }
      if (lx.length !== ly.length) return lx.length - ly.length;
    }
  }
  return ra.length - rb.length;
}

/** The name order: file name, then folder, then id. */
function nameOrder(a: FakePhoto, b: FakePhoto): number {
  return (
    naturalCompare(fileName(a.rel_path), fileName(b.rel_path)) ||
    naturalCompare(folder(a.rel_path), folder(b.rel_path)) ||
    a.id - b.id
  );
}

/** Date taken, undated last, ties by the name order. */
function dateOrder(a: FakePhoto, b: FakePhoto): number {
  const da = a.date ?? null;
  const db = b.date ?? null;
  if (da !== db) {
    if (da === null) return 1;
    if (db === null) return -1;
    return da < db ? -1 : 1;
  }
  return nameOrder(a, b);
}

/** Sorts by `key` (absent keys last, then ascending), ties by the name order. */
function keyOrder(key: (p: FakePhoto) => number[] | null): (a: FakePhoto, b: FakePhoto) => number {
  return (a, b) => {
    const ka = key(a);
    const kb = key(b);
    if (ka === null || kb === null) {
      if (ka !== kb) return ka === null ? 1 : -1;
      return nameOrder(a, b);
    }
    for (let i = 0; i < ka.length; i++) {
      if (ka[i] !== kb[i]) return ka[i]! - kb[i]!;
    }
    return nameOrder(a, b);
  };
}

/** The five sorts of docs/05-organising.md, "Sorting", as Rust runs them. */
const ORDERS: Record<Query["sort"], (a: FakePhoto, b: FakePhoto) => number> = {
  date: dateOrder,
  name: nameOrder,
  score: keyOrder((p) => (p.score === undefined || p.score === null ? null : [-p.score])),
  interesting: keyOrder((p) => (p.interesting === undefined || p.interesting === null ? null : [-p.interesting])),
  colour: keyOrder((p) => (p.colour === undefined || p.colour === null ? null : [p.colour.bucket, p.colour.lightness])),
};

/** Days from 1970-01-01 of a `YYYY-MM-DD…` date, read as written. */
function dayNumber(date: string): number {
  return Date.UTC(Number(date.slice(0, 4)), Number(date.slice(5, 7)) - 1, Number(date.slice(8, 10))) / 86_400_000;
}

/** The timeline's groups of `shown`, in date order, as Rust makes them (owlbum_core::query::timeline). */
function timelineGroups(
  shown: FakePhoto[],
  unit: Query["timeline_unit"],
  weight: (i: number) => number = () => 1,
): { groups: Group[]; unit: GroupUnit } {
  const dated = shown.filter((p) => typeof p.date === "string");
  let used: GroupUnit = unit === "month" ? "month" : "day";
  if (unit === "auto" && dated.length > 0 && dayNumber(dated.at(-1)!.date!) - dayNumber(dated[0]!.date!) > 90) {
    used = "month";
  }
  const groups: Group[] = [];
  shown.forEach((p, i) => {
    const key = typeof p.date === "string" ? p.date.slice(0, used === "day" ? 10 : 7) : "none";
    const last = groups.at(-1);
    if (last?.key === key) {
      last.count += 1;
      last.photos += weight(i);
    } else groups.push({ key, start: i, count: 1, photos: weight(i) });
  });
  return { groups, unit: used };
}

/** Settles `keep` as Rust does (owlbum_core::query::keep). */
function settle(
  keep: Keep,
  old: { tiles: number[]; stacks: Stacks } | undefined,
  tiles: number[],
  stacks: Stacks = noStacks(),
): Kept {
  if (tiles.length === 0) return { current: null, anchor: null, selection: [], view: null };
  const place = new Map(tiles.map((id, index) => [id, index]));
  const find = (id: number): PhotoAt | null => {
    const index = place.get(id);
    return index === undefined ? null : { id, index };
  };
  const inItsPlace = (gone: PhotoAt): PhotoAt => {
    let k = gone.index;
    if (old !== undefined) {
      let at = old.tiles[gone.index] === gone.id ? gone.index : old.tiles.indexOf(gone.id);
      if (at < 0) at = Math.min(gone.index, old.tiles.length);
      k = old.tiles.slice(0, at).filter((t) => place.has(t)).length;
    }
    const at = Math.min(k, tiles.length - 1);
    return { id: tiles[at]!, index: at };
  };
  // Rule 2: the face of the stack it is folded in now, or the new face of the stack it was the face of.
  const replacement = (id: number): PhotoAt | null => {
    const folded = stacks.folded.get(id);
    if (folded !== undefined) return folded.face;
    for (const m of old?.stacks.faces.get(id) ?? []) {
      const at = stacks.folded.get(m)?.face ?? find(m);
      if (at === null) continue;
      if (stacks.rows.get(at.index)?.face === false) {
        const face = [...stacks.faces.keys()].find((f) => stacks.faces.get(f)!.includes(at.id));
        if (face !== undefined) return find(face);
      }
      return at;
    }
    return null;
  };
  const replaced = (gone: PhotoAt): PhotoAt => find(gone.id) ?? replacement(gone.id) ?? inItsPlace(gone);
  const current = keep.current === null ? null : replaced(keep.current);
  const anchor = (keep.anchor_id === null ? null : find(keep.anchor_id)) ?? current;
  // A selected photo now folded in a closed stack hands its place to the face.
  const selection = [
    ...new Set(
      keep.selection.flatMap((id) => {
        if (place.has(id)) return [id];
        const folded = stacks.folded.get(id);
        return folded === undefined ? [] : [folded.face.id];
      }),
    ),
  ];
  if (current !== null && !selection.includes(current.id)) selection.push(current.id);
  const view = keep.view === null ? null : replaced(keep.view);
  return { current, anchor, selection, view };
}

function row(id: number, p: FakePhoto | undefined): Row {
  const width = p?.width ?? 0;
  const height = p?.height ?? 0;
  return {
    id,
    rel_path: p?.rel_path ?? "",
    file_name: p === undefined ? "" : fileName(p.rel_path),
    thumb_key: p === undefined ? "" : `tk-${id}`,
    aspect: width > 0 && height > 0 ? width / height : 1,
    kind: p?.kind ?? "image",
    duration_ms: p?.duration_ms ?? null,
    star: p?.star ?? false,
    hidden: p?.hidden ?? false,
    score: p?.score ?? null,
    stack: null,
    edit_plan: p?.edit_plan ?? null,
  };
}

/**
 * A stateful fake of the query engine over `photos`, behaving like Rust:
 * the grid, the timeline and the map in any of the five sorts, with every filter, its chips
 * and facets (queryFilter.ts), the
 * last four results kept under tokens, rows read as they are now, and
 * `keep` settled by the four rules. What Rust does not run yet answers
 * `bad_query` here too.
 */
export function createQueryFake(photos: FakePhoto[] = []): QueryFake {
  let list = [...photos];
  let revision = 0;
  const held: { token: string; tiles: number[]; stacks: Stacks; filter: Query["filter"]; text: Set<number> | null }[] =
    [];

  const heldOf = (token: string) => {
    const h = held.find((x) => x.token === token);
    if (h === undefined) throw appError("query_expired", "The list of photos changed. Showing it again.");
    return h;
  };
  const tilesOf = (token: string): number[] => heldOf(token).tiles;

  const handlers: AreaHandlers<QueryCommands> = {
    query_run: ({ query, keep }) => {
      const filter = normalised(query.filter);
      validate(filter);
      const old = keep === null ? undefined : held.find((h) => h.token === keep.token);
      const text = textMatches(list, query.search);
      const passed = pass(list, filter, "", query.want_facets, text);
      const shown = passed.shown;
      const timeline = query.view === "timeline";
      shown.sort(ORDERS[timeline ? "date" : query.sort]);
      // The map gives every photo its own marker, so it never folds.
      const { tiles, stacks } =
        query.stack && query.view !== "map"
          ? fold(shown, query.open_stacks)
          : { tiles: shown.map((p) => p.id), stacks: noStacks() };
      const token = `q${nextToken++}`;
      held.push({ token, tiles, stacks, filter, text });
      while (held.length > TOKENS_KEPT) held.shift();
      const chipList = chips(filter, "");
      const byId = new Map(shown.map((p) => [p.id, p]));
      const tilePhotos = tiles.map((id) => byId.get(id)!);
      const weight = (i: number) => {
        const s = stacks.rows.get(i);
        return s !== undefined && s.face && !s.open ? s.size : 1;
      };
      return {
        token,
        revision,
        count: tiles.length,
        photos: shown.length,
        total: passed.total,
        album_photos: list.length,
        filtered: Object.keys(filter).length > 0 || isSearch(query.search),
        panel_count: chipList.length,
        chips: chipList,
        kept: keep === null ? null : settle(keep, old, tiles, stacks),
        ...(timeline ? timelineGroups(tilePhotos, query.timeline_unit, weight) : {}),
        ...(passed.facets === undefined ? {} : { facets: passed.facets }),
      };
    },
    query_window: ({ token, offset, limit }) => {
      const tiles = tilesOf(token);
      const byId = new Map(list.map((p) => [p.id, p]));
      const { stacks } = heldOf(token);
      return tiles
        .slice(offset, offset + Math.min(limit, WINDOW_MAX))
        .map((id, i) => ({ ...row(id, byId.get(id)), stack: stacks.rows.get(offset + i) ?? null }));
    },
    query_facets: ({ token }) => pass(list, heldOf(token).filter, "", true, heldOf(token).text).facets!,
    query_parse: ({ text, enter, filter }) => parseBox(list, text, enter, normalised(filter ?? {})),
    query_ids: ({ token, offset, limit }) => tilesOf(token).slice(offset, offset + limit),
    query_locate: ({ token, id }) => {
      const index = tilesOf(token).indexOf(id);
      if (index >= 0) return { index };
      const folded = heldOf(token).stacks.folded.get(id);
      return folded === undefined ? null : { folded_in: folded.stack };
    },
    query_map: ({ token }) => {
      const tiles = tilesOf(token);
      const byId = new Map(list.map((p) => [p.id, p]));
      const points: MapPoint[] = [];
      for (const id of tiles) {
        const p = byId.get(id);
        const place = p?.place;
        if (p === undefined || place === undefined || place === null) continue;
        const [lat, lon] = place;
        if (Math.abs(lat) > 90 || Math.abs(lon) > 180) continue;
        points.push([id, lat, lon, `tk-${id}`, fileName(p.rel_path), p.edit_plan ?? null]);
      }
      return { points, unlocated: tiles.length - points.length };
    },
  };

  return {
    handlers,
    change(edit) {
      list = edit([...list]);
      revision += 1;
      return revision;
    },
  };
}

/**
 * The fake backend's `query_` commands over an empty album. `createFakeApi`
 * makes a fresh set for each backend, so no two share results.
 */
export const queryHandlers: AreaHandlers<QueryCommands> = createQueryFake().handlers;
