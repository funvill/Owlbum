// The colour filters of edited photos: one hidden SVG at the page root with
// one `feColorMatrix` per distinct edit key in use
// (docs/06-editing-and-file-actions.md, "How the page draws a plan"). CSS
// filter functions such as brightness() are never used: their maths
// differs from the model's.

import { useEffect } from "react";
import { create } from "zustand";
import { filterId } from "./drawing";

interface Held {
  values: string;
  users: number;
}

export const useColourFilters = create<{ held: Record<string, Held> }>(() => ({ held: {} }));

/** Keeps the filter for `key` while the calling component is mounted. */
export function useColourFilter(key: string | null, values: string | null): void {
  useEffect(() => {
    if (key === null || values === null) return;
    useColourFilters.setState(({ held }) => ({
      held: { ...held, [key]: { values, users: (held[key]?.users ?? 0) + 1 } },
    }));
    return () =>
      useColourFilters.setState(({ held }) => {
        const h = held[key];
        if (h === undefined) return { held };
        const next = { ...held };
        if (h.users <= 1) delete next[key];
        else next[key] = { ...h, users: h.users - 1 };
        return { held: next };
      });
  }, [key, values]);
}

/** The hidden SVG holding every filter in use. Mounted once, in `App`. */
export function ColourFilters() {
  const held = useColourFilters((s) => s.held);
  return (
    <svg width="0" height="0" aria-hidden="true" focusable="false" style={{ position: "absolute" }}>
      {Object.entries(held).map(([key, h]) => (
        <filter key={key} id={filterId(key)} colorInterpolationFilters="sRGB">
          <feColorMatrix type="matrix" values={h.values} />
        </filter>
      ))}
    </svg>
  );
}
