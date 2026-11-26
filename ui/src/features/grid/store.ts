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
  /** Runs the query and follows `album_changed`. Calling it again does nothing. */
  start(options?: GridOptions): Promise<void>;
  /** Stops following the album and forgets its result (the album closed). */
  stop(): void;
  /** Runs a new query; changes within one animation frame make one call. */
  setQuery(query: Query): void;
  /** Sets the thumbnail size at once and saves it 300 ms after the last change. */
  setThumbSize(size: number): void;
  /** Sets the current photo, anchor and selection (#19's clicks and keys). */
  setSelection(selected: Selected): void;
}

type Cause = "first" | "user" | "change";

interface Session {
  client: QueryClient;
  ready: Promise<void>;
  unlisten: Unlisten | null;
  busy: () => boolean;
  /** Why the newest call was made. */
  cause: Cause;
  lastRerun: number;
  rerunTimer: ReturnType<typeof setTimeout> | undefined;
  frame: number | undefined;
}

let session: Session | null = null;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
/** Whether the size was chosen on the page, so the stored setting no longer applies. */
let sizeChosen = false;
let blocks = newBlocks();

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
        if (isAppError(e) && e.code === "query_expired") rerunExpired();
        throw e;
      }
    },
    (e) => {
      if (!(isAppError(e) && e.code === "query_expired")) useGridStore.setState({ error: toAppError(e) });
    },
  );
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
    set({ result, error: null, ...selected, scroll });
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

  rerunExpired = () => {
    if (session !== null) rerun(session);
  };

  return {
    result: null,
    query: defaultQuery(),
    error: null,
    thumbSize: THUMB_DEFAULT,
    scroll: null,
    ...empty,

    start(options = {}) {
      if (session !== null) return session.ready;
      const s: Session = {
        client: new QueryClient(api(), {
          keep: () => {
            const { current, anchor, selection } = get();
            return { current, anchor_id: anchor?.id ?? null, selection };
          },
          onResult: (result) => applyResult(s, result),
        }),
        ready: Promise.resolve(),
        unlisten: null,
        busy: options.busy ?? (() => false),
        cause: "first",
        lastRerun: Number.NEGATIVE_INFINITY,
        rerunTimer: undefined,
        frame: undefined,
      };
      session = s;
      s.ready = (async () => {
        const settings = useSettingsStore.getState();
        if (settings.settings === null) await settings.load();
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
        clearTimeout(s.rerunTimer);
        if (s.frame !== undefined) cancelAnimationFrame(s.frame);
      }
      blocks.reset("", 0);
      set({ result: null, error: null, query: defaultQuery(), scroll: null, ...empty });
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
      const before = get().current;
      const moved =
        selected.current !== null &&
        (before === null || before.id !== selected.current.id || before.index !== selected.current.index);
      set({ ...selected, scroll: moved ? nextScroll("current") : get().scroll });
    },
  };
});

/** Stops the grid and puts its store back as it started. For tests and closing the page. */
export function resetGrid(): void {
  useGridStore.getState().stop();
  clearTimeout(saveTimer);
  saveTimer = undefined;
  sizeChosen = false;
  blocks = newBlocks();
  useGridStore.setState({ thumbSize: THUMB_DEFAULT });
}
