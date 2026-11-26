// The grid's store: drives the query client for the open album
// (docs/04-browsing.md, "How the page holds a result"), holds the current
// photo, anchor and selection that every `query_run` settles, and the
// thumbnail size. Rows live in the block store, outside React state.

import { create } from "zustand";
import { api, isAppError, toAppError } from "../../api";
import type { AppError, Unlisten } from "../../api";
import type { AlbumChanged } from "../../api/bindings/AlbumChanged";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import type { Query } from "../../api/bindings/QueryShape";
import type { QueryResult } from "../../api/bindings/QueryResult";
import { QueryClient, defaultQuery, rerunFor } from "../../api/query";
import { useSettingsStore } from "../app";
import { BlockStore } from "./blocks";
import { clampThumbSize, THUMB_DEFAULT } from "./layout";

/** Re-runs after `album_changed` are at most this often while a scan or tagging runs… */
export const RERUN_BUSY_MS = 1000;
/** …and this often otherwise. */
export const RERUN_IDLE_MS = 100;
/** The thumbnail size is saved this long after the slider last moved. */
export const SAVE_SIZE_MS = 300;

/** The current photo, the anchor and the selection. */
export interface Selected {
  current: PhotoAt | null;
  anchor: PhotoAt | null;
  /** Ids, in no particular order. */
  selection: number[];
}

/**
 * A selection change waiting on the backend: a step onto a row not fetched
 * yet, or a range whose ids come from `query_ids`. Until it settles, the
 * grid outlines `index` as the current photo and shows `first`..`last`
 * selected by index. The committed `current`, `anchor` and `selection`
 * stay as they were, and are what the next `query_run` keeps; a new result
 * drops whatever is pending.
 */
export interface Pending {
  /** Where the current photo is going. */
  index: number;
  first: number;
  last: number;
  /** Whether the committed selection still shows beside the range (Ctrl+Shift). */
  add: boolean;
  /** `row`: a plain step, settled when the row at `index` arrives. `ids`: a range, settled by `query_ids`. */
  wait: "row" | "ids";
}

/**
 * After a re-run for `album_changed`: the tile the grid's place is kept by was
 * at `from` and is now at `to`, so the grid scrolls by the rows between them
 * and the photos in view stay where they were while rows arrive.
 */
export interface Follow {
  from: number;
  to: number;
  /** Moves on with each one. */
  seq: number;
}

/** A request for the grid to scroll, after a change the user made. */
export interface ScrollRequest {
  to: "current" | "top";
  /** Moves on with each request. */
  seq: number;
}

export interface GridOptions {
  /**
   * Whether a scan or tagging is running, which spaces re-runs a second
   * apart. By default nothing is running.
   */
  busy?: () => boolean;
}

export interface GridState extends Selected {
  /** The newest result, once one arrived. */
  result: QueryResult | null;
  /** The query of the newest call. */
  query: Query;
  /** Why the last run failed, if it did. */
  error: AppError | null;
  /** The chosen thumbnail size, 96 to 320. */
  thumbSize: number;
  scroll: ScrollRequest | null;
  /** Where the grid's place moved in the newest result; see `Follow`. */
  follow: Follow | null;
  /** A selection change waiting on the backend; see `Pending`. */
  pending: Pending | null;
  /** Moves on when the formats from `app_formats` arrive. */
  formats: number;
  /** Runs the query and follows `album_changed`. Calling it again does nothing. */
  start(options?: GridOptions): Promise<void>;
  /** Stops following the album and forgets its result (the album closed). */
  stop(): void;
  /** Runs a new query; changes within one animation frame make one call. */
  setQuery(query: Query): void;
  /** Sets the thumbnail size at once and saves it 300 ms after the last change. */
  setThumbSize(size: number): void;
  /** Sets the current photo, anchor and selection, dropping anything pending. */
  setSelection(selected: Selected): void;
  /** Marks a selection change as waiting on the backend, and scrolls to where it goes. */
  setPending(pending: Pending): void;
  /**
   * Runs the query again now, keeping the selection (as after
   * `album_changed`). For a `photo_not_found` answer, which this settles.
   */
  refresh(): Promise<void>;
}

type Cause = "first" | "user" | "change";

interface Session {
  client: QueryClient;
  ready: Promise<void>;
  unlisten: Unlisten | null;
  /** Stops watching the block store for a pending step's row. */
  unwatch: () => void;
  busy: () => boolean;
  /** Why the newest call was made. */
  cause: Cause;
  /** The tile the newest call asked to keep the grid's place by. */
  view: PhotoAt | null;
  lastRerun: number;
  rerunTimer: ReturnType<typeof setTimeout> | undefined;
  frame: number | undefined;
  /**
   * Whether the first photo is still to become current when rows arrive: an
   * album opens with its first photo current (#119), unless a photo is
   * current already (kept, or picked before the rows came).
   */
  pickFirst: boolean;
}

