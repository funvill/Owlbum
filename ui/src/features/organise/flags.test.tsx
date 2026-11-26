import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setApi } from "../../api";
import type { InfoPatch } from "../../api/bindings/InfoPatch";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { handlerGaps, row as keyRow } from "../../keys";
import { useAlbumStore } from "../album";
import { Review } from "../album/Review";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { resetImageCache, type Load } from "../preview";
import { resetViewer } from "../viewer/store";
import { resetFlags } from "./flags";
import { FLAG_KEY_IDS, flagKeyHandlers } from "./keys";

// jsdom has no layout: every zoom stage measures 800 × 600 and everything
// else 1000 × 800. ResizeObserver, which jsdom lacks, is a stub.
class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
const isStage = (el: HTMLElement) => el.classList.contains("ow-zoom");

let fake: FakeApi;
let album: ReturnType<typeof createQueryFake>;
/** The fake album's revision, which its details carry. */
let revision = 0;
const grid = () => useGridStore.getState();
const initialAlbum = useAlbumStore.getState();

function photos(n: number, change: (p: FakePhoto) => FakePhoto = (p) => p): FakePhoto[] {
  return Array.from({ length: n }, (_, i) =>
    change({ id: i + 1, rel_path: `day/p${i + 1}.jpg`, width: 4000, height: 2000 }),
  );
}

const noLoad = (): Load => ({ done: new Promise<void>(() => {}), cancel: () => {} });

/**
 * A fake album whose `photo_set_info` changes the photos' flags and sends
 * `album_changed` (reason `info`), as Rust does.
 */
