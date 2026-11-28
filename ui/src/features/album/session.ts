import { create } from "zustand";
import { api, toAppError } from "../../api";
import type { AppError, Recent, Unlisten } from "../../api";
import type { AlbumStatus } from "../../api/bindings/AlbumStatus";
import type { OpenOptions } from "../../api/bindings/OpenOptions";
import type { ScanProgress } from "../../api/bindings/ScanProgress";
import { showToast } from "../../controls";
import { albumText } from "../../text/album";
import { useSettingsStore } from "../app/settings";
import { listenLifecycle, startLifecycle } from "../app/lifecycle";
import { restoreSession, showPhoto, type SavedSession } from "../grid";
import { followEditStatus } from "../edit/save";
import { flushSession, followSession, stopSession } from "./reopen";
import type { ReadStart } from "./reading";

/** A question `album_open` asked, with what the dialog needs to answer it. */
export type OpenQuestion =
  | { kind: "stale"; path: string; message: string }
  | { kind: "damaged"; path: string; message: string; hasBackup: boolean };

/**
 * The window's album: which one is open, or opening, the questions opening
 * asked, the recent list and the reading bar (docs/04-browsing.md, "Whole
 * window" and "Opening a folder"). Rust holds the album; this is the
 * page's view of it.
 */
export interface AlbumState {
  /** The folder whose `album_open` has not answered yet. */
  opening: { path: string; name: string } | null;
  /** The open album; null when none is open. */
  status: AlbumStatus | null;
  /** The recent list, newest first. */
  recent: Recent[];
  /** A question to answer before the album opens. */
  question: OpenQuestion | null;
  /** Something is being dragged over the window. */
  dropOver: boolean;
  /** The conflict banner was dismissed until the album is opened again. */
  conflictHidden: boolean;
  /** The notices were dismissed while they said this (`noticeKey`); null when shown. */
  noticesHidden: string | null;
  /** The reading bar shows, with this scan's progress. */
  reading: ScanProgress | null;
  /** Where reading started in this scan, for the estimate. */
  readStart: ReadStart | null;

  /** Starts listening, then asks Rust what to open first (`app_startup`). Once per page load. */
  start(): Promise<void>;
  /** Stops listening. */
  stop(): void;
  /**
   * Opens a folder that a route chose: the dialog, a recent entry, a drop,
   * `open_folder`. The folder that is already open is left as it is.
   * `photo`, a picture's full path, is selected and shown (a picture
   * opened from the file manager, docs/04-browsing.md, "Opening a picture").
   */
  openFolder(path: string, photo?: string | null): Promise<void>;
  /**
   * Calls `album_open`, closing any open album; answers to questions pass
   * `options`. With `photo`, that picture is selected and shown instead of
   * the saved session's place.
   */
  open(path: string, options?: Partial<OpenOptions>, photo?: string | null): Promise<void>;
  /** The Open folder button: the native dialog, then `openFolder`. */
  pickFolder(): Promise<void>;
  /** Close folder: back to the start screen. */
  close(): Promise<void>;
  /** Reads `album_status` again. */
  refresh(): Promise<void>;
  /** Removes one folder from the recent list. */
  removeRecent(root: string): Promise<void>;
  /** Answers the stale-lock question. */
  answerStale(choice: "anyway" | "view_only"): Promise<void>;
  /** Answers the damaged-database question. */
  answerDamaged(choice: "restore" | "view_only" | "cancel"): Promise<void>;
  /** Restore backup… on the damaged banner: asks the damaged question again. */
  askRestore(): Promise<void>;
  /** Hides the conflict banner until the album is opened again. */
  hideConflict(): void;
  /** Hides the notices until their numbers change or the album is opened again. */
  hideNotices(): void;
}

/**
 * What the notices say, as one string: dismissing them hides them until
 * this changes (docs/04-browsing.md, "Notices").
 */
export function noticeKey(status: AlbumStatus): string {
  return `${status.unsupported.count}|${status.unsupported.labels.join(",")}|${status.unreadable}`;
}

/** The last part of a path: an opened folder's own name, for messages. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter((p) => p !== "");
  return parts[parts.length - 1] ?? path;
}

/** A scan after the first shows its bar only once it has run this long. */
export const LATE_SCAN_SHOW_MS = 1000;
/** `album_changed` during a scan reads the status at most this often. */
export const REFRESH_EVERY_MS = 1000;

