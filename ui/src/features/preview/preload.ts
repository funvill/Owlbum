// Wiring the image cache to the page (docs/04-browsing.md, "Cache rules"):
// on every change of the photo on screen, want it and its preload window,
// mapped to medium addresses. The preview uses it now; the viewer (#22)
// calls the same hook for the photo it shows.

import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import { api, thumbUrl } from "../../api";
import { gridBlocks } from "../grid";
import { imageCache, preloadOrder, type Wanted } from "./cache";

/** Whether `url` is decoded in the image cache; follows the cache. */
export function useCachedReady(url: string): boolean {
  return useSyncExternalStore(
    useCallback((listener: () => void) => imageCache().subscribe(listener), []),
    () => imageCache().ready(url),
  );
}

/**
 * Wants the photo at `index` and its preload window. `index` is where the
 * photo on screen is (or is going, while a step waits for its row). Only
 * rows of the current result with a thumbnail are asked for; the rest are
 * picked up when their blocks arrive, which the grid already fetches.
 */
export function usePreload(index: number | null): void {
  const version = useSyncExternalStore(
    useCallback((listener: () => void) => gridBlocks().subscribe(listener), []),
    () => gridBlocks().version,
  );
  const direction = useRef<1 | -1>(1);
  const last = useRef<number | null>(null);

  useEffect(() => {
    if (index === null) return;
    if (last.current !== null && index !== last.current) direction.current = index < last.current ? -1 : 1;
    last.current = index;
    const blocks = gridBlocks();
    const wanted: Wanted[] = [];
    for (const i of [index, ...preloadOrder(index, blocks.count, direction.current)]) {
      if (!blocks.fresh(i)) continue;
      const row = blocks.row(i);
      if (row === undefined || row.thumb_key === "") continue;
      wanted.push({ id: row.id, url: thumbUrl(row.thumb_key, "m") });
    }
    imageCache().want(wanted);
  }, [index, version]);

  // Photos whose files changed: their cached pictures are stale.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let gone = false;
    void api()
      .listen("album_changed", (change) => {
        if (change.rekeyed !== undefined) imageCache().dropPhotos(change.rekeyed);
      })
      .then((u) => {
        if (gone) u();
        else unlisten = u;
      });
    return () => {
      gone = true;
      unlisten?.();
    };
  }, []);
}
