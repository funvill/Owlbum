import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { AppError, Recent } from "../../api";
import type { AlbumStatus } from "../../api/bindings/AlbumStatus";
import type { ScanProgress } from "../../api/bindings/ScanProgress";
import { appError, createFakeApi, defaultHandlers } from "../../api/fake";
import type { FakeApi, FakeHandlers } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { defaultSettings } from "../../api/fake/app";
import { Toasts, useToasts } from "../../controls";
import { albumText } from "../../text/album";
import { seriousFindings } from "../../test/axe";
import { createQueryFake } from "../../api/fake/query";
import { gridText } from "../../text/grid";
import { resetGrid, useGridStore } from "../grid";
import { Shell } from "./Shell";
import { LATE_SCAN_SHOW_MS, resetAlbumSession, useAlbumStore } from "./session";

const initial = useAlbumStore.getState();

beforeEach(() => {
  resetGrid();
  resetAlbumSession();
  useAlbumStore.setState(initial, true);
  useToasts.setState({ toasts: [] });
});

/** A promise and its resolve/reject, for an answer a test hands out later. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function recent(root: string, exists = true): Recent {
  return { root, name: root.split("\\").pop() ?? root, opened_at: 0, exists };
}

/** Renders the window with the fake backend and waits for start-up. */
async function shell(overrides: Partial<FakeHandlers> = {}): Promise<FakeApi> {
  const fake = createFakeApi(overrides);
  setApi(fake);
  render(
    <>
      <Shell />
      <Toasts />
    </>,
  );
  await waitFor(() => expect(fake.calls.some((c) => c.command === "app_startup")).toBe(true));
  await act(async () => {});
  return fake;
}

/** The body's message, when it shows one. */
async function bodySays(text: string) {
  await waitFor(() => expect(document.querySelector(".ow-body__message")).toHaveTextContent(text));
}

function opens(fake: FakeApi) {
  return fake.calls.filter((c) => c.command === "album_open").map((c) => c.args);
}

const progress = (p: Partial<ScanProgress>): ScanProgress => ({
  phase: "reading",
  found: 0,
  done: 0,
  failed: 0,
  finished: false,
  cancelled: false,
  ...p,
});

/** A fake whose album_open answers `status` for any path, and album_status the same. */
function withStatus(status: AlbumStatus, extra: Partial<FakeHandlers> = {}): Partial<FakeHandlers> {
  return {
    album_open: () => ({ status, session: null }),
    album_status: () => status,
    ...extra,
  };
}

describe("start screen", () => {
  it("shows the heading, Open folder and the recent list, with folders not found greyed out", async () => {
    const fake = await shell({
      app_startup: (a) => ({
        ...(defaultHandlers.app_startup(a) as Awaited<ReturnType<FakeHandlers["app_startup"]>>),
        recent: [recent("D:\\Photos\\Trip"), recent("E:\\Old\\Album", false)],
        settings: { ...defaultSettings, reopen_last: false },
      }),
    });
    expect(screen.getByRole("heading", { level: 1, name: albumText.startHeading })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: albumText.openFolder })).toBeInTheDocument();
    const list = screen.getByRole("list");
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    const gone = within(list).getByRole("button", { name: /E:\\Old\\Album/ });
    expect(gone).toHaveAttribute("aria-disabled", "true");
    expect(gone).toHaveTextContent(albumText.notFound);

    await userEvent.click(gone);
    expect(opens(fake)).toEqual([]);

    await userEvent.click(within(list).getByRole("button", { name: /D:\\Photos\\Trip/ }));
    expect(opens(fake)).toEqual([
      { path: "D:\\Photos\\Trip", options: { take_lock: false, view_only: false, restore_backup: false } },
    ]);
    expect(await seriousFindings(document.body)).toEqual([]);
  });

  it("removes a recent entry with its x and touches nothing else", async () => {
    const fake = await shell({
      app_startup: (a) => ({
        ...(defaultHandlers.app_startup(a) as Awaited<ReturnType<FakeHandlers["app_startup"]>>),
        recent: [recent("D:\\Photos\\Trip", false)],
      }),
      app_recent_remove: () => [],
    });
    await userEvent.click(screen.getByRole("button", { name: albumText.removeRecent }));
    expect(fake.calls.filter((c) => c.command === "app_recent_remove")).toEqual([
      { command: "app_recent_remove", args: { root: "D:\\Photos\\Trip" } },
    ]);
    expect(screen.queryByRole("list")).toBeNull();
    expect(opens(fake)).toEqual([]);
  });
});