let listening: Unlisten[] = [];
/** `start` runs once per page load, whatever React does with effects. */
let started = false;
/** Raised by each `album_open` call: an answer to an older call is ignored. */
let openCalls = 0;
/** Raised by each `scan_progress` with `finished`. */
let scansFinished = 0;
/** "Can't watch this folder" is said once per session. */
let notWatchingSaid = false;
/**
 * A picture a launch named that the album did not hold yet when it opened
 * (a folder opened for the first time): looked for as the scan goes on,
 * and shown when found. `call` is the `album_open` it belongs to.
 */
let wantedPhoto: { path: string; call: number; tries: number } | null = null;
/** After the scan, a found picture the grid does not show yet is tried this many more times, a second apart. */
const WANTED_TRIES = 5;

/** The id of the picture at `path` in the open album; null when it is not there (yet). */
async function findPhoto(path: string): Promise<number | null> {
  try {
    return await api().invoke("photo_find", { path });
  } catch {
    return null;
  }
}

/**
 * Looks for the wanted picture again: shown once the scan has reached it
 * and the grid holds it. Given up once the scan is over and it still is
 * not there.
 */
async function lookForWanted(scanning: boolean): Promise<void> {
  const wanted = wantedPhoto;
  if (wanted === null || wanted.call !== openCalls) return;
  const id = await findPhoto(wanted.path);
  if (wantedPhoto !== wanted) return;
  if (id !== null && (await showPhoto(id))) {
    if (wantedPhoto === wanted) wantedPhoto = null;
    return;
  }
  if (scanning) return;
  wanted.tries += 1;
  if (id === null || wanted.tries > WANTED_TRIES) {
    wantedPhoto = null;
    return;
  }
  // Found, but the grid's result is not the one with it yet.
  setTimeout(() => void lookForWanted(false), REFRESH_EVERY_MS);
}
let lateScanTimer: ReturnType<typeof setTimeout> | null = null;
let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let lastRefresh = 0;

/**
 * What must finish before the open album is left (another album opened,
 * or this one closed): edit mode ends as Done, so its draft is kept
 * (docs/06-editing-and-file-actions.md, "Ending edit mode").
 */
const beforeLeave = new Set<() => Promise<unknown>>();

/** Runs `fn` before the open album is left, until the answered function is called. */
export function onLeaveAlbum(fn: () => Promise<unknown>): () => void {
  beforeLeave.add(fn);
  return () => beforeLeave.delete(fn);
}

async function leaving(): Promise<void> {
  await Promise.all([...beforeLeave].map((fn) => fn().catch(() => {})));
}

/** The window is closing: finish what the album needs, then tell Rust. */
async function closingWindow(): Promise<void> {
  await leaving();
  await flushSession();
  try {
    await api().invoke("app_close_ready");
  } catch {
    // Rust closes anyway after 3 seconds.
  }
}

/** Forgets what one session said and timed. For tests. */
export function resetAlbumSession(): void {
  started = false;
  stopSession();
  openCalls = 0;
  scansFinished = 0;
  notWatchingSaid = false;
  wantedPhoto = null;
  if (lateScanTimer !== null) clearTimeout(lateScanTimer);
  if (refreshTimer !== null) clearTimeout(refreshTimer);
  lateScanTimer = null;
  refreshTimer = null;
  lastRefresh = 0;
}

const idle: ScanProgress = {
  phase: "walking",
  found: 0,
  done: 0,
  failed: 0,
  finished: false,
  cancelled: false,
};

