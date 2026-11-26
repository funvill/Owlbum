import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { handlerGaps, row as keyRow } from "../../keys";
import { seriousFindings } from "../../test/axe";
import { useSettingsStore } from "../app";
import { Review } from "../album/Review";
import { resetGrid, useGridStore } from "../grid";
import { resetImageCache, type ImageCache, type Load } from "../preview";
import { resetViewer, useViewerStore } from "./store";
import { VIEWER_KEY_IDS, viewerKeyHandlers } from "./Viewer";

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
const grid = () => useGridStore.getState();

function photos(n: number, change: (p: FakePhoto) => FakePhoto = (p) => p): FakePhoto[] {
  return Array.from({ length: n }, (_, i) =>
    change({ id: i + 1, rel_path: `day/p${i + 1}.jpg`, width: 4000, height: 2000 }),
  );
}

// The image cache's loads, finished by hand: jsdom never loads a picture.
let loads: Map<string, () => void>;
let started: string[];
let cache: ImageCache;
function fakeLoader(url: string): Load {
  started.push(url);
  let resolve = () => {};
  const done = new Promise<void>((r) => (resolve = r));
  loads.set(url, resolve);
  return { done, cancel: () => {} };
}
const key = (url: string) => /thumb\/(tk-\d+)\/m$/.exec(url)?.[1];

function setup(list: FakePhoto[] = photos(5)) {
  album = createQueryFake(list);
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) =>
      fakeDetail(id, { width: 4000, height: 2000, file_name: `p${id}.jpg`, people: ["Ann", "Bob"] }),
  });
  setApi(fake);
}

/** Renders the review screen with photo `id` current (and `also` selected too). */
async function show(id: number, also: number[] = []) {
  render(<Review busy={() => false} />);
  await waitFor(() => expect(grid().result?.count).toBeGreaterThan(0));
  act(() =>
    grid().setSelection({ current: { id, index: id - 1 }, anchor: { id, index: id - 1 }, selection: [id, ...also] }),
  );
}

/** Opens the viewer with Enter, as the review screen's key does. */
async function open(id: number, also: number[] = []) {
  await show(id, also);
  fireEvent.keyDown(document.body, { key: "Enter" });
  await waitFor(() => expect(viewer().querySelector(".ow-zoom img")).not.toBeNull());
}

