import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { handlerGaps, row, type KeyId } from "../../keys";
import { BrowseGrid } from "./BrowseGrid";
import { BROWSE_KEY_IDS, browseKeyHandlers } from "./browseKeys";
import { BLOCK } from "./blocks";
import { reduceToCurrent, selectAll, selectById, selectNone } from "./selection";
import { gridBlocks, resetGrid, useGridStore } from "./store";

// jsdom does no layout: the grid's box is given a size (two columns of
// 160 px tiles), and ResizeObserver, which jsdom lacks, is a stub.
const box = { width: 420, height: 600 };

class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const isGrid = (el: Element) => el.classList.contains("ow-grid");

beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isGrid(this) ? box.width : 0;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return isGrid(this) ? box.height : 0;
    },
  });
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
});

afterAll(() => {
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
  vi.unstubAllGlobals();
});

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({
    id: i + 1,
    rel_path: `IMG_${i + 1}.jpg`,
    date: new Date(Date.UTC(2000, 0, 1) + i * 60_000).toISOString().slice(0, 19),
  }));
}

let fake: FakeApi;
let album: ReturnType<typeof createQueryFake>;

function openAlbum(n: number) {
  album = createQueryFake(photos(n));
  fake = createFakeApi(album.handlers);
  setApi(fake);
}

const grid = () => useGridStore.getState();
const calls = (command: string) => fake.calls.filter((c) => c.command === command);
const windows = () => calls("query_window").map((c) => (c.args as { offset: number }).offset);
const tile = (index: number) => document.querySelector<HTMLElement>(`[role="gridcell"][data-index="${index}"]`);
const selectedIndexes = () =>
  screen
    .getAllByRole("gridcell")
    .filter((t) => t.getAttribute("aria-selected") === "true")
    .map((t) => Number(t.dataset.index));
const at = (index: number): PhotoAt => ({ id: index + 1, index });
const sorted = (ids: number[]) => [...ids].sort((a, b) => a - b);

/** Renders the grid with selection and keys, and waits for the first rows. */
async function setup(n = 10, props: Parameters<typeof BrowseGrid>[0] = {}) {
  openAlbum(n);
  const view = render(<BrowseGrid {...props} />);
  await waitFor(() => expect(tile(0)).toHaveAccessibleName(/IMG_1\.jpg/));
  return view;
}

function click(index: number, mods: { ctrlKey?: boolean; shiftKey?: boolean } = {}) {
  fireEvent.click(tile(index)!, mods);
}

function key(k: string, mods: Partial<KeyboardEventInit> = {}, target: Element = document.body) {
  return fireEvent.keyDown(target, { key: k, ...mods });
}

beforeEach(() => {
  resetGrid();
});

afterEach(() => {
  resetGrid();
});

