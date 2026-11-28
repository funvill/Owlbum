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
  /** The chosen models' names by part, read with `tagging_models` when a run starts; for the bar's tooltip. */
  models: { describe: string | null; keywords: string | null } | null;
  /** When this run was first seen running, and how many photos were seen then: the time left is worked out from it. */
  since: { at: number; seen: number } | null;
}

export const useTaggingStore = create<TaggingState>(() => ({ progress: null, hiddenAt: null, models: null, since: null }));

/** The clock the time left is measured with; tests replace it. */
let now = () => Date.now();

/** For tests: the clock the time left is measured with. */
export function setTaggingClock(clock: () => number): void {
  now = clock;
}

/** Reads the chosen models' names for the bar's tooltip (issue 198). A failure leaves them unknown. */
async function readModels(): Promise<void> {
  try {
    const state = await api().invoke("tagging_models");
    const label = (id: string | null) => (id === null ? null : (state.models.find((m) => m.id === id)?.label ?? null));
    useTaggingStore.setState({ models: { describe: label(state.settings.describe), keywords: label(state.settings.keywords) } });
  } catch {
    // The tooltip says what tagging does without the models' names.
  }
}

/** A new status: a run that starts is timed from here, and its models are read. */
function arrived(p: TaggingProgress): void {
  const before = useTaggingStore.getState();
  const seen = p.done + p.failed;
  const starts = p.running && (before.since === null || !(before.progress?.running ?? false));
  useTaggingStore.setState({
    progress: p,
    since: !p.running ? null : starts ? { at: now(), seen } : before.since,
  });
  if (starts) void readModels();
}

/**
 * The seconds tagging still needs, from the pace since the run started:
 * none until 3 photos and 10 seconds have gone by.
 */
export function secondsLeft(p: TaggingProgress, since: TaggingState["since"], at = now()): number | null {
  if (since === null || !p.running) return null;
  const seen = p.done + p.failed;
  const did = seen - since.seen;
  const took = (at - since.at) / 1000;
  if (did < 3 || took < 10) return null;
  return Math.max(0, ((p.total - seen) * took) / did);
}

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
    listening = await api().listen("tagging_progress", arrived);
    const status = await api().invoke("tagging_status");
    // An event that arrived meanwhile is newer.
    if (useTaggingStore.getState().progress === null) arrived(status);
  } catch {
    // No status: the bar stays hidden until the first event.
  }
}

/** Stops listening and forgets the status. For tests. */
export function resetTagging(): void {
  listening?.();
  listening = null;
  listenedTo = null;
  useTaggingStore.setState({ progress: null, hiddenAt: null, models: null, since: null });
  now = () => Date.now();
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
