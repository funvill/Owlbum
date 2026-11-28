// The program's lifecycle as the page sees it (docs/08-desktop-and-release.md):
// the updater's state and its messages, the first-start update notice,
// What's new, and a fatal error Rust asks the page to show. Rust owns every
// decision; this store only holds what to draw.

import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { AppFatal } from "../../api/bindings/AppFatal";
import type { Startup } from "../../api/bindings/Startup";
import type { UpdateMessage } from "../../api/bindings/UpdateMessage";
import type { UpdateState } from "../../api/bindings/UpdateState";
import type { WhatsNewEntry } from "../../api/bindings/WhatsNewEntry";
import type { Unlisten } from "../../api/types";
import { showToast } from "../../controls";

interface WhatsNew {
  entries: WhatsNewEntry[];
  /** Shown at start: closing it records the running version. */
  atStart: boolean;
}

interface LifecycleState {
  version: string;
  update: UpdateState;
  /** The update message being shown, if any. */
  message: UpdateMessage | null;
  /** The first-start notice is waiting for an answer. */
  notice: boolean;
  whatsNew: WhatsNew | null;
  fatal: AppFatal | null;
}

export const useLifecycle = create<LifecycleState>(() => ({
  version: "",
  update: { state: "unavailable", version: null, percent: null },
  message: null,
  notice: false,
  whatsNew: null,
  fatal: null,
}));

function failed(e: unknown): void {
  showToast({ kind: "error", message: toAppError(e).message });
}

/** Listens for the updater's events and a fatal error. */
export async function listenLifecycle(): Promise<Unlisten[]> {
  const listening = await Promise.all([
    api().listen("update_state", (update) => useLifecycle.setState({ update })),
    api().listen("update_message", (message) => useLifecycle.setState({ message })),
    api().listen("app_fatal", (fatal) => useLifecycle.setState({ fatal })),
  ]);
  void api()
    .invoke("app_update_state")
    .then((update) => useLifecycle.setState({ update }), failed);
  return listening;
}

/** What `app_startup` said about the notice and What's new. */
export function startLifecycle(startup: Startup): void {
  useLifecycle.setState({
    version: startup.version,
    notice: startup.show_update_notice,
    whatsNew: startup.whats_new.length > 0 ? { entries: startup.whats_new, atStart: true } : null,
  });
}

/** Help > What's new: the running version's notes, never recorded. */
export async function openWhatsNew(): Promise<void> {
  try {
    const entries = await api().invoke("app_whats_new");
    useLifecycle.setState({ whatsNew: { entries, atStart: false } });
  } catch (e) {
    failed(e);
  }
}

export function closeWhatsNew(): void {
  const shown = useLifecycle.getState().whatsNew;
  useLifecycle.setState({ whatsNew: null });
  if (shown?.atStart === true) void api().invoke("app_whats_new_seen").catch(failed);
}

/** The first-start notice's answer: OK keeps the daily check on. */
export async function answerNotice(keepOn: boolean): Promise<void> {
  useLifecycle.setState({ notice: false });
  try {
    await api().invoke("app_update_notice_answer", { keep_on: keepOn });
  } catch (e) {
    failed(e);
  }
}

export function closeMessage(): void {
  useLifecycle.setState({ message: null });
}

/** Runs one of the updater's commands, a failure as a toast. */
export function updateCommand(
  command:
    | "app_check_updates"
    | "app_update_download"
    | "app_update_install"
    | "app_open_release_page"
    | "app_open_release_notes",
): void {
  void api().invoke(command).catch(failed);
}