describe("clicking", () => {
  it("plain: the photo becomes the current photo, the whole selection and the anchor", async () => {
    await setup();
    click(2);
    click(4);
    expect(grid().current).toEqual(at(4));
    expect(grid().anchor).toEqual(at(4));
    expect(grid().selection).toEqual([5]);
    await waitFor(() => expect(selectedIndexes()).toEqual([4]));
    expect(tile(4)).toHaveClass("ow-tile--current");
  });

  it("Ctrl on a photo not selected adds it and makes it current and the anchor", async () => {
    await setup();
    click(2);
    click(5, { ctrlKey: true });
    expect(sorted(grid().selection)).toEqual([3, 6]);
    expect(grid().current).toEqual(at(5));
    expect(grid().anchor).toEqual(at(5));
  });

  it("Ctrl on a selected photo removes it; the anchor becomes the clicked photo", async () => {
    await setup();
    click(1);
    click(3, { ctrlKey: true });
    click(5, { ctrlKey: true });
    click(3, { ctrlKey: true });
    expect(sorted(grid().selection)).toEqual([2, 6]);
    expect(grid().current).toEqual(at(5));
    expect(grid().anchor).toEqual(at(3));
  });

  it("Ctrl on the current photo moves current to the next selected photo, else the one before", async () => {
    await setup();
    click(1);
    click(6, { ctrlKey: true });
    click(3, { ctrlKey: true });
    // Current is 3; the next selected after it is 6.
    click(3, { ctrlKey: true });
    await waitFor(() => expect(grid().current).toEqual(at(6)));
    expect(sorted(grid().selection)).toEqual([2, 7]);
    // Current is 6; nothing selected after it, so the one before: 1.
    click(6, { ctrlKey: true });
    await waitFor(() => expect(grid().current).toEqual(at(1)));
    expect(grid().selection).toEqual([2]);
  });

  it("Ctrl on the last selected photo leaves nothing selected", async () => {
    await setup();
    click(2);
    click(2, { ctrlKey: true });
    await waitFor(() => expect(grid().current).toBeNull());
    expect(grid().selection).toEqual([]);
  });

  it("finds the nearest selected photo with query_ids when the rows between are not fetched", async () => {
    await setup(1000);
    // Photo 900 is selected but its row was never fetched.
    act(() => grid().setSelection({ current: at(1), anchor: at(1), selection: [2, 901] }));
    click(1, { ctrlKey: true });
    await waitFor(() => expect(grid().current).toEqual(at(900)));
    expect(calls("query_ids")).toHaveLength(1);
    expect(grid().selection).toEqual([901]);
  });

  it("Shift selects from the anchor to the clicked photo; the anchor does not move", async () => {
    await setup();
    click(2);
    click(5, { shiftKey: true });
    expect(sorted(grid().selection)).toEqual([3, 4, 5, 6]);
    expect(grid().current).toEqual(at(5));
    expect(grid().anchor).toEqual(at(2));
    click(0, { shiftKey: true });
    expect(sorted(grid().selection)).toEqual([1, 2, 3]);
    expect(grid().anchor).toEqual(at(2));
    expect(calls("query_ids")).toHaveLength(0);
  });

  it("Shift with no anchor is a plain click", async () => {
    await setup();
    click(3, { shiftKey: true });
    expect(grid().selection).toEqual([4]);
    expect(grid().anchor).toEqual(at(3));
  });

  it("Ctrl+Shift adds the range instead of replacing", async () => {
    await setup();
    click(0);
    click(5, { ctrlKey: true });
    click(7, { ctrlKey: true, shiftKey: true });
    expect(sorted(grid().selection)).toEqual([1, 6, 7, 8]);
    expect(grid().anchor).toEqual(at(5));
    expect(grid().current).toEqual(at(7));
  });

  it("a range across rows not fetched calls query_ids once, showing the range by index meanwhile", async () => {
    await setup(1000);
    let answer: () => void = () => {};
    fake.handle("query_ids", (args) => {
      const ids = album.handlers.query_ids(args) as number[];
      return new Promise<number[]>((resolve) => (answer = () => resolve(ids)));
    });
    // The anchor's row was never fetched (setting no current photo keeps the view at the top).
    act(() => grid().setSelection({ current: null, anchor: at(900), selection: [901] }));
    click(1, { shiftKey: true });
    expect(calls("query_ids")).toHaveLength(1);
    expect(calls("query_ids")[0]!.args).toMatchObject({ offset: 1, limit: 900 });
    // Shown selected by index until the ids arrive.
    await waitFor(() => expect(selectedIndexes()).toContain(1));
    expect(selectedIndexes()).toContain(2);
    expect(selectedIndexes()).not.toContain(0);
    await act(async () => answer());
    await waitFor(() => expect(grid().selection).toHaveLength(900));
    expect(grid().current).toEqual(at(1));
    expect(grid().anchor).toEqual(at(900));
    expect(grid().pending).toBeNull();
  });

  it("right-click selects a photo not in the selection, then opens the menu", async () => {
    const onMenu = vi.fn();
    await setup(10, { onMenu });
    click(1);
    click(2, { shiftKey: true });
    fireEvent.contextMenu(tile(2)!);
    expect(sorted(grid().selection)).toEqual([2, 3]);
    expect(onMenu).toHaveBeenLastCalledWith(at(2), expect.anything());
    fireEvent.contextMenu(tile(6)!);
    expect(grid().selection).toEqual([7]);
    expect(grid().current).toEqual(at(6));
    expect(onMenu).toHaveBeenCalledTimes(2);
  });

  it("double-click opens the viewer on the photo", async () => {
    const onOpen = vi.fn();
    await setup(10, { onOpen });
    fireEvent.doubleClick(tile(3)!);
    expect(onOpen).toHaveBeenCalledWith(at(3));
  });
});

