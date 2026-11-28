// "Show on map" (docs/04-browsing.md, "Show on map"; owner's feedback,
// issue 183): the details panel asks the Map view to centre one photo's
// marker. The photo is the page's current photo, so its marker is already
// the highlighted one; the view only has to come up and centre it.

import { create } from "zustand";
import { useSettingsStore } from "../app";
import { useGridStore } from "../grid";

interface MapFocus {
  /** The photo whose marker the map centres once it holds it, or null. */
  id: number | null;
}

export const useMapFocus = create<MapFocus>(() => ({ id: null }));

/**
 * Switches to the Map view (saved as the setting `view`, like the view
 * switch) and asks it to centre photo `id`'s marker.
 */
export function showOnMap(id: number): void {
  useMapFocus.setState({ id });
  const grid = useGridStore.getState();
  if (grid.query.view === "map") return;
  grid.setQuery({ ...grid.query, view: "map" });
  void useSettingsStore.getState().update({ view: "map" });
}

/** Forgets a request (tests). */
export function resetMapFocus(): void {
  useMapFocus.setState({ id: null });
}
