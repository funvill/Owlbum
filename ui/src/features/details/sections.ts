// The details panel's foldable sections (docs/04-browsing.md, "Sections").
// Which are folded is the setting `sections`, `{ [id]: boolean }`; a
// missing id counts as open.

import { useCallback, useState } from "react";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import type { SectionId } from "../../text/details";
import { useSettingsStore } from "../app";

/** Every section, in the order the panel shows them. */
export const SECTION_IDS: readonly SectionId[] = [
  "photo",
  "whatsinit",
  "camera",
  "histogram",
  "place",
  "people",
  "date",
  "notes",
];

/**
 * Whether a section is shown at all for this photo. The Histogram is not
 * shown for videos. "What's in it" waits for its data, which #55 adds to
 * `photo_detail`.
 */
export function sectionShown(id: SectionId, d: PhotoDetail): boolean {
  switch (id) {
    case "histogram":
      return d.kind !== "video";
    case "whatsinit":
      return d.tags !== null;
    default:
      return true;
  }
}

/**
 * The sections' open state, from the setting, with a fold or unfold shown
 * at once and saved through `app_settings_set`.
 */
export function useSections(): { isOpen(id: SectionId): boolean; toggle(id: SectionId): void } {
  const stored = useSettingsStore((s) => s.settings?.sections);
  const update = useSettingsStore((s) => s.update);
  const [local, setLocal] = useState<Record<string, boolean>>({});

  const isOpen = useCallback((id: SectionId) => local[id] ?? stored?.[id] ?? true, [local, stored]);
  const toggle = useCallback(
    (id: SectionId) => {
      const next = { ...stored, ...local, [id]: !isOpen(id) };
      setLocal(next);
      void update({ sections: next });
    },
    [stored, local, isOpen, update],
  );
  return { isOpen, toggle };
}