describe("keys", () => {
  it("has a handler for every 04 row of the review screen, and no other", () => {
    const rows = BROWSE_KEY_IDS.map((id: KeyId) => row(id));
    expect(rows.every((r) => r.place === "browse")).toBe(true);
    expect(handlerGaps(browseKeyHandlers({}), rows)).toEqual({ missing: [], unknown: [] });
    expect([...BROWSE_KEY_IDS].sort()).toEqual(
      [
        "browse.previous",
        "browse.next",
        "browse.extend_previous",
        "browse.extend_next",
        "browse.first",
        "browse.last",
        "browse.open_viewer",
        "browse.escape",
        "browse.select_all",
        "browse.select_none",
      ].sort(),
    );
  });

  it("Left, Right and Home with no current photo select the first; End the last", async () => {
    await setup();
    for (const k of ["ArrowLeft", "ArrowRight", "Home"]) {
      act(() => selectNone());
      key(k);
      expect(grid().current, k).toEqual(at(0));
      expect(grid().selection).toEqual([1]);
    }
    key("End");
    expect(grid().current).toEqual(at(9));
    expect(grid().selection).toEqual([10]);
    expect(grid().anchor).toEqual(at(9));
  });

  it("Left and Right step one place, select only that photo, and stop at the ends", async () => {
    await setup();
    click(3);
    click(5, { ctrlKey: true });
    key("ArrowRight");
    expect(grid().current).toEqual(at(6));
    expect(grid().selection).toEqual([7]);
    key("ArrowLeft");
    key("ArrowLeft");
    expect(grid().current).toEqual(at(4));
    key("Home");
    key("ArrowLeft");
    expect(grid().current).toEqual(at(0));
    key("End");
    key("ArrowRight", { repeat: true });
    expect(grid().current).toEqual(at(9));
  });

  it("Shift+Left and Shift+Right move the current photo and select the range from the anchor", async () => {
    await setup();
    click(4);
    key("ArrowRight", { shiftKey: true });
    key("ArrowRight", { shiftKey: true });
    expect(grid().current).toEqual(at(6));
    expect(grid().anchor).toEqual(at(4));
    expect(sorted(grid().selection)).toEqual([5, 6, 7]);
    key("ArrowLeft", { shiftKey: true });
    key("ArrowLeft", { shiftKey: true });
    key("ArrowLeft", { shiftKey: true });
    expect(grid().current).toEqual(at(3));
    expect(sorted(grid().selection)).toEqual([4, 5]);
  });

  it("Enter and Space open the viewer on the current photo, not with none and not on a control", async () => {
    const onOpen = vi.fn();
    const { container } = await setup(10, { onOpen });
    key("Enter");
    expect(onOpen).not.toHaveBeenCalled();
    click(2);
    key("Enter");
    key(" ");
    expect(onOpen).toHaveBeenCalledTimes(2);
    expect(onOpen).toHaveBeenLastCalledWith(at(2));
    const button = document.createElement("button");
    container.appendChild(button);
    button.focus();
    key("Enter", {}, button);
    expect(onOpen).toHaveBeenCalledTimes(2);
  });

  it("Esc with more than one photo selected reduces the selection to the current photo", async () => {
    await setup();
    click(1);
    click(4, { shiftKey: true });
    key("Escape");
    expect(grid().selection).toEqual([5]);
    expect(grid().current).toEqual(at(4));
    expect(reduceToCurrent()).toBe(false);
  });

  it("Ctrl+A selects every photo with one query_ids call and keeps the current photo", async () => {
    await setup();
    click(6);
    const e = key("a", { ctrlKey: true });
    expect(e).toBe(false); // the default (selecting the page's text) is blocked
    await waitFor(() => expect(grid().selection).toHaveLength(10));
    expect(calls("query_ids")).toHaveLength(1);
    expect(grid().current).toEqual(at(6));
  });

  it("Ctrl+A with no current photo makes the first photo current", async () => {
    await setup();
    await act(async () => selectAll());
    expect(grid().selection).toHaveLength(10);
    expect(grid().current).toEqual(at(0));
  });

  it("Ctrl+D selects nothing: no current photo and no anchor", async () => {
    await setup();
    click(1);
    click(3, { shiftKey: true });
    key("d", { ctrlKey: true });
    expect(grid().selection).toEqual([]);
    expect(grid().current).toBeNull();
    expect(grid().anchor).toBeNull();
  });

  it("ignores its keys while typing, in a dialog, and with Alt or Meta", async () => {
    const { container } = await setup();
    click(2);
    const input = document.createElement("input");
    container.appendChild(input);
    input.focus();
    key("ArrowRight", {}, input);
    key("a", { ctrlKey: true }, input);
    expect(grid().current).toEqual(at(2));
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    document.body.appendChild(dialog);
    try {
      key("ArrowRight");
      expect(grid().current).toEqual(at(2));
    } finally {
      dialog.remove();
    }
    key("ArrowRight", { altKey: true });
    key("ArrowRight", { metaKey: true });
    expect(grid().current).toEqual(at(2));
    key("ArrowRight");
    expect(grid().current).toEqual(at(3));
  });

  it("leaves keys it does not own alone", async () => {
    await setup();
    click(2);
    expect(key("ArrowUp")).toBe(true);
    expect(key("PageDown")).toBe(true);
    expect(grid().current).toEqual(at(2));
  });

  it("moves keyboard focus with the current photo when focus is in the grid", async () => {
    await setup();
    click(2);
    tile(2)!.focus();
    key("ArrowRight", {}, tile(2)!);
    await waitFor(() => expect(document.activeElement).toBe(tile(3)));
  });

  it("stepping onto a row not fetched waits for it; a held Right does not pile up block requests", async () => {
    await setup(1000);
    act(() => grid().setSelection({ current: at(BLOCK - 1), anchor: at(BLOCK - 1), selection: [BLOCK] }));
    const before = windows().length;
    act(() => {
      for (let i = 0; i < 2 * BLOCK; i++) key("ArrowRight", { repeat: i > 0 });
    });
    // The index moved at once; the photo becomes current when its row arrives.
    const target = 3 * BLOCK - 1;
    expect(grid().pending?.index).toBe(target);
    await waitFor(() => expect(grid().current).toEqual(at(target)));
    expect(grid().selection).toEqual([target + 1]);
    expect(grid().pending).toBeNull();
    const asked = windows().slice(before);
    expect(asked).not.toContain(BLOCK);
    expect(asked).toContain(2 * BLOCK);
    expect(gridBlocks().fresh(target)).toBe(true);
  });
});

