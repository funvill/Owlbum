import type { Theme } from "../../api";
import { useSettingsStore } from "./settings";

/**
 * The colour theme (docs/09-visual-design.md, "Colour tokens"; issue 75).
 * The `theme` setting picks dark, light or the system's choice; the root
 * element's `data-theme` selects the token block. Before the settings
 * arrive the system's choice is used, which is also the setting's default.
 */

/** The theme to draw with. */
export function resolveTheme(setting: Theme, systemPrefersLight: boolean): "dark" | "light" {
  if (setting === "system") return systemPrefersLight ? "light" : "dark";
  return setting;
}

const LIGHT_QUERY = "(prefers-color-scheme: light)";

function systemQuery(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(LIGHT_QUERY) : null;
}

function draw(): void {
  const setting = useSettingsStore.getState().settings?.theme ?? "system";
  document.documentElement.dataset.theme = resolveTheme(setting, systemQuery()?.matches ?? false);
}

/**
 * Keeps `data-theme` in step with the setting and, under `system`, with
 * the system. Answers a function that stops following.
 */
export function followTheme(): () => void {
  draw();
  const stopStore = useSettingsStore.subscribe((state, before) => {
    if (state.settings?.theme !== before.settings?.theme) draw();
  });
  const query = systemQuery();
  query?.addEventListener("change", draw);
  return () => {
    stopStore();
    query?.removeEventListener("change", draw);
  };
}
