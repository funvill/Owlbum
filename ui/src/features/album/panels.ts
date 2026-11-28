// Hiding and moving the review screen's panels (docs/04-browsing.md,
// "Hiding and moving the panels"; owner's feedback, issue 185). The
// thumbnails and the details panel can each be hidden and shown again, and
// the thumbnails can sit in a column at the left or in a strip along the
// bottom. Each choice is a setting, so it is remembered.

import { useEffect } from "react";
import type { GridPlace } from "../../api/bindings/GridPlace";
import { matchKey, type KeyId } from "../../keys";
import { useSettingsStore } from "../app/settings";

/** Which panels show, and where the thumbnails sit. */
export interface Panels {
  grid: boolean;
  details: boolean;
  place: GridPlace;
}

/** The panels the settings ask for; before they load, everything shows at the left. */
export function usePanels(): Panels {
  const grid = useSettingsStore((s) => s.settings?.["panel.grid_shown"] ?? true);
  const details = useSettingsStore((s) => s.settings?.["panel.details_shown"] ?? true);
  const place = useSettingsStore((s) => s.settings?.["panel.grid_place"] ?? "left");
  return { grid, details, place };
}

/** Shows the thumbnails when hidden, hides them when shown. */
export async function toggleGrid(): Promise<void> {
  const shown = useSettingsStore.getState().settings?.["panel.grid_shown"] ?? true;
  await useSettingsStore.getState().update({ "panel.grid_shown": !shown });
}

/** Shows the details panel when hidden, hides it when shown. */
export async function toggleDetails(): Promise<void> {
  const shown = useSettingsStore.getState().settings?.["panel.details_shown"] ?? true;
  await useSettingsStore.getState().update({ "panel.details_shown": !shown });
}

/** Puts the thumbnails at `place`, and shows them: moving hidden thumbnails would show nothing. */
export async function setGridPlace(place: GridPlace): Promise<void> {
  await useSettingsStore.getState().update({ "panel.grid_place": place, "panel.grid_shown": true });
}

/** Moves the thumbnails between the left and the bottom (View > Thumbnails at the bottom). */
export async function toggleGridPlace(): Promise<void> {
  const place = useSettingsStore.getState().settings?.["panel.grid_place"] ?? "left";
  await setGridPlace(place === "left" ? "bottom" : "left");
}

const PANEL_KEYS: Partial<Record<KeyId, () => Promise<void>>> = {
  "browse.toggle_grid": toggleGrid,
  "browse.toggle_details": toggleDetails,
};

/** Whether a modal dialog is open anywhere on the page, which stops every review key. */
function dialogOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/** T and I on the review screen: hide or show the thumbnails and the details panel. */
export function usePanelKeys(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const match = matchKey(e, { place: "browse", focus: dialogOpen() ? "dialog" : undefined });
      const act = match === null ? undefined : PANEL_KEYS[match.id];
      if (match === null || act === undefined) return;
      e.preventDefault();
      if (match.act) void act();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
