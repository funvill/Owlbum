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
import { resetNudges, toastChange, toastText } from "./nudge";
import { undoToast } from "./undo";
import { useToasts } from "../../controls/toastQueue";
import type { Undone } from "../../api/bindings/Undone";
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
  resetNudges();
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

describe("Nudge keys", () => {
  /** The `photo_nudge` calls made so far. */
  const nudges = () =>
    fake.calls.filter((c) => c.command === "photo_nudge").map((c) => c.args as { ids: number[]; delta: number });
  const toast = () => screen.queryByTestId("score-toast");

  it("writes the toast texts: the change alone, never the new total", () => {
    expect(toastText(1)).toBe("+1");
    expect(toastText(-1)).toBe("−1");
    expect(toastText(5)).toBe("+5");
    expect(toastText(-5)).toBe("−5");
    expect(toastText(0)).toBe("Can't nudge any further");
  });

  it("adds up taps in one direction while the toast shows in the same place", () => {
    const up = { text: "+4", delta: 1 as const, change: 4, place: "preview" as const, seq: 4 };
    expect(toastChange(up, 1, 1, "preview")).toBe(5);
    expect(toastChange(up, -1, 1, "preview")).toBe(-1);
    expect(toastChange(up, 1, 1, "viewer")).toBe(1);
    expect(toastChange(up, 1, 0, "preview")).toBe(0);
    expect(toastChange(null, -1, 3, "preview")).toBe(-1);
    expect(toastChange({ ...up, change: 0 }, 1, 1, "preview")).toBe(1);
  });

  it("Up nudges the current photo and shows the toast over the preview, without moving the selection", async () => {
    fake.handle("photo_nudge", ({ ids }) => ({
      changed: ids,
      total: 74,
      revision: 9,
    }));
    await show(2);
    press("ArrowUp");
    await waitFor(() => expect(nudges()).toEqual([{ ids: [2], delta: 1 }]));
    const t = await within(region("Preview")).findByTestId("score-toast");
    expect(t).toHaveTextContent("+1");
    expect(t).not.toHaveTextContent("74");
    expect(t).toHaveClass("ow-score-toast--up");
    expect(grid().current).toEqual({ id: 2, index: 1 });
  });

  it("Down nudges the whole selection, current photo first", async () => {
    await show(2, [3, 4]);
    press("ArrowDown");
    await waitFor(() => expect(nudges()).toHaveLength(1));
    expect(nudges()[0]).toEqual({ ids: [2, 3, 4], delta: -1 });
    await waitFor(() => expect(toast()).toHaveTextContent("−1"));
    expect(toast()).toHaveClass("ow-score-toast--down");
  });

  it("says when no photo can be nudged further", async () => {
    fake.handle("photo_nudge", () => ({
      changed: [],
      total: 100060,
      revision: 1,
    }));
    await show(2);
    press("ArrowUp");
    await waitFor(() => expect(toast()).toHaveTextContent("Can't nudge any further"));
  });

  it("in the viewer nudges the shown photo only", async () => {
    await show(2, [3]);
    press("Enter");
    const viewer = await screen.findByRole("dialog");
    act(() =>
      grid().setSelection({
        current: { id: 2, index: 1 },
        anchor: { id: 2, index: 1 },
        selection: [2, 3],
      }),
    );
    press("ArrowUp");
    await waitFor(() => expect(nudges()).toEqual([{ ids: [2], delta: 1 }]));
    expect(await within(viewer).findByTestId("score-toast")).toHaveTextContent("+1");
  });

  it("does nothing while typing, for a held key, or in a view-only album", async () => {
    await show(2);
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    fireEvent.keyDown(field, { key: "ArrowUp" });
    field.remove();
    press("ArrowUp", { repeat: true });
    useAlbumStore.setState({
      status: fakeStatus("D:/Photos", { view_only: "read_only" }),
    });
    press("ArrowDown");
    await new Promise((r) => setTimeout(r, 20));
    expect(nudges()).toEqual([]);
    expect(toast()).toBeNull();
  });

  it("removes the toast after 1100 ms", async () => {
    await show(2);
    press("ArrowUp");
    await waitFor(() => expect(toast()).not.toBeNull());
    await waitFor(() => expect(toast()).toBeNull(), { timeout: 2500 });
  });
});

