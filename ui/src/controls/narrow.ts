import { useSyncExternalStore } from "react";

/**
 * Below 1100 px the bars give up room (docs/09-visual-design.md, "The
 * bars"): the top bar's buttons become icon buttons, and the filter bar
 * hides its hint row and shows the photo count as the number alone.
 */
const NARROW = "(max-width: 1099px)";

function narrowQuery(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(NARROW) : null;
}

function subscribeNarrow(changed: () => void): () => void {
  const query = narrowQuery();
  query?.addEventListener("change", changed);
  return () => query?.removeEventListener("change", changed);
}

/** True when the window is narrower than 1100 px. */
export function useNarrowWindow(): boolean {
  return useSyncExternalStore(subscribeNarrow, () => narrowQuery()?.matches ?? false);
}