describe("opening a folder", () => {
  it("opens the folder the dialog picked, once, and a cancelled dialog does nothing", async () => {
    let picked: string | null = null;
    const fake = await shell({ app_pick_folder: () => picked });
    await userEvent.click(screen.getByRole("button", { name: albumText.openFolder }));
    expect(opens(fake)).toEqual([]);
    picked = "D:\\Photos";
    await userEvent.click(screen.getByRole("button", { name: albumText.openFolder }));
    expect(opens(fake)).toHaveLength(1);
    await bodySays(albumText.lookingForPhotos);
  });

  it("shows Opening… until album_open answers, then never the filter's empty text", async () => {
    const answer = deferred<{ status: AlbumStatus; session: null }>();
    const fake = await shell({ album_open: () => answer.promise });
    act(() => fake.emit("open_folder", { path: "D:\\Photos\\Trip" }));
    expect(await screen.findByText(albumText.opening("Trip"))).toBeInTheDocument();
    expect(screen.queryByText(/No photos match/)).toBeNull();
    await act(async () => answer.resolve({ status: fakeStatus("D:\\Photos\\Trip"), session: null }));
    await bodySays(albumText.lookingForPhotos);
  });

  it("goes from Looking for photos to No photos in this folder when the scan ends empty", async () => {
    let status = fakeStatus("D:\\Empty");
    const fake = await shell({ album_open: () => ({ status, session: null }), album_status: () => status });
    act(() => fake.emit("open_folder", { path: "D:\\Empty" }));
    await bodySays(albumText.lookingForPhotos);
    status = { ...status, scanning: false };
    act(() => fake.emit("scan_progress", progress({ phase: "idle", finished: true })));
    expect(await screen.findByText(albumText.noPhotos)).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("shows the review screen as soon as the album holds photos, before the scan ends", async () => {
    let status = fakeStatus("D:\\Trip");
    const fake = await shell({
      album_open: () => ({ status, session: null }),
      album_status: () => status,
    });
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    await bodySays(albumText.lookingForPhotos);
    status = { ...status, photos: 3 };
    act(() => fake.emit("album_changed", { revision: 1, reason: "scan" }));
    expect(await screen.findByRole("region", { name: albumText.photosRegion }, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
  });

  it("does nothing when the folder already open is opened again", async () => {
    const fake = await shell();
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    await bodySays(albumText.lookingForPhotos);
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    await act(async () => {});
    expect(opens(fake)).toHaveLength(1);
  });

  it("ignores the answer of an older open", async () => {
    const first = deferred<{ status: AlbumStatus; session: null }>();
    const fake = await shell({
      album_open: ({ path }) =>
        path === "D:\\Old" ? first.promise : { status: fakeStatus(path, { photos: 2 }), session: null },
    });
    act(() => fake.emit("open_folder", { path: "D:\\Old" }));
    act(() => fake.emit("open_folder", { path: "D:\\New" }));
    await screen.findByRole("region", { name: albumText.photosRegion });
    await act(async () => first.resolve({ status: fakeStatus("D:\\Old"), session: null }));
    expect(useAlbumStore.getState().status?.folder).toBe("D:\\New");
  });

  it("reads the status again when the scan ended before album_open answered", async () => {
    const answer = deferred<{ status: AlbumStatus; session: null }>();
    const done = fakeStatus("D:\\Small", { scanning: false, photos: 0 });
    const fake = await shell({ album_open: () => answer.promise, album_status: () => done });
    act(() => fake.emit("open_folder", { path: "D:\\Small" }));
    await screen.findByText(albumText.opening("Small"));
    act(() => fake.emit("scan_progress", progress({ phase: "idle", finished: true })));
    await act(async () => answer.resolve({ status: fakeStatus("D:\\Small"), session: null }));
    expect(await screen.findByText(albumText.noPhotos)).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("shows a toast and stays on the start screen when the folder cannot be opened", async () => {
    await shell({
      album_open: () => {
        throw appError("not_a_folder", "Not a readable folder: D:\\Gone");
      },
    });
    act(() => void useAlbumStore.getState().openFolder("D:\\Gone"));
    expect(await screen.findByText(albumText.couldNotOpen("Gone", "Not a readable folder: D:\\Gone"))).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: albumText.startHeading })).toBeInTheDocument();
  });

  it("shows the drop outline while a drag is over the window, and a toast for a drop of nothing", async () => {
    const fake = await shell();
    act(() => fake.emit("folder_drop", { phase: "over" }));
    expect(screen.getByText(albumText.dropHere)).toBeInTheDocument();
    act(() => fake.emit("folder_drop", { phase: "left" }));
    expect(screen.queryByText(albumText.dropHere)).toBeNull();
    act(() => fake.emit("folder_drop", { phase: "refused" }));
    expect(await screen.findByText(albumText.dropAFolder)).toBeInTheDocument();
  });

  it("closes the folder and shows the start screen", async () => {
    const fake = await shell();
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    await userEvent.click(await screen.findByRole("button", { name: albumText.closeFolder }));
    expect(fake.calls.some((c) => c.command === "album_close")).toBe(true);
    expect(screen.getByRole("heading", { level: 1, name: albumText.startHeading })).toBeInTheDocument();
  });
});

describe("the grid in the window", () => {
  it("fills the review screen, has its size slider in the top bar, and lets its result go on close", async () => {
    const album = createQueryFake([
      { id: 1, rel_path: "a.jpg", date: "2024-01-01T00:00:01" },
      { id: 2, rel_path: "b.jpg", date: "2024-01-01T00:00:02" },
    ]);
    const fake = await shell({
      ...album.handlers,
      album_open: () => ({ status: { ...fakeStatus("D:\\Trip"), photos: 2 }, session: null }),
    });
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    const region = await screen.findByRole("region", { name: albumText.photosRegion });
    const grid = await within(region).findByRole("grid", { name: gridText.label });
    await waitFor(() => expect(within(grid).getAllByRole("gridcell")).toHaveLength(2));
    expect(within(screen.getByRole("banner")).getByRole("slider", { name: gridText.thumbSize })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: albumText.closeFolder }));
    await waitFor(() => expect(useGridStore.getState().result).toBeNull());
  });
});

describe("questions when opening", () => {
  const stale: AppError = appError("album_lock_stale", "This album looks open on STUDIO-PC, last seen 2026-11-20 14:03.", {
    host: "STUDIO-PC",
    last_seen: 0,
  });

  function asking(error: AppError): Partial<FakeHandlers> {
    return {
      album_open: ({ options }) => {
        if (options === null || (!options.take_lock && !options.view_only && !options.restore_backup)) throw error;
        return { status: fakeStatus("D:\\Trip"), session: null };
      },
    };
  }

  it("asks about a stale lock with Open view-only focused, and Esc chooses it", async () => {
    const fake = await shell(asking(stale));
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(stale.message)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: albumText.openViewOnly })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(opens(fake).at(-1)).toEqual({
      path: "D:\\Trip",
      options: { take_lock: false, view_only: true, restore_backup: false },
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("opens anyway by taking the lock", async () => {
    const fake = await shell(asking(stale));
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    await userEvent.click(await screen.findByRole("button", { name: albumText.openAnyway }));
    expect(opens(fake).at(-1)).toMatchObject({ options: { take_lock: true } });
  });

  it("offers to restore a damaged database's backup, and Cancel does nothing more", async () => {
    const damaged = appError("album_damaged", "This album's database is damaged. Owlbum can restore the backup from 2026-11-24.", {
      backup_date: "2026-11-24",
    });
    const fake = await shell(asking(damaged));
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: albumText.cancel })).toHaveFocus();
    await userEvent.click(within(dialog).getByRole("button", { name: albumText.restoreBackup }));
    expect(opens(fake).at(-1)).toMatchObject({ options: { restore_backup: true } });

    act(() => fake.emit("open_folder", { path: "D:\\Other" }));
    await userEvent.click(await screen.findByRole("button", { name: albumText.cancel }));
    expect(opens(fake)).toHaveLength(3);
    expect(screen.getByRole("heading", { level: 1, name: albumText.startHeading })).toBeInTheDocument();
  });

  it("has no Restore backup when there is none", async () => {
    const fake = await shell(asking(appError("album_damaged", "No backup.", { backup_date: null })));
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("button", { name: albumText.restoreBackup })).toBeNull();
    await userEvent.click(within(dialog).getByRole("button", { name: albumText.openViewOnly }));
    expect(opens(fake).at(-1)).toMatchObject({ options: { view_only: true } });
  });
});

