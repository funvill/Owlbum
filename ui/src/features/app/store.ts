import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { AppError } from "../../api";

/** Program-wide facts the page shows. Each feature has its own store. */
export interface AppState {
  /** The program's version, once known. */
  version: string | null;
  /** Why the version could not be read, if it could not. */
  error: AppError | null;
  /** Asks Rust for the version. */
  loadVersion(): Promise<void>;
}

export const useAppStore = create<AppState>()((set) => ({
  version: null,
  error: null,
  async loadVersion() {
    try {
      const { version } = await api().invoke("app_version");
      set({ version, error: null });
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
}));
