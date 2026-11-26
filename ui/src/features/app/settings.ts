import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { AppError, Settings, SettingsPatch } from "../../api";

/**
 * The app's settings, as Rust keeps them in `app.sqlite`. Every preference
 * goes through here; the page keeps nothing in `localStorage`
 * (docs/02-data.md, "app.sqlite").
 */
export interface SettingsState {
  /** Every setting, once loaded. */
  settings: Settings | null;
  /** Why the last load or change failed, if it did. */
  error: AppError | null;
  /** Asks Rust for every setting. */
  load(): Promise<void>;
  /**
   * Saves some settings. The store then holds Rust's answer; a refused
   * change leaves the settings as they were and sets `error`.
   */
  update(patch: SettingsPatch): Promise<void>;
}

export const useSettingsStore = create<SettingsState>()((set) => ({
  settings: null,
  error: null,
  async load() {
    try {
      const settings = await api().invoke("app_settings_get");
      set({ settings, error: null });
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
  async update(patch) {
    try {
      const settings = await api().invoke("app_settings_set", { patch });
      set({ settings, error: null });
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
}));
