// The page's view of tagging: the newest `tagging_progress`, read once with
// `tagging_status` and then from the event alone (docs/07-tagging.md, "Status
// and the `tagging_progress` event"). The page never polls.

import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { Api, Unlisten } from "../../api";
import type { TaggingProgress } from "../../api/bindings/TaggingProgress";
import { showToast } from "../../controls";

export interface TaggingState {
  /** The newest status; null until the first arrives. */
  progress: TaggingProgress | null;
  /** The finished-with-failures line was closed while the counts were these. */
  hiddenAt: string | null;
}

export const useTaggingStore = create<TaggingState>(() => ({ progress: null, hiddenAt: null }));

/** The counts as one string: closing the line hides it until this changes. */
export function countsKey(p: TaggingProgress): string {
  return `${p.total}|${p.done}|${p.failed}`;
}

let listening: Unlisten | null = null;
/** The backend listened to: once per page load, or again when a test installs another. */
let listenedTo: Api | null = null;

/** Listens for `tagging_progress`, then reads `tagging_status`. Once per page load. */
export async function startTagging(): Promise<void> {
  if (listenedTo === api()) return;
  listening?.();
  listening = null;
  listenedTo = api();
  try {
    listening = await api().listen("tagging_progress", (p) => useTaggingStore.setState({ progress: p }));
    const now = await api().invoke("tagging_status");
    // An event that arrived meanwhile is newer.
    if (useTaggingStore.getState().progress === null) useTaggingStore.setState({ progress: now });
  } catch {
    // No status: the bar stays hidden until the first event.
  }
}

/** Stops listening and forgets the status. For tests. */
export function resetTagging(): void {
  listening?.();
  listening = null;
  listenedTo = null;
  useTaggingStore.setState({ progress: null, hiddenAt: null });
}

/** Closes the "could not be tagged" line until the counts next change. */
export function hideTaggingLine(): void {
  const p = useTaggingStore.getState().progress;
  if (p !== null) useTaggingStore.setState({ hiddenAt: countsKey(p) });
}

/** Runs one of the bar's commands; an error raises a toast. The page waits for the next event. */
export async function taggingAction(run: () => Promise<unknown>): Promise<void> {
  try {
    await run();
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
  }
}