let session: Session | null = null;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
/** Whether the size was chosen on the page, so the stored setting no longer applies. */
let sizeChosen = false;
let blocks = newBlocks();

/** Answers the tile the grid's place is kept by; see `setGridView`. */
let gridView: () => PhotoAt | null = () => null;

/**
 * Tells the store how to find the tile the grid's place is kept by while rows
 * arrive: the current photo when it is in view, else the first tile in view,
 * or null (docs/04-browsing.md, "When the result changes"). It is asked when
 * a re-run for `album_changed` is made, so it sees the rows held then.
 */
export function setGridView(view: () => PhotoAt | null): void {
  gridView = view;
}

/** Starts a re-run, spaced as re-runs after `album_changed` are; set by the store. */
let rerunExpired: () => void = () => {};

function newBlocks(): BlockStore {
  return new BlockStore(
    async (token, offset, limit) => {
      if (session === null) throw new Error("No album is open.");
      try {
        return await api().invoke("query_window", { token, offset, limit });
      } catch (e) {
        // The token expired: run the query again. The new result's rows are
        // then asked for again, which repeats the call. The re-run is spaced
        // like any other, so a backend that keeps expiring cannot spin.
        if (isAppError(e) && e.code === "query_expired" && token === blocks.token) rerunExpired();
        throw e;
      }
    },
    (e) => {
      if (!(isAppError(e) && e.code === "query_expired")) useGridStore.setState({ error: toAppError(e) });
    },
  );
}

/** File extension (".gif") to the label of an animated format, from `app_formats`. */
let animatedLabels = new Map<string, string>();

/** The label of the animated format of `fileName`, such as "GIF"; null when none is known. */
export function animatedLabel(fileName: string): string | null {
  const dot = fileName.lastIndexOf(".");
  return dot < 0 ? null : (animatedLabels.get(fileName.slice(dot).toLowerCase()) ?? null);
}

async function loadFormats(): Promise<void> {
  try {
    const formats = await api().invoke("app_formats");
    animatedLabels = new Map(
      formats.filter((f) => f.animated).flatMap((f) => f.extensions.map((e) => [e.toLowerCase(), f.label] as const)),
    );
    useGridStore.setState((s) => ({ formats: s.formats + 1 }));
  } catch {
    // Without the formats an animated picture has no pill; nothing else changes.
  }
}

/** The rows of the current result. */
export function gridBlocks(): BlockStore {
  return blocks;
}

const empty: Selected = { current: null, anchor: null, selection: [] };