function setup(list: FakePhoto[] = photos(5)) {
  album = createQueryFake(list);
  revision = 0;
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg`, revision }),
    photo_set_info: ({ ids, patch }) => {
      revision = album.change((all) =>
        all.map((p) =>
          ids.includes(p.id)
            ? {
                ...p,
                ...(patch.star === undefined ? {} : { star: patch.star }),
                ...(patch.hidden === undefined ? {} : { hidden: patch.hidden }),
              }
            : p,
        ),
      );
      const now = revision;
      setTimeout(() => fake.emit("album_changed", { revision: now, reason: "info", rekeyed: [] }), 0);
      return { changed: ids, revision: now };
    },
  });
  setApi(fake);
}

/** Renders the review screen with photo `id` current (and `also` selected too). */
async function show(id: number, also: number[] = []) {
  render(<Review busy={() => false} />);
  await waitFor(() => expect(grid().result?.count).toBeGreaterThan(0));
  await waitFor(() => expect(screen.getAllByRole("gridcell").length).toBeGreaterThan(0));
  act(() =>
    grid().setSelection({ current: { id, index: id - 1 }, anchor: { id, index: id - 1 }, selection: [id, ...also] }),
  );
}

const press = (key: string, init: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(document.activeElement ?? document.body, { key, ...init });

/** The `photo_set_info` calls made so far. */
const sets = () =>
  fake.calls
    .filter((c) => c.command === "photo_set_info")
    .map((c) => c.args as { ids: number[]; patch: InfoPatch });

const mark = () => screen.queryByTestId("flag-mark");
const region = (name: string) => screen.getByRole("region", { name });

beforeEach(() => {
  resetGrid();
  resetViewer();
  resetFlags();
  resetImageCache(noLoad);
  useAlbumStore.setState(initialAlbum, true);
  useSettingsStore.setState({ settings: null, error: null });
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isStage(this) ? 800 : 1000;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return isStage(this) ? 600 : 800;
    },
  });
  setup();
});

afterEach(() => {
  resetGrid();
  resetViewer();
  resetFlags();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("Star and hide keys", () => {
  it("has a handler for every star and hide row", () => {
    const rows = FLAG_KEY_IDS.map((id) => keyRow(id));
    expect(handlerGaps(flagKeyHandlers(), rows)).toEqual({ missing: [], unknown: [] });
  });

  it("F stars the current photo and plays the Starred mark over the preview", async () => {
    await show(2);
    press("f");
    await waitFor(() => expect(sets()).toEqual([{ ids: [2], patch: { star: true } }]));
    const m = within(region("Preview")).getByTestId("flag-mark");
    expect(m).toHaveAccessibleName("Starred");
    expect(m).toHaveClass("ow-flag-mark--starred");
  });

  it("S stars too, and unstars a starred photo with the outline mark", async () => {
    setup(photos(5, (p) => (p.id === 2 ? { ...p, star: true } : p)));
    await show(2);
    press("s");
    await waitFor(() => expect(sets()).toEqual([{ ids: [2], patch: { star: false } }]));
    expect(mark()).toHaveAccessibleName("Star removed");
  });

  it("H hides; the page re-runs with keep and selects kept.current", async () => {
    await show(2);
    press("h");
    await waitFor(() => expect(sets()).toEqual([{ ids: [2], patch: { hidden: true } }]));
    expect(mark()).toHaveAccessibleName("Hidden");
    await waitFor(() => expect(grid().result?.count).toBe(4));
    const run = fake.calls.filter((c) => c.command === "query_run").at(-1)!.args as { keep: { current: unknown } };
    expect(run.keep.current).toEqual({ id: 2, index: 1 });
    // The photo left: the tile now in its place becomes current.
    expect(grid().current).toEqual({ id: 3, index: 1 });
    expect(grid().selection).toEqual([3]);
  });

  it("two quick presses send true then false: the second reads what the first sent", async () => {
    await show(2);
    press("f");
    press("f");
    await waitFor(() => expect(sets()).toHaveLength(2));
    expect(sets().map((s) => s.patch)).toEqual([{ star: true }, { star: false }]);
    expect(mark()).toHaveAccessibleName("Star removed");
  });

  it("gives every selected photo the opposite of the current photo's value", async () => {
    setup(photos(5, (p) => (p.id === 3 ? { ...p, star: true } : p)));
    await show(2, [3, 4]);
    press("f");
    await waitFor(() => expect(sets()).toHaveLength(1));
    expect(sets()[0]!.patch).toEqual({ star: true });
    expect([...sets()[0]!.ids].sort()).toEqual([2, 3, 4]);
  });

  it("the tile's star button changes that photo only, whatever is selected, and plays no mark", async () => {
    await show(2, [3]);
    const button = within(screen.getAllByRole("gridcell")[4]!).getByRole("button", { name: "Star" });
    fireEvent.click(button);
    await waitFor(() => expect(sets()).toEqual([{ ids: [5], patch: { star: true } }]));
    expect(mark()).toBeNull();
    expect(grid().selection.sort()).toEqual([2, 3]);
  });

  it("in the viewer changes the shown photo only and plays the mark over the picture", async () => {
    await show(2, [3, 4]);
    // The viewer keeps the selection it opened with only when it was one photo;
    // put the multi-selection back to prove the viewer ignores it.
    press("Enter");
    const viewer = await screen.findByRole("dialog");
    act(() =>
      grid().setSelection({ current: { id: 2, index: 1 }, anchor: { id: 2, index: 1 }, selection: [2, 3, 4] }),
    );
    press("s");
    await waitFor(() => expect(sets()).toEqual([{ ids: [2], patch: { star: true } }]));
    expect(within(viewer).getByTestId("flag-mark")).toHaveAccessibleName("Starred");
    expect(within(region("Preview")).queryByTestId("flag-mark")).toBeNull();
  });

  it("does nothing while typing in a field or for a held key", async () => {
    await show(2);
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    fireEvent.keyDown(field, { key: "f" });
    field.remove();
    press("f", { repeat: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(sets()).toEqual([]);
    expect(mark()).toBeNull();
  });

  it("does nothing in a view-only album, and the tile's star is not a button", async () => {
    useAlbumStore.setState({ status: fakeStatus("D:/Photos", { view_only: "read_only" }) });
    setup(photos(5, (p) => (p.id === 1 ? { ...p, star: true } : p)));
    await show(2);
    press("f");
    press("h");
    await new Promise((r) => setTimeout(r, 20));
    expect(sets()).toEqual([]);
    expect(mark()).toBeNull();
    expect(screen.queryByRole("button", { name: /^(Star|Unstar)$/ })).toBeNull();
  });

  it("removes the mark after 0.8 s", async () => {
    await show(2);
    press("f");
    await waitFor(() => expect(mark()).not.toBeNull());
    await waitFor(() => expect(mark()).toBeNull(), { timeout: 2000 });
  });
});
