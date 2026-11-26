import type { ScanProgress } from "../../api/bindings/ScanProgress";
import { albumText } from "../../text/album";

/** Where reading started in this scan: `done` on its first `reading` event, and when (ms). */
export interface ReadStart {
  done: number;
  at: number;
}

/** No estimate until this many files have been read… */
export const ESTIMATE_AFTER_FILES = 5;
/** …and this long has been spent reading. */
export const ESTIMATE_AFTER_MS = 2000;

/**
 * The time left, as words, or null when there is none yet
 * (docs/04-browsing.md, "Reading bar", "Estimate"). Unchanged files are
 * counted in `done` at once, so only files read since `start` count as work.
 */
export function estimate(p: ScanProgress, start: ReadStart | null, now: number): string | null {
  if (start === null || p.phase !== "reading") return null;
  const read = p.done - start.done;
  const elapsed = now - start.at;
  if (read < ESTIMATE_AFTER_FILES || elapsed < ESTIMATE_AFTER_MS) return null;
  const seconds = ((elapsed / 1000) / read) * Math.max(0, p.found - p.done);
  if (seconds < 60) return albumText.lessThanAMinute;
  const minutes = Math.ceil(seconds / 60);
  if (minutes >= 90) return albumText.hoursLeft(Math.floor(minutes / 60), minutes % 60);
  return albumText.minutesLeft(minutes);
}

/** The reading bar's label (docs/04-browsing.md, "Reading bar"). */
export function readingLabel(p: ScanProgress, start: ReadStart | null, now: number): string {
  if (p.phase !== "reading" || p.found === 0) return albumText.scanWalking;
  const left = estimate(p, start, now);
  const base = albumText.scanReading(p.done, p.found);
  return left === null ? base : `${base} · ${left}`;
}