describe("start-up", () => {
  it("opens the launch folder, else the folder to reopen", async () => {
    const fake = await shell({
      app_startup: (a) => ({
        ...(defaultHandlers.app_startup(a) as Awaited<ReturnType<FakeHandlers["app_startup"]>>),
        launch_folder: "D:\\Launch",
        reopen: null,
      }),
    });
    expect(opens(fake)).toEqual([{ path: "D:\\Launch", options: expect.anything() }]);
  });

  it("reopens the newest recent folder", async () => {
    const fake = await shell({
      app_startup: (a) => ({
        ...(defaultHandlers.app_startup(a) as Awaited<ReturnType<FakeHandlers["app_startup"]>>),
        reopen: "D:\\Last",
        recent: [recent("D:\\Last")],
      }),
    });
    expect(opens(fake).map((o) => (o as { path: string }).path)).toEqual(["D:\\Last"]);
  });

  it("says when the last folder is not available, and shows the start screen", async () => {
    await shell({
      app_startup: (a) => ({
        ...(defaultHandlers.app_startup(a) as Awaited<ReturnType<FakeHandlers["app_startup"]>>),
        recent: [recent("E:\\Old\\Album", false)],
      }),
    });
    expect(await screen.findByText(albumText.lastFolderMissing("Album"))).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: albumText.startHeading })).toBeInTheDocument();
  });
});