const viewer = () => screen.getByRole("dialog");
const press = (key: string, init: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(document.activeElement ?? document.body, { key, ...init });
const stage = () => viewer().querySelector<HTMLElement>(".ow-zoom")!;
const picture = () => stage().querySelector<HTMLElement>(".ow-zoom__picture")!;
const images = () => [...stage().querySelectorAll("img")].map((i) => i.getAttribute("src"));
const zoomOf = () => Number(/scale\(([\d.]+)\)/.exec(picture().style.transform)?.[1] ?? NaN);

beforeEach(() => {
  resetGrid();
  resetViewer();
  loads = new Map();
  started = [];
  cache = resetImageCache(fakeLoader);
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
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("Viewer", () => {
  it("opens with Enter on the current photo, as a modal dialog labelled with its file name", async () => {
    await open(3);
    expect(viewer()).toHaveAttribute("aria-modal", "true");
    expect(viewer()).toHaveAccessibleName("p3.jpg");
    expect(within(viewer()).getByText("3 / 5")).toBeInTheDocument();
    expect(images().every((s) => s!.includes("tk-3"))).toBe(true);
    expect(document.activeElement).toBe(viewer());
  });

  it("opens with a double-click on the preview at fit", async () => {
    await show(2);
    const previewStage = await waitFor(() => {
      const el = screen.getByRole("region", { name: "Preview" }).querySelector<HTMLElement>(".ow-zoom");
      expect(el).not.toBeNull();
      return el!;
    });
    fireEvent.doubleClick(previewStage);
    expect(viewer()).toHaveAccessibleName("p2.jpg");
  });

  it("steps with Left, Right, Home and End, stopping at the ends, and moves the current photo", async () => {
    await open(2);
    press("ArrowRight");
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p3.jpg"));
    expect(grid().current).toEqual({ id: 3, index: 2 });
    press("End");
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p5.jpg"));
    press("ArrowRight");
    expect(grid().current).toEqual({ id: 5, index: 4 });
    press("Home");
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p1.jpg"));
    press("ArrowLeft");
    expect(grid().current).toEqual({ id: 1, index: 0 });
    expect(within(viewer()).getByText("1 / 5")).toBeInTheDocument();
  });

  it("keeps keys from the page behind and blocks Tab", async () => {
    await open(2);
    press("a", { ctrlKey: true });
    await act(async () => new Promise((r) => setTimeout(r, 0)));
    expect(grid().selection).toEqual([2]);
    expect(fake.calls.some((c) => c.command === "query_ids")).toBe(false);
    expect(press("Tab")).toBe(false); // default prevented
  });

  it("zooms with + and -, goes back to fit with 0, and Esc goes to fit before closing", async () => {
    await open(2);
    press("+");
    expect(zoomOf()).toBeCloseTo(1.2);
    press("=");
    expect(zoomOf()).toBeCloseTo(1.44);
    press("-");
    expect(zoomOf()).toBeCloseTo(1.2);
    press("0");
    expect(zoomOf()).toBe(1);
    press("+");
    press("Escape");
    expect(zoomOf()).toBe(1);
    expect(useViewerStore.getState().open).toBe(true);
    press("Escape");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("double-click at fit zooms to actual size at the pointer", async () => {
    await open(2);
    await waitFor(() => expect(fake.calls.some((c) => c.command === "photo_detail")).toBe(true));
    // A 4000 px wide original fitted to 800 px: actual size is zoom 5.
    await waitFor(() => {
      fireEvent.doubleClick(stage(), { clientX: 400, clientY: 300 });
      expect(zoomOf()).toBe(5);
    });
    fireEvent.doubleClick(stage());
    expect(zoomOf()).toBe(1);
  });

  it("closing makes the last photo shown the current photo and the whole selection", async () => {
    await open(2, [1, 3]);
    press("ArrowRight");
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p3.jpg"));
    press("Enter");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(grid().current).toEqual({ id: 3, index: 2 });
    expect(grid().selection).toEqual([3]);
  });

  it("closes with its close button, reducing the selection to the photo shown", async () => {
    await open(2, [1, 3]);
    fireEvent.click(within(viewer()).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(grid().selection).toEqual([2]);
  });

  it("shows the photo now in its place when the shown photo leaves the result", async () => {
    await open(3);
    await act(async () => {
      album.change((list) => list.filter((p) => p.id !== 3));
      await grid().refresh();
    });
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p4.jpg"));
    expect(within(viewer()).getByText("3 / 4")).toBeInTheDocument();
  });

  it("closes when the result becomes empty", async () => {
    await open(3);
    await act(async () => {
      album.change(() => []);
      await grid().refresh();
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("shows the details panel with I, and keeps it open for the session", async () => {
    await open(2);
    expect(within(viewer()).queryByRole("region", { name: "Details" })).toBeNull();
    press("i");
    const box = await within(viewer()).findByRole("region", { name: "Details" });
    expect(box).toHaveTextContent("p2.jpg");
    expect(box).toHaveTextContent("Taken12 Jun 2024, 14:03");
    expect(box).toHaveTextContent("CameraOwlbum Fixture One");
    expect(box).toHaveTextContent("Size4000 × 2000 px · 3.1 MB");
    expect(box).toHaveTextContent("PeopleAnn, Bob");
    press("Escape");
    fireEvent.keyDown(document.body, { key: "Enter" });
    expect(await within(viewer()).findByRole("region", { name: "Details" })).toBeInTheDocument();
    press("I");
    expect(within(viewer()).queryByRole("region", { name: "Details" })).toBeNull();
  });

  it("shows an edited photo as edited, on the same picture the preview uses", async () => {
    setup(photos(5, (p) => (p.id === 2 ? { ...p, edit_plan: { steps: [] } } : p)));
    await open(2);
    expect(within(viewer()).getByText("Edited")).toBeInTheDocument();
    expect(images()[0]).toMatch(/thumb\/tk-2\/s$/);
    press("ArrowRight");
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p3.jpg"));
    expect(within(viewer()).queryByText("Edited")).toBeNull();
  });

  it("shows the star and hidden marks in the top bar", async () => {
    setup(photos(5, (p) => (p.id === 2 ? { ...p, star: true } : p)));
    await open(2);
    expect(within(viewer()).getByRole("img", { name: "Starred" })).toBeInTheDocument();
    expect(within(viewer()).queryByRole("img", { name: "Hidden" })).toBeNull();
  });

  it("has no serious accessibility findings", async () => {
    await open(2);
    press("i");
    await within(viewer()).findByRole("region", { name: "Details" });
    expect(await seriousFindings(viewer())).toEqual([]);
  });

  it("has a handler for each of the viewer's rows marked 04", () => {
    const rows = VIEWER_KEY_IDS.map((id) => keyRow(id));
    expect(handlerGaps(viewerKeyHandlers(null), rows)).toEqual({ missing: [], unknown: [] });
  });
});

describe("Preloading in the viewer", () => {
  it("after opening, asks for the next 20 and the previous 5, the next one first", async () => {
    setup(photos(40));
    const want = vi.spyOn(cache, "want");
    await open(10);
    const keys = want.mock.lastCall![0].map((w) => key(w.url));
    expect(keys[0]).toBe("tk-10");
    expect(keys.slice(1, 5)).toEqual(["tk-11", "tk-12", "tk-13", "tk-9"]);
    expect(keys).toHaveLength(26);
    expect(keys).toContain("tk-30");
    expect(keys).toContain("tk-5");
    expect(keys).not.toContain("tk-31");
    expect(keys).not.toContain("tk-4");
  });

  it("stepping to a preloaded photo shows its medium picture at once and asks for nothing new for it", async () => {
    setup(photos(40));
    await open(10);
    for (let round = 0; round < 10; round++) {
      await act(async () => {
        for (const resolve of loads.values()) resolve();
        await new Promise((r) => setTimeout(r, 0));
      });
    }
    expect(cache.held()).toHaveLength(26);
    const asked = started.length;
    press("ArrowRight");
    await waitFor(() => expect(viewer()).toHaveAccessibleName("p11.jpg"));
    expect(images()).toEqual([expect.stringMatching(/thumb\/tk-11\/m$/)]);
    expect(started.slice(asked).map(key)).toEqual(["tk-31"]);
  });
});
