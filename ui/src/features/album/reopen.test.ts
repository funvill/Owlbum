import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { defaultQuery } from "../../api/query";
import { useSettingsStore } from "../app/settings";
import { resetGrid, sessionOf, useGridStore, withSession } from "../grid";
import {
  SESSION_SAVE_MS,
  flushSession,
  followSession,
  stopSession,
} from "./reopen";
import { resetAlbumSession, useAlbumStore } from "./session";

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({
    id: i + 1,
    rel_path: `p${i + 1}.jpg`,
    date: `2024-01-01T00:00:${String(i).padStart(2, "0")}`,
  }));
}

let fake: FakeApi;
const grid = () => useGridStore.getState();
const saves = () =>
  fake.calls
    .filter((c) => c.command === "app_session_save")
    .map((c) => (c.args as { session: unknown }).session);

beforeEach(() => {
  resetGrid();
  resetAlbumSession();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi({
    ...createQueryFake(photos(10)).handlers,
    album_open: ({ path }) => ({
      status: fakeStatus(path, { photos: 10, scanning: false }),
      session:
        path === "/b" ? { sort: "name", selected_id: 3, scroll: 0 } : null,
    }),
  });
  setApi(fake);
});

afterEach(() => {
  stopSession();
  resetGrid();
  resetAlbumSession();
  vi.useRealTimers();
});

describe("the session's shape", () => {
  it("keeps the album's query fields, the current photo and the scroll index", () => {
    const q = {
      ...defaultQuery(),
      sort: "name" as const,
      open_stacks: [4],
      want_facets: true,
      search: "sea",
    };
    expect(sessionOf(q, 7, 30)).toEqual({
      view: "grid",
      sort: "name",
      filter: {},
      search: "sea",
      selected_id: 7,
      scroll: 30,
    });
  });

  it("puts only the Query's own fields back, and not the settings or the open stacks", () => {
    const q = withSession(defaultQuery(), {
      view: "timeline",
      sort: "score",
      search: "dog",
      timeline_unit: "month",
      open_stacks: [1],
      selected_id: 2,
      scroll: 9,
      later: 1,
    } as never);
    expect(q).toEqual({
      ...defaultQuery(),
      view: "timeline",
      sort: "score",
      search: "dog",
    });
  });
});

describe("saving the session", () => {
  it("saves 1,000 ms after the last change, once", async () => {
    followSession();
    await grid().start();
    await vi.waitFor(() => expect(grid().restored).toBe(true));
    vi.useFakeTimers();
    grid().setSelection({
      current: { id: 4, index: 3 },
      anchor: { id: 4, index: 3 },
      selection: [4],
    });
    vi.advanceTimersByTime(SESSION_SAVE_MS - 10);
    grid().setSelection({
      current: { id: 5, index: 4 },
      anchor: { id: 5, index: 4 },
      selection: [5],
    });
    vi.advanceTimersByTime(SESSION_SAVE_MS - 10);
    expect(saves()).toHaveLength(0);
    vi.advanceTimersByTime(20);
    expect(saves()).toEqual([
      expect.objectContaining({ sort: "date", selected_id: 5 }),
    ]);
    // Nothing changed since: closing saves nothing more.
    vi.useRealTimers();
    await flushSession();
    expect(saves()).toHaveLength(1);
  });

  it("does not save before the album's session is restored", async () => {
    followSession();
    await flushSession();
    expect(saves()).toHaveLength(0);
  });

  it("saves the open album at once before another opens, and restores the new one's", async () => {
    await useAlbumStore.getState().start();
    await useAlbumStore.getState().open("/a");
    await grid().start();
    await vi.waitFor(() => expect(grid().restored).toBe(true));
    grid().setSelection({
      current: { id: 4, index: 3 },
      anchor: { id: 4, index: 3 },
      selection: [4],
    });

    await useAlbumStore.getState().open("/b");
    const order = fake.calls
      .map((c) => c.command)
      .filter((c) => c === "app_session_save" || c === "album_open");
    expect(order).toEqual(["album_open", "app_session_save", "album_open"]);
    expect(saves()[0]).toMatchObject({ selected_id: 4 });

    // The shell stops the grid when the album changes; the new one starts restored.
    grid().stop();
    await grid().start();
    expect(grid().query.sort).toBe("name");
    await vi.waitFor(() => expect(grid().current).toMatchObject({ id: 3 }));
  });
});
