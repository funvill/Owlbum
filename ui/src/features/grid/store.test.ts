import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import {
  createQueryFake,
  type FakePhoto,
  type QueryFake,
} from "../../api/fake/query";
import { defaultQuery } from "../../api/query";
import { useSettingsStore } from "../app/settings";
import {
  RERUN_BUSY_MS,
  RERUN_IDLE_MS,
  SAVE_SIZE_MS,
  gridBlocks,
  resetGrid,
  restoreSession,
  setGridView,
  useGridStore,
} from "./store";

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({
    id: i + 1,
    rel_path: `p${i + 1}.jpg`,
    date: `2024-01-01T00:00:${String(i).padStart(2, "0")}`,
  }));
}

let album: QueryFake;
let fake: FakeApi;
let busy = false;

const runs = () => fake.calls.filter((c) => c.command === "query_run");
const grid = () => useGridStore.getState();

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  album = createQueryFake(photos(10));
  fake = createFakeApi(album.handlers);
  setApi(fake);
  busy = false;
});

afterEach(() => {
  resetGrid();
  vi.useRealTimers();
});

describe("grid store", () => {
  it("runs the default query when it starts, once", async () => {
    await grid().start();
    await grid().start();
    expect(runs()).toHaveLength(1);
    expect(runs()[0]!.args).toEqual({ query: defaultQuery(), keep: null });
    expect(grid().result?.count).toBe(10);
    expect(gridBlocks().token).toBe(grid().result?.token);
    expect(gridBlocks().count).toBe(10);
  });

  it("makes one call for changes within one frame, with the last query", async () => {
    await grid().start();
    grid().setQuery({ ...defaultQuery(), sort: "name" });
    grid().setQuery({ ...defaultQuery(), sort: "date" });
    grid().setQuery({ ...defaultQuery(), sort: "name" });
    await vi.waitFor(() => expect(runs()).toHaveLength(2));
    await new Promise((r) => setTimeout(r, 50));
    expect(runs()).toHaveLength(2);
    expect(runs()[1]!.args).toMatchObject({ query: { sort: "name" } });
    expect(grid().query.sort).toBe("name");
  });

  it("makes the first photo current when the album opens and its row arrives", async () => {
    await grid().start();
    expect(grid().pending).toMatchObject({ index: 0, wait: "row" });
    gridBlocks().want({
      first: 0,
      last: 9,
      span: 10,
      direction: 1,
      current: null,
      anchor: null,
    });
    await vi.waitFor(() => expect(grid().current).toEqual({ id: 1, index: 0 }));
    expect(grid().selection).toEqual([1]);
    expect(grid().pending).toBeNull();
  });

  it("restores a saved session: its query runs first and its photo becomes current", async () => {
    restoreSession({
      view: "map",
      sort: "name",
      filter: { hidden: "show" },
      search: null,
      selected_id: 7,
      scroll: 0,
    });
    await grid().start();
    expect(runs()).toHaveLength(1);
    // A saved map view opens on the map (#36).
    expect(runs()[0]!.args).toMatchObject({
      query: { view: "map", sort: "name", filter: { hidden: "show" } },
    });
    await vi.waitFor(() => expect(grid().restored).toBe(true));
    expect(grid().current).toMatchObject({ id: 7 });
    expect(grid().selection).toEqual([7]);
    expect(grid().scroll).toMatchObject({ to: "current" });
    // The first photo was not picked instead.
    gridBlocks().want({
      first: 0,
      last: 9,
      span: 10,
      direction: 1,
      current: null,
      anchor: null,
    });
    await new Promise((r) => setTimeout(r, 20));
    expect(grid().current?.id).toBe(7);
  });

  it("restores the photo at the scroll index when the saved photo is gone", async () => {
    restoreSession({ sort: "date", selected_id: 99, scroll: 5 });
    await grid().start();
    await vi.waitFor(() =>
      expect(grid().pending).toMatchObject({ index: 5, wait: "row" }),
    );
    gridBlocks().want({
      first: 0,
      last: 9,
      span: 10,
      direction: 1,
      current: 5,
      anchor: null,
    });
    await vi.waitFor(() => expect(grid().current).toEqual({ id: 6, index: 5 }));
    expect(grid().restored).toBe(true);
  });

  it("restores a session only once, and not after the album closed", async () => {
    restoreSession({ sort: "name", selected_id: null });
    resetGrid();
    await grid().start();
    expect(runs()[0]!.args).toMatchObject({ query: { sort: "date" } });
  });

  it("keeps a photo picked before the rows arrived instead of the first", async () => {
    await grid().start();
    grid().setSelection({
      current: { id: 4, index: 3 },
      anchor: { id: 4, index: 3 },
      selection: [4],
    });
    gridBlocks().want({
      first: 0,
      last: 9,
      span: 10,
      direction: 1,
      current: null,
      anchor: null,
    });
    grid().setQuery({ ...defaultQuery(), sort: "name" });
    await vi.waitFor(() => expect(runs()).toHaveLength(2));
    await new Promise((r) => setTimeout(r, 20));
    expect(grid().current?.id).toBe(4);
  });

  it("goes to the top after a change the user made with no current photo", async () => {
    await grid().start();
    const seq = grid().scroll?.seq ?? 0;
    grid().setQuery({ ...defaultQuery(), sort: "name" });
    await vi.waitFor(() => expect(grid().scroll).toMatchObject({ to: "top" }));
    expect(grid().scroll!.seq).toBeGreaterThan(seq);
  });

  it("follows the current photo and the selection into the new result", async () => {
    await grid().start();
    grid().setSelection({
      current: { id: 3, index: 2 },
      anchor: { id: 3, index: 2 },
      selection: [3, 5],
    });
    grid().setQuery({ ...defaultQuery(), sort: "name" });
    await vi.waitFor(() => expect(runs()).toHaveLength(2));
    expect(runs()[1]!.args).toMatchObject({
      keep: { current: { id: 3, index: 2 }, anchor_id: 3, selection: [3, 5] },
    });
    await vi.waitFor(() =>
      expect(grid().scroll).toMatchObject({ to: "current" }),
    );
    expect(grid().current?.id).toBe(3);
    expect(grid().selection).toEqual([3, 5]);
  });

  it("settles the current photo when it leaves the result", async () => {
    await grid().start();
    grid().setSelection({
      current: { id: 3, index: 2 },
      anchor: null,
      selection: [3],
    });
    const revision = album.change((ps) =>
      ps.map((p) => (p.id === 3 ? { ...p, hidden: true } : p)),
    );
    fake.emit("album_changed", { reason: "info", revision });
    await vi.waitFor(() => expect(grid().current).toEqual({ id: 4, index: 2 }));
    expect(grid().selection).toEqual([4]);
    expect(grid().anchor).toEqual({ id: 4, index: 2 });
  });

  it("keeps the scroll position after album_changed", async () => {
    await grid().start();
    const before = grid().scroll;
    const revision = album.change((ps) => [
      ...ps,
      { id: 99, rel_path: "new.jpg" },
    ]);
    fake.emit("album_changed", { reason: "scan", revision });
    await vi.waitFor(() => expect(grid().result?.count).toBe(11));
    expect(grid().scroll).toEqual(before);
  });

  it("keeps the grid's place by the tile in view while rows arrive before it", async () => {
    await grid().start();
    // Photo 5 is first in view, at index 4; two photos taken earlier arrive.
    setGridView(() => ({ id: 5, index: 4 }));
    const revision = album.change((ps) => [
      ...ps,
      { id: 50, rel_path: "a.jpg", date: "2023-01-01T00:00:00" },
      { id: 51, rel_path: "b.jpg", date: "2023-01-02T00:00:00" },
    ]);
    fake.emit("album_changed", { reason: "scan", revision });
    await vi.waitFor(() => expect(grid().result?.count).toBe(12));
    expect(runs()[1]!.args).toMatchObject({
      keep: { view: { id: 5, index: 4 } },
    });
    expect(grid().result?.kept?.view).toEqual({ id: 5, index: 6 });
    expect(grid().follow).toMatchObject({ from: 4, to: 6 });
  });

  it("keeps no place by a tile after a change the user made", async () => {
    await grid().start();
    setGridView(() => ({ id: 5, index: 4 }));
    grid().setQuery({ ...defaultQuery(), sort: "name" });
    await vi.waitFor(() => expect(runs()).toHaveLength(2));
    expect(runs()[1]!.args).toMatchObject({ keep: { view: null } });
    expect(grid().follow).toBeNull();
  });

  it("re-runs at most once per 100 ms when nothing is running, and never loses the last change", async () => {
    vi.useFakeTimers();
    const started = grid().start({ busy: () => busy });
    await vi.advanceTimersByTimeAsync(0);
    await started;
    for (let i = 0; i < 5; i++) {
      const revision = album.change((ps) => [
        ...ps,
        { id: 100 + i, rel_path: `n${i}.jpg` },
      ]);
      fake.emit("album_changed", { reason: "scan", revision });
      await vi.advanceTimersByTimeAsync(10);
    }
    expect(runs().length).toBeLessThanOrEqual(2);
    await vi.advanceTimersByTimeAsync(RERUN_IDLE_MS);
    expect(runs()).toHaveLength(3);
    expect(grid().result?.count).toBe(15);
  });

  it("re-runs at most once per second while a scan is running", async () => {
    vi.useFakeTimers();
    busy = true;
    const started = grid().start({ busy: () => busy });
    await vi.advanceTimersByTimeAsync(0);
    await started;
    for (let i = 0; i < 10; i++) {
      const revision = album.change((ps) => [
        ...ps,
        { id: 100 + i, rel_path: `n${i}.jpg` },
      ]);
      fake.emit("album_changed", { reason: "scan", revision });
      await vi.advanceTimersByTimeAsync(RERUN_BUSY_MS / 10);
    }
    // One at once, then one a second later.
    expect(runs().length).toBeLessThanOrEqual(3);
    await vi.advanceTimersByTimeAsync(RERUN_BUSY_MS);
    expect(grid().result?.count).toBe(20);
  });

  it("skips an event whose revision is not newer, and reasons that need no re-run", async () => {
    await grid().start();
    fake.emit("album_changed", { reason: "scan", revision: 0 });
    const revision = album.change((ps) => ps);
    fake.emit("album_changed", { reason: "tidy", revision });
    fake.emit("album_changed", { reason: "status", revision });
    await new Promise((r) => setTimeout(r, RERUN_IDLE_MS * 2));
    expect(runs()).toHaveLength(1);
  });

  it("asks for the rows on screen again after an edit", async () => {
    await grid().start();
    const refresh = vi.spyOn(gridBlocks(), "refresh");
    const revision = album.change((ps) => ps);
    fake.emit("album_changed", { reason: "edit", revision });
    expect(refresh).toHaveBeenCalled();
    expect(runs()).toHaveLength(1);
  });

  it("keeps the error of a failed run", async () => {
    fake.handle("query_run", () => {
      throw appError("bad_query", "Owlbum can't show filtered photos yet.");
    });
    await grid().start();
    expect(grid().error?.code).toBe("bad_query");
  });

  it("stops following album_changed when stopped", async () => {
    await grid().start();
    grid().stop();
    const revision = album.change((ps) => ps);
    fake.emit("album_changed", { reason: "scan", revision });
    await new Promise((r) => setTimeout(r, RERUN_IDLE_MS * 2));
    expect(runs()).toHaveLength(1);
  });
});

