// Loading the current photo's details (docs/04-browsing.md, "The details
// panel", "Loading").

import { useEffect, useRef, useState } from "react";
import { api, isAppError, toAppError } from "../../api";
import type { AppError } from "../../api";
import type { Histogram } from "../../api/bindings/Histogram";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";

export interface DetailState {
  /** The details of the photo asked for; null until the first answer. */
  detail: PhotoDetail | null;
  /** Why the details could not be read, other than `photo_not_found`. */
  error: AppError | null;
}

const empty: DetailState = { detail: null, error: null };

/**
 * The details of photo `id`. It asks `photo_detail` when the id changes,
 * and again on an `album_changed` for a revision newer than the details it
 * holds. The previous photo's details are never answered under a new id,
 * and an answer for an id no longer asked for is thrown away. A
 * `photo_not_found` answer calls `onNotFound`, which re-runs the query.
 */
export function useDetail(id: number | null, onNotFound: () => void): DetailState {
  const [state, setState] = useState<DetailState & { id: number | null }>({ ...empty, id: null });
  /** The newest `album_changed` revision seen. */
  const newest = useRef(0);
  const notFound = useRef(onNotFound);
  useEffect(() => {
    notFound.current = onNotFound;
  });

  useEffect(() => {
    if (id === null) return;
    let active = true;
    let asked = 0;
    let held: PhotoDetail | null = null;

    const load = () => {
      const n = ++asked;
      api()
        .invoke("photo_detail", { id })
        .then(
          (detail) => {
            if (!active || n !== asked) return;
            held = detail;
            setState({ detail, error: null, id });
            // A change that landed while this was read: read again.
            if (newest.current > detail.revision) load();
          },
          (e: unknown) => {
            if (!active || n !== asked) return;
            if (isAppError(e) && e.code === "photo_not_found") {
              notFound.current();
              return;
            }
            setState({ detail: held, error: toAppError(e), id });
          },
        );
    };
    load();

    let unlisten: (() => void) | null = null;
    void api()
      .listen("album_changed", (change) => {
        newest.current = Math.max(newest.current, change.revision);
        if (held !== null && change.revision > held.revision) load();
      })
      .then((u) => {
        if (active) unlisten = u;
        else u();
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [id]);

  // Until this id's first answer, the state held is another photo's.
  return state.id === id ? { detail: state.detail, error: state.error } : empty;
}

export type HistogramState = { kind: "loading" } | { kind: "ready"; histogram: Histogram } | { kind: "failed" };

/**
 * The histogram of photo `id`, asked for once, and again only when the
 * photo or its thumbnail changes. Only an open section calls it.
 */
export function useHistogram(id: number, thumbKey: string): HistogramState {
  const key = `${id}:${thumbKey}`;
  const [state, setState] = useState<{ key: string; value: HistogramState }>({ key: "", value: { kind: "loading" } });

  useEffect(() => {
    let current = true;
    api()
      .invoke("photo_histogram", { id })
      .then(
        (histogram) => {
          if (current) setState({ key, value: { kind: "ready", histogram } });
        },
        () => {
          if (current) setState({ key, value: { kind: "failed" } });
        },
      );
    return () => {
      current = false;
    };
    // `key` holds both the id and the thumbnail key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return state.key === key ? state.value : { kind: "loading" };
}