describe("across results", () => {
  it("refresh re-runs the query keeping the selection", async () => {
    await setup();
    click(3);
    album.change((list) => list.filter((p) => p.id !== 1));
    await act(async () => grid().refresh());
    const run = calls("query_run").at(-1)!;
    expect(run.args).toMatchObject({ keep: { current: at(3), anchor_id: 4, selection: [4] } });
    expect(grid().current).toEqual({ id: 4, index: 2 });
  });

  it("a new result drops a pending change and its late answer", async () => {
    await setup(1000);
    let answer: () => void = () => {};
    fake.handle("query_ids", (args) => {
      const ids = album.handlers.query_ids(args) as number[];
      return new Promise<number[]>((resolve) => (answer = () => resolve(ids)));
    });
    click(2);
    act(() => grid().setSelection({ current: at(2), anchor: at(900), selection: [3] }));
    click(5, { shiftKey: true });
    await waitFor(() => expect(grid().pending).not.toBeNull());
    await act(async () => grid().refresh());
    expect(grid().pending).toBeNull();
    await act(async () => answer());
    expect(grid().selection).toEqual([3]);
    expect(grid().current).toEqual(at(2));
  });

  it("selectById finds a photo known only by id, and leaves the selection for one not shown", async () => {
    await setup(1000);
    await act(async () => selectById(800));
    expect(grid().current).toEqual(at(799));
    expect(grid().selection).toEqual([800]);
    await act(async () => selectById(5000));
    expect(grid().current).toEqual(at(799));
  });
});