describe("Undo key", () => {
  const undos = () => fake.calls.filter((c) => c.command === "album_undo");
  const toasts = () => useToasts.getState().toasts.map((t) => ({ kind: t.kind, message: t.message }));

  beforeEach(() => useToasts.setState({ toasts: [] }));

  /** `album_undo` answers `undone`, after putting photo 2 back on show as Rust would. */
  function answer(undone: Undone, unhide = false) {
    fake.handle("album_undo", () => {
      if (unhide) {
        revision = album.change((all) => all.map((p) => (p.id === 2 ? { ...p, hidden: false } : p)));
        const now = revision;
        setTimeout(() => fake.emit("album_changed", { revision: now, reason: "info", rekeyed: [] }), 0);
      }
      return undone;
    });
  }

  it("writes the toast for each outcome", () => {
    const u = (outcome: Undone["outcome"], label: string | null): Undone => ({ outcome, label, ids: [], revision: 1 });
    expect(undoToast(u("nothing", null))).toEqual({ kind: "info", message: "Nothing to undo" });
    expect(undoToast(u("done", "star"))).toEqual({ kind: "info", message: "Undid: star" });
    expect(undoToast(u("partial", "hide"))).toEqual({ kind: "error", message: "Couldn't undo all of: hide" });
    expect(undoToast(u("gone", "notes"))).toEqual({
      kind: "error",
      message: "Couldn't undo notes: the photo is no longer in the album",
    });
  });

  it("Ctrl+Z calls album_undo and says when there is nothing to undo", async () => {
    await show(2);
    press("z", { ctrlKey: true });
    await waitFor(() => expect(undos()).toHaveLength(1));
    await waitFor(() => expect(toasts()).toEqual([{ kind: "info", message: "Nothing to undo" }]));
  });

  it("selects the one photo put back, even one that was hidden", async () => {
    setup(photos(5, (p) => (p.id === 2 ? { ...p, hidden: true } : p)));
    answer({ outcome: "done", label: "hide", ids: [2], revision: 9 }, true);
    await show(3);
    press("Z", { ctrlKey: true });
    await waitFor(() => expect(toasts()).toEqual([{ kind: "info", message: "Undid: hide" }]));
    await waitFor(() => expect(grid().current).toEqual({ id: 2, index: 1 }));
    expect(grid().selection).toEqual([2]);
  });

  it("keeps the selection when several photos were put back", async () => {
    answer({ outcome: "partial", label: "star", ids: [1, 4], revision: 9 });
    await show(3);
    press("z", { ctrlKey: true });
    await waitFor(() => expect(toasts()).toEqual([{ kind: "error", message: "Couldn't undo all of: star" }]));
    expect(grid().current).toEqual({ id: 3, index: 2 });
  });

  it("works in the viewer", async () => {
    await show(2);
    press("Enter");
    await screen.findByRole("dialog");
    press("z", { ctrlKey: true });
    await waitFor(() => expect(undos()).toHaveLength(1));
  });

  it("is left to a text field, and does nothing with Shift or in a view-only album", async () => {
    await show(2);
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    fireEvent.keyDown(field, { key: "z", ctrlKey: true });
    field.remove();
    press("z", { ctrlKey: true, shiftKey: true });
    useAlbumStore.setState({ status: fakeStatus("D:/Photos", { view_only: "read_only" }) });
    press("z", { ctrlKey: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(undos()).toEqual([]);
    expect(toasts()).toEqual([]);
  });
});