export const useGridStore = create<GridState>()((set, get) => {
  const nextScroll = (to: ScrollRequest["to"]): ScrollRequest => ({ to, seq: (get().scroll?.seq ?? 0) + 1 });

  const applyResult = (s: Session, result: QueryResult) => {
    if (session !== s) return;
    blocks.reset(result.token, result.count);
    const kept = result.kept ?? null;
    const selected: Partial<Selected> =
      kept === null ? {} : { current: kept.current, anchor: kept.anchor, selection: kept.selection };
    const current = kept === null ? get().current : kept.current;
    const scroll = s.cause === "user" ? nextScroll(current === null ? "top" : "current") : get().scroll;
    const to = result.kept?.view ?? null;
    const from = s.view;
    const follow =
      s.cause === "change" && from !== null && to !== null && to.index !== from.index
        ? { from: from.index, to: to.index, seq: (get().follow?.seq ?? 0) + 1 }
        : get().follow;
    set({ result, error: null, ...selected, scroll, follow, pending: null });
    if (s.pickFirst) {
      if (current !== null) s.pickFirst = false;
      else if (result.count > 0) {
        set({ pending: { index: 0, first: 0, last: 0, add: false, wait: "row" } });
        settleStep();
      }
    }
  };

  const run = async (s: Session, cause: Cause) => {
    s.cause = cause;
    try {
      await s.client.run(get().query);
    } catch (e) {
      if (session === s) set({ error: toAppError(e) });
    }
  };

  const rerun = (s: Session) => {
    if (s.rerunTimer !== undefined) return;
    const fire = () => {
      s.rerunTimer = undefined;
      s.lastRerun = Date.now();
      void run(s, "change");
    };
    const wait = s.lastRerun + (s.busy() ? RERUN_BUSY_MS : RERUN_IDLE_MS) - Date.now();
    if (wait <= 0) fire();
    else s.rerunTimer = setTimeout(fire, wait);
  };

  const changed = (s: Session, change: AlbumChanged) => {
    const held = s.client.result;
    if (session !== s || held === null || change.revision <= held.revision) return;
    switch (rerunFor(change.reason, get().query)) {
      case "run":
        rerun(s);
        break;
      case "rows":
        blocks.refresh();
        break;
      case "none":
        break;
    }
  };

  /** A pending step settles when its row arrives. */
  const settleStep = () => {
    const p = get().pending;
    if (p === null || p.wait !== "row" || !blocks.fresh(p.index)) return;
    const row = blocks.row(p.index);
    if (row === undefined) return;
    const at = { id: row.id, index: p.index };
    if (session !== null) session.pickFirst = false;
    set({ current: at, anchor: at, selection: [row.id], pending: null });
  };

  rerunExpired = () => {
    if (session !== null) rerun(session);
  };

  return {
    result: null,
    query: defaultQuery(),
    error: null,
    thumbSize: THUMB_DEFAULT,
    scroll: null,
    follow: null,
    formats: 0,
    pending: null,
    ...empty,

    start(options = {}) {
      if (session !== null) return session.ready;
      const s: Session = {
        client: new QueryClient(api(), {
          keep: () => {
            const { current, anchor, selection } = get();
            // Only a re-run for album_changed keeps the place by a tile.
            s.view = s.cause === "change" ? gridView() : null;
            return { current, anchor_id: anchor?.id ?? null, selection, view: s.view };
          },
          onResult: (result) => applyResult(s, result),
        }),
        ready: Promise.resolve(),
        unlisten: null,
        unwatch: blocks.subscribe(settleStep),
        busy: options.busy ?? (() => false),
        cause: "first",
        view: null,
        lastRerun: Number.NEGATIVE_INFINITY,
        rerunTimer: undefined,
        frame: undefined,
        pickFirst: true,
      };
      session = s;
      s.ready = (async () => {
        const settings = useSettingsStore.getState();
        if (settings.settings === null) await settings.load();
        if (session !== s) return;
        if (animatedLabels.size === 0) await loadFormats();
        if (session !== s) return;
        if (!sizeChosen) set({ thumbSize: clampThumbSize(useSettingsStore.getState().settings?.thumb_size) });
        const unlisten = await api().listen("album_changed", (change) => changed(s, change));
        if (session !== s) {
          unlisten();
          return;
        }
        s.unlisten = unlisten;
        await run(s, "first");
      })();
      return s.ready;
    },

    stop() {
      const s = session;
      session = null;
      if (s !== null) {
        s.unlisten?.();
        s.unwatch();
        clearTimeout(s.rerunTimer);
        if (s.frame !== undefined) cancelAnimationFrame(s.frame);
      }
      blocks.clear();
      gridView = () => null;
      set({ result: null, error: null, query: defaultQuery(), scroll: null, follow: null, pending: null, ...empty });
    },

    setQuery(query) {
      set({ query });
      const s = session;
      if (s === null || s.frame !== undefined) return;
      s.frame = requestAnimationFrame(() => {
        s.frame = undefined;
        if (session === s) void run(s, "user");
      });
    },

    setThumbSize(size) {
      const thumbSize = clampThumbSize(size);
      sizeChosen = true;
      set({ thumbSize });
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        saveTimer = undefined;
        void useSettingsStore.getState().update({ thumb_size: thumbSize });
      }, SAVE_SIZE_MS);
    },

    setSelection(selected) {
      if (session !== null) session.pickFirst = false;
      const before = get().current;
      const moved =
        selected.current !== null &&
        (before === null || before.id !== selected.current.id || before.index !== selected.current.index);
      set({ ...selected, pending: null, scroll: moved ? nextScroll("current") : get().scroll });
    },

    setPending(pending) {
      if (session !== null) session.pickFirst = false;
      set({ pending, scroll: nextScroll("current") });
      settleStep();
    },

    async refresh() {
      const s = session;
      if (s === null) return;
      await s.ready;
      if (session === s) await run(s, "change");
    },
  };
});

/** Stops the grid and puts its store back as it started. For tests and closing the page. */
export function resetGrid(): void {
  useGridStore.getState().stop();
  clearTimeout(saveTimer);
  saveTimer = undefined;
  sizeChosen = false;
  animatedLabels = new Map();
  blocks = newBlocks();
  useGridStore.setState({ thumbSize: THUMB_DEFAULT });
}