describe("thumbnail size", () => {
  it("starts from the setting", async () => {
    await useSettingsStore.getState().load();
    useSettingsStore.setState((s) => ({
      settings: { ...s.settings!, thumb_size: 200 },
    }));
    await grid().start();
    expect(grid().thumbSize).toBe(200);
  });

  it("changes at once, clamped, and is saved 300 ms after the slider last moved", async () => {
    vi.useFakeTimers();
    grid().setThumbSize(170);
    grid().setThumbSize(500);
    expect(grid().thumbSize).toBe(320);
    await vi.advanceTimersByTimeAsync(SAVE_SIZE_MS - 50);
    grid().setThumbSize(240);
    await vi.advanceTimersByTimeAsync(SAVE_SIZE_MS - 1);
    expect(
      fake.calls.filter((c) => c.command === "app_settings_set"),
    ).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);
    const saves = fake.calls.filter((c) => c.command === "app_settings_set");
    expect(saves).toEqual([
      { command: "app_settings_set", args: { patch: { thumb_size: 240 } } },
    ]);
  });
});

describe("closing the album", () => {
  it("draws none of the closed album's rows in the next one", async () => {
    await grid().start();
    gridBlocks().want({
      first: 0,
      last: 9,
      span: 10,
      direction: 1,
      current: null,
      anchor: null,
    });
    await vi.waitFor(() =>
      expect(gridBlocks().row(0)?.rel_path).toBe("p1.jpg"),
    );
    grid().stop();
    const next = createQueryFake([{ id: 50, rel_path: "other.jpg" }]);
    fake = createFakeApi({
      ...next.handlers,
      query_window: () => new Promise(() => {}),
    });
    setApi(fake);
    await grid().start();
    expect(grid().result?.count).toBe(1);
    expect(gridBlocks().row(0)).toBeUndefined();
  });
});

describe("expired tokens", () => {
  it("runs the query again and repeats the window, without spinning when it keeps expiring", async () => {
    await grid().start();
    fake.handle("query_window", () => {
      throw appError(
        "query_expired",
        "The list of photos changed. Showing it again.",
      );
    });
    gridBlocks().want({
      first: 0,
      last: 9,
      span: 10,
      direction: 1,
      current: null,
      anchor: null,
    });
    await new Promise((r) => setTimeout(r, RERUN_IDLE_MS * 3 + 50));
    // At most one re-run per 100 ms, however often the token expires.
    expect(runs().length).toBeGreaterThan(1);
    expect(runs().length).toBeLessThanOrEqual(5);
    expect(grid().error).toBeNull();
  });
});