export const useAlbumStore = create<AlbumState>()((set, get) => {
  /** The newest progress of a scan whose bar is not shown yet. */
  let latest: ScanProgress | null = null;
  /** The damaged question came from the banner: Cancel keeps the album open view-only. */
  let restoringFromBanner = false;

  /** Takes in a status from Rust, and says once that the folder is not watched. */
  const took = (status: AlbumStatus | null) => {
    set({ status });
    if (status !== null && !status.scan.watching && !notWatchingSaid) {
      notWatchingSaid = true;
      showToast({ kind: "info", message: albumText.notWatching });
    }
  };

  const loadRecent = async () => {
    try {
      set({ recent: await api().invoke("app_recent_list") });
    } catch {
      // The list stays as it was; it is only a convenience.
    }
  };

  const failed = (path: string, e: AppError) => {
    const name = folderName(path);
    switch (e.code) {
      case "album_lock_stale":
        set({ question: { kind: "stale", path, message: e.message } });
        return;
      case "album_damaged": {
        const date = (e.details as { backup_date?: unknown } | undefined)
          ?.backup_date;
        set({
          question: {
            kind: "damaged",
            path,
            message: e.message,
            hasBackup: typeof date === "string",
          },
        });
        return;
      }
      default:
        showToast({
          kind: "error",
          message: albumText.couldNotOpen(name, e.message),
        });
    }
  };

  const onProgress = (p: ScanProgress) => {
    if (p.finished) {
      scansFinished += 1;
      if (lateScanTimer !== null) clearTimeout(lateScanTimer);
      lateScanTimer = null;
      set({ reading: null, readStart: null });
      if (get().status !== null) void get().refresh();
      return;
    }
    const readStart =
      get().readStart ??
      (p.phase === "reading" ? { done: p.done, at: Date.now() } : null);
    if (get().reading !== null) {
      set({ reading: p, readStart });
      return;
    }
    // A scan the watch or Rescan started: shown only if it lasts.
    set({ readStart });
    if (lateScanTimer === null) {
      lateScanTimer = setTimeout(() => {
        lateScanTimer = null;
        if (get().status !== null || get().opening !== null)
          set({ reading: latest ?? p });
      }, LATE_SCAN_SHOW_MS);
    }
    latest = p;
  };

  const onChanged = (reason: string) => {
    if (get().status === null) return;
    if (reason === "status") {
      void get().refresh();
      return;
    }
    // The photo count follows the scan, about once a second.
    if (refreshTimer !== null) return;
    const wait = Math.max(0, lastRefresh + REFRESH_EVERY_MS - Date.now());
    refreshTimer = setTimeout(() => {
      refreshTimer = null;
      void get().refresh();
    }, wait);
  };

  return {
    opening: null,
    status: null,
    recent: [],
    question: null,
    dropOver: false,
    conflictHidden: false,
    noticesHidden: null,
    reading: null,
    readStart: null,

    async start() {
      if (started) return;
      started = true;
      followSession();
      listening = await Promise.all([
        api().listen("scan_progress", onProgress),
        api().listen("album_changed", (e) => onChanged(e.reason)),
        api().listen("open_folder", (e) => void get().openFolder(e.path, e.photo)),
        // The close handshake (docs/08-desktop-and-release.md, "Closing and
        // quitting"): edit mode ends as Done and the session is saved
        // before Rust closes the album.
        api().listen("app_closing", () => void closingWindow()),
        api().listen("folder_drop", (e) => {
          if (e.phase === "over") set({ dropOver: true });
          else set({ dropOver: false });
          if (e.phase === "refused")
            showToast({ kind: "info", message: albumText.dropAFolder });
        }),
      ]);
      const stopStatus = await followEditStatus();
      listening.push(stopStatus);
      listening.push(...(await listenLifecycle()));
      let startup;
      try {
        startup = await api().invoke("app_startup");
      } catch (e) {
        showToast({ kind: "error", message: toAppError(e).message });
        return;
      }
      useSettingsStore.setState({ settings: startup.settings, error: null });
      startLifecycle(startup);
      set({ recent: startup.recent });
      const first = startup.launch_folder ?? startup.reopen;
      if (first !== null) {
        await get().open(first, undefined, startup.launch_folder === null ? null : startup.launch_photo);
        return;
      }
      const newest = startup.recent[0];
      if (
        startup.settings.reopen_last &&
        newest !== undefined &&
        !newest.exists
      ) {
        showToast({
          kind: "info",
          message: albumText.lastFolderMissing(newest.name),
        });
      }
    },

    stop() {
      stopSession();
      for (const unlisten of listening) unlisten();
      listening = [];
    },

    async openFolder(path, photo = null) {
      const open = get().status;
      if (open !== null && open.folder === path && get().opening === null) {
        // The album is open already: only the picture changes.
        if (photo !== null) {
          const call = openCalls;
          const id = await findPhoto(photo);
          if (id !== null && call === openCalls && (await showPhoto(id))) return;
          wantedPhoto = { path: photo, call, tries: 0 };
          void lookForWanted(open.scanning);
        }
        return;
      }
      await get().open(path, undefined, photo);
    },

    async open(path, options, photo = null) {
      // Where the person was in the album still open, while Rust has it open.
      if (get().status !== null) {
        await leaving();
        await flushSession();
      }
      const call = ++openCalls;
      const finishedBefore = scansFinished;
      set({
        opening: { path, name: folderName(path) },
        question: null,
        status: null,
        reading: null,
        readStart: null,
        conflictHidden: false,
        noticesHidden: null,
      });
      latest = null;
      wantedPhoto = null;
      const full: OpenOptions = {
        take_lock: false,
        view_only: false,
        restore_backup: false,
        ...options,
      };
      try {
        const { status, session, notices } = await api().invoke("album_open", {
          path,
          options: full,
        });
        if (call !== openCalls) return;
        let restored = session as SavedSession | null;
        if (photo !== null) {
          // The picture is shown in the grid instead of where the album was
          // left: none of the saved filters or search can hide it.
          const id = await findPhoto(photo);
          if (call !== openCalls) return;
          if (id !== null) restored = { view: "grid", selected_id: id };
          else wantedPhoto = { path: photo, call, tries: 0 };
        }
        // Before the review screen is drawn, so the first query is the restored one.
        restoreSession(restored);
        const { phase, found, done, failed: unread } = status.scan;
        set({
          opening: null,
          reading: status.scanning
            ? { ...idle, phase, found, done, failed: unread }
            : null,
        });
        took(status);
        // A save into a file that could not be finished (docs/06, "The save record and recovery").
        for (const message of notices ?? [])
          showToast({ kind: "error", message });
        // A small folder's scan can end before the answer arrives.
        if (scansFinished !== finishedBefore) await get().refresh();
      } catch (e) {
        if (call !== openCalls) return;
        set({ opening: null, status: null, reading: null });
        failed(path, toAppError(e));
      }
      await loadRecent();
    },

    async pickFolder() {
      let path: string | null;
      try {
        path = await api().invoke("app_pick_folder");
      } catch (e) {
        showToast({ kind: "error", message: toAppError(e).message });
        return;
      }
      if (path !== null) await get().openFolder(path);
    },

    async close() {
      if (get().status !== null) {
        await leaving();
        await flushSession();
      }
      openCalls += 1;
      if (lateScanTimer !== null) clearTimeout(lateScanTimer);
      lateScanTimer = null;
      set({
        opening: null,
        status: null,
        reading: null,
        readStart: null,
        question: null,
      });
      try {
        await api().invoke("album_close");
      } catch (e) {
        showToast({ kind: "error", message: toAppError(e).message });
      }
      await loadRecent();
    },

    async refresh() {
      const call = openCalls;
      lastRefresh = Date.now();
      try {
        const status = await api().invoke("album_status");
        if (call !== openCalls || get().opening !== null) return;
        took(status);
        if (status !== null && wantedPhoto !== null) void lookForWanted(status.scanning);
        if (status !== null && !status.scanning)
          set({ reading: null, readStart: null });
      } catch {
        // The banners keep what they showed; the next change reads it again.
      }
    },

    async removeRecent(root) {
      try {
        set({ recent: await api().invoke("app_recent_remove", { root }) });
      } catch (e) {
        showToast({ kind: "error", message: toAppError(e).message });
      }
    },

    async answerStale(choice) {
      const q = get().question;
      if (q?.kind !== "stale") return;
      set({ question: null });
      await get().open(
        q.path,
        choice === "anyway" ? { take_lock: true } : { view_only: true },
      );
    },

    async answerDamaged(choice) {
      const q = get().question;
      if (q?.kind !== "damaged") return;
      set({ question: null });
      const fromBanner = restoringFromBanner;
      restoringFromBanner = false;
      if (choice === "cancel" && !fromBanner) return;
      await get().open(
        q.path,
        choice === "restore" ? { restore_backup: true } : { view_only: true },
      );
    },

    async askRestore() {
      const status = get().status;
      if (status === null) return;
      // Opening it for changes asks the damaged question again, with the
      // backup's date. Cancel there opens it view-only again.
      restoringFromBanner = true;
      await get().open(status.folder);
      if (get().question?.kind !== "damaged") restoringFromBanner = false;
    },

    hideConflict() {
      set({ conflictHidden: true });
    },

    hideNotices() {
      const status = get().status;
      set({ noticesHidden: status === null ? null : noticeKey(status) });
    },
  };
});