describe("top bar and banners", () => {
  async function openWith(status: AlbumStatus, extra: Partial<FakeHandlers> = {}) {
    const fake = await shell(withStatus(status, extra));
    act(() => fake.emit("open_folder", { path: status.folder }));
    await screen.findByRole("button", { name: albumText.closeFolder });
    return fake;
  }

  const cases: [Partial<AlbumStatus>, string, string | null][] = [
    [{ view_only: "locked", lock_host: "STUDIO-PC" }, albumText.viewOnlyLocked("STUDIO-PC"), albumText.tryAgain],
    [{ view_only: "read_only" }, albumText.viewOnlyReadOnly, null],
    [{ view_only: "read_only", in_memory: true }, albumText.viewOnlyInMemory, null],
    [{ view_only: "newer" }, albumText.viewOnlyNewer, null],
    [{ view_only: "damaged" }, albumText.viewOnlyDamaged, albumText.restoreBackupEllipsis],
    [{ view_only: "chosen", lock_host: "STUDIO-PC" }, albumText.viewOnlyChosenHost("STUDIO-PC"), albumText.openForChanges],
    [{ view_only: "chosen" }, albumText.viewOnlyChosen, albumText.openForChanges],
  ];

  it.each(cases)("shows the banner for %o", async (fields, text, action) => {
    await openWith(fakeStatus("D:\\Trip", fields));
    expect(screen.getByText(text)).toBeInTheDocument();
    expect(screen.getByText(albumText.viewOnlyBadge)).toBeInTheDocument();
    if (action !== null) expect(screen.getByRole("button", { name: action })).toBeInTheDocument();
  });

  it("re-opens for changes: with the lock taken when another computer may have it", async () => {
    const fake = await openWith(fakeStatus("D:\\Trip", { view_only: "chosen", lock_host: "STUDIO-PC" }));
    await userEvent.click(screen.getByRole("button", { name: albumText.openForChanges }));
    expect(opens(fake).at(-1)).toMatchObject({ path: "D:\\Trip", options: { take_lock: true } });
  });

  it("shows no banner and no badge for an album that can be changed", async () => {
    await openWith(fakeStatus("D:\\Trip"));
    expect(screen.queryByText(albumText.viewOnlyBadge)).toBeNull();
    expect(screen.queryByRole("status", { name: /View only/ })).toBeNull();
  });

  it("names the conflict copy and hides it when dismissed", async () => {
    await openWith(fakeStatus("D:\\Trip", { conflict_file: "owlbum (conflict).sqlite" }));
    const text = albumText.conflict("owlbum (conflict).sqlite");
    expect(screen.getByText(text)).toBeInTheDocument();
    const banner = screen.getByText(text).closest(".ow-banner") as HTMLElement;
    await userEvent.click(within(banner).getByRole("button"));
    expect(screen.queryByText(text)).toBeNull();
  });

  it("shows the root and the scope of a scoped open, and Show whole album opens the root", async () => {
    const fake = await openWith(
      fakeStatus("D:\\Photos\\2024\\Trip", { root: "D:\\Photos", scope: "2024/Trip", name: "Trip" }),
    );
    expect(screen.getByText("D:\\Photos")).toBeInTheDocument();
    expect(screen.getByText("2024\\Trip")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: albumText.showWholeAlbum }));
    expect(opens(fake).at(-1)).toMatchObject({ path: "D:\\Photos" });
  });

  it("reads the status again when album_changed says it changed", async () => {
    let status = fakeStatus("D:\\Trip", { scanning: false });
    const fake = await shell({ album_open: () => ({ status, session: null }), album_status: () => status });
    act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
    await screen.findByRole("button", { name: albumText.closeFolder });
    status = { ...status, view_only: "locked", lock_host: "THERE" };
    act(() => fake.emit("album_changed", { revision: 1, reason: "status" }));
    expect(await screen.findByText(albumText.viewOnlyLocked("THERE"))).toBeInTheDocument();
  });

  it("says once per session that the folder can't be watched", async () => {
    const status = fakeStatus("D:\\Share", {
      scanning: false,
      scan: { ...fakeStatus("x").scan, watching: false },
    });
    const fake = await openWith(status);
    expect(await screen.findByText(albumText.notWatching)).toBeInTheDocument();
    useToasts.setState({ toasts: [] });
    act(() => fake.emit("album_changed", { revision: 1, reason: "status" }));
    act(() => fake.emit("open_folder", { path: "D:\\Other" }));
    await act(async () => {});
    expect(screen.queryByText(albumText.notWatching)).toBeNull();
  });
});

describe("reading bar", () => {
  it("shows a later scan only once it has run for a second", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const fake = await shell(withStatus(fakeStatus("D:\\Trip", { scanning: false, photos: 4 })));
      act(() => fake.emit("open_folder", { path: "D:\\Trip" }));
      await screen.findByRole("region", { name: albumText.photosRegion });
      expect(screen.queryByRole("progressbar")).toBeNull();

      act(() => fake.emit("scan_progress", progress({ phase: "walking" })));
      act(() => vi.advanceTimersByTime(LATE_SCAN_SHOW_MS - 100));
      expect(screen.queryByRole("progressbar")).toBeNull();
      act(() => fake.emit("scan_progress", progress({ phase: "idle", finished: true })));
      act(() => vi.advanceTimersByTime(1000));
      expect(screen.queryByRole("progressbar")).toBeNull();

      act(() => fake.emit("scan_progress", progress({ phase: "reading", found: 10, done: 2 })));
      act(() => vi.advanceTimersByTime(LATE_SCAN_SHOW_MS));
      expect(screen.getByRole("progressbar")).toBeInTheDocument();
      expect(screen.getByText(albumText.scanReading(2, 10))).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});
