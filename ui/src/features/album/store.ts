import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { AppError, Unlisten } from "../../api";
import type { ScanProgress } from "../../api/bindings/ScanProgress";
import { albumText } from "../../text/album";

/** No scan has run, or the last one ended. */
export const idleScan: ScanProgress = {
  phase: "idle",
  found: 0,
  done: 0,
  failed: 0,
  finished: false,
  cancelled: false,
};

/** The open album's scan, as `scan_progress` reports it. */
export interface ScanState {
  /** The last `scan_progress`. */
  progress: ScanProgress;
  /** Why the last scan command failed, if it did. */
  error: AppError | null;
  /** Starts following `scan_progress`. */
  start(): Promise<void>;
  /** Stops following it. */
  stop(): void;
  /** Manual Rescan: retries every file with a scan error. */
  rescan(): Promise<void>;
  /** Cancels the running scan. */
  cancel(): Promise<void>;
}

let unlisten: Unlisten | null = null;

export const useScanStore = create<ScanState>()((set) => ({
  progress: idleScan,
  error: null,
  async start() {
    if (unlisten !== null) return;
    unlisten = await api().listen("scan_progress", (progress) => set({ progress }));
  },
  stop() {
    unlisten?.();
    unlisten = null;
  },
  async rescan() {
    try {
      await api().invoke("album_rescan");
      set({ error: null });
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
  async cancel() {
    try {
      await api().invoke("album_scan_cancel");
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
}));

/** The status-bar text for a scan; null when no scan runs. */
export function scanLabel(p: ScanProgress): string | null {
  if (p.finished) return null;
  switch (p.phase) {
    case "walking":
      return albumText.scanWalking;
    case "reading":
      return albumText.scanReading(p.done, p.found);
    case "idle":
      return null;
  }
}
