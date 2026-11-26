import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setApi } from "../../api";
import type { Row } from "../../api/bindings/Row";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { handlerGaps, row as keyRow } from "../../keys";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { seriousFindings } from "../../test/axe";
import { Review } from "../album/Review";
import { Caption } from "./Caption";
import { ZOOM_KEY_IDS, zoomKeyHandlers, type ZoomHandle } from "./ZoomStage";

// jsdom has no layout: the stage measures 800 × 600 and everything else
// 1000 × 800. ResizeObserver, which jsdom lacks, is a stub the test fires.
let stageSize = { width: 800, height: 600 };
let observers: (() => void)[] = [];
class FakeResizeObserver {
  fire: () => void;
  constructor(callback: ResizeObserverCallback) {
    this.fire = () => callback([], this as unknown as ResizeObserver);
  }
  observe() {
    observers.push(this.fire);
  }
  unobserve() {}
  disconnect() {
    observers = observers.filter((f) => f !== this.fire);
  }
}

const isStage = (el: HTMLElement) => el.classList.contains("ow-zoom");

let fake: FakeApi;
const grid = () => useGridStore.getState();

function photos(n: number, change: (p: FakePhoto) => FakePhoto = (p) => p): FakePhoto[] {
  return Array.from({ length: n }, (_, i) =>
    change({ id: i + 1, rel_path: `day/p${i + 1}.jpg`, width: 4000, height: 2000 }),
  );
}

let album: ReturnType<typeof createQueryFake>;

function setup(list: FakePhoto[] = photos(5), detail = (id: number) => fakeDetail(id, { width: 4000, height: 2000 })) {
  album = createQueryFake(list);
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) => detail(id),
  });
  setApi(fake);
}

async function show(id: number, onOpen?: () => void) {
  render(<Review busy={() => false} onOpen={onOpen} />);
  await waitFor(() => expect(grid().result?.count).toBeGreaterThan(0));
  act(() =>
    grid().setSelection({ current: { id, index: id - 1 }, anchor: { id, index: id - 1 }, selection: [id] }),
  );
  await waitFor(() => expect(stage().querySelector("img")).not.toBeNull());
}

const preview = () => screen.getByRole("region", { name: "Preview" });
const stage = () => preview().querySelector<HTMLElement>(".ow-zoom")!;
const picture = () => stage().querySelector<HTMLElement>(".ow-zoom__picture")!;
const images = () => [...stage().querySelectorAll<HTMLImageElement>(".ow-zoom__picture img")].map((i) => i.getAttribute("src"));
const miniMap = () => stage().querySelector<HTMLElement>(".ow-minimap");
const wheel = (deltaY: number, x = 400, y = 300) => fireEvent.wheel(stage(), { deltaY, clientX: x, clientY: y });
const zoomOf = () => Number(/scale\(([\d.]+)\)/.exec(picture().style.transform)?.[1] ?? NaN);

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  stageSize = { width: 800, height: 600 };
  observers = [];
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isStage(this) ? stageSize.width : 1000;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return isStage(this) ? stageSize.height : 800;
    },
  });
  setup();
});

afterEach(() => {
  resetGrid();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("Preview", () => {
  it("shows the hint with no current photo, and no caption", async () => {
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result?.count).toBe(5));
    expect(preview()).toHaveTextContent("Select a photo to see it here.");
    expect(preview().querySelector(".ow-preview__caption")).toBeNull();
  });

  it("shows the small thumbnail at once, fitted, then swaps to the medium one when it loads", async () => {
    await show(3);
    // 4000 × 2000 in 800 × 600: 800 × 400, centred.
    expect(picture().style.width).toBe("800px");
    expect(picture().style.height).toBe("400px");
    expect(picture().style.transform).toBe("translate(0px, 100px) scale(1)");
    const [small, medium] = stage().querySelectorAll<HTMLImageElement>(".ow-zoom__picture img");
    expect(small!.getAttribute("src")).toMatch(/thumb\/tk-3\/s$/);
    expect(medium!.getAttribute("src")).toMatch(/thumb\/tk-3\/m$/);
    expect(medium).not.toHaveClass("ow-zoom__img--ready");
    fireEvent.load(medium!);
    expect(medium).toHaveClass("ow-zoom__img--ready");
    expect(picture().style.transform).toBe("translate(0px, 100px) scale(1)");
    expect(within(preview()).getByText("day/p3.jpg")).toBeInTheDocument();
  });

  it("never shows another photo's picture, and a new photo starts fitted", async () => {
    await show(3);
    wheel(-100);
    expect(zoomOf()).toBeCloseTo(1.2);
    act(() => grid().setSelection({ current: { id: 4, index: 3 }, anchor: { id: 4, index: 3 }, selection: [4] }));
    await waitFor(() => expect(images().some((s) => s!.includes("tk-4"))).toBe(true));
    expect(images().every((s) => s!.includes("tk-4"))).toBe(true);
    expect(zoomOf()).toBe(1);
  });

  it("zooms with the wheel about the pointer, shows the mini map, and comes back to fit", async () => {
    await show(2);
    expect(miniMap()).toBeNull();
    const notScrolled = fireEvent.wheel(stage(), { deltaY: -100, clientX: 400, clientY: 300 });
    expect(notScrolled).toBe(false); // the wheel never scrolls the page
    expect(zoomOf()).toBeCloseTo(1.2);
    // The point under (400, 300) stays put: it was picture point (400, 200).
    expect(picture().style.transform).toBe(`translate(${400 - 400 * 1.2}px, ${300 - 200 * 1.2}px) scale(1.2)`);
    expect(miniMap()).not.toBeNull();
    wheel(100);
    expect(zoomOf()).toBe(1);
    expect(miniMap()).toBeNull();
  });

  it("pans with a drag when zoomed in, and clamps", async () => {
    await show(2);
    wheel(-100);
    wheel(-100); // 1.44: 1152 × 576
    const before = picture().style.transform;
    fireEvent.pointerDown(stage(), { button: 0, clientX: 400, clientY: 300, pointerId: 1 });
    expect(stage()).toHaveClass("ow-zoom--grabbing");
    fireEvent.pointerMove(stage(), { buttons: 1, clientX: 1400, clientY: 300, pointerId: 1 });
    fireEvent.pointerUp(stage(), { clientX: 1400, clientY: 300, pointerId: 1 });
    expect(picture().style.transform).not.toBe(before);
    // Clamped at the left edge; the height (576) fits, so it is centred.
    expect(picture().style.transform).toBe("translate(0px, 12px) scale(1.44)");
  });

  it("double-click at fit opens the viewer; zoomed in it goes back to fit", async () => {
    const onOpen = vi.fn();
    await show(2, onOpen);
    wheel(-100);
    fireEvent.doubleClick(stage());
    expect(zoomOf()).toBe(1);
    expect(onOpen).not.toHaveBeenCalled();
    fireEvent.doubleClick(stage());
    expect(onOpen).toHaveBeenCalledWith({ id: 2, index: 1 });
  });

  it("goes back to fit when the stage is resized", async () => {
    await show(2);
    wheel(-100);
    stageSize = { width: 600, height: 600 };
    act(() => observers.forEach((f) => f()));
    expect(zoomOf()).toBe(1);
    expect(picture().style.width).toBe("600px");
  });

  it("asks for the full-size picture only past the medium picture's pixels", async () => {
    await show(2);
    await waitFor(() => expect(fake.calls.some((c) => c.command === "photo_detail")).toBe(true));
    // Medium is 1600 wide; fitted is 800, so the full picture is wanted above zoom 2.
    for (let i = 0; i < 3; i++) wheel(-100); // 1.728
    expect(images().some((s) => s!.includes("full/2"))).toBe(false);
    for (let i = 0; i < 2; i++) wheel(-100); // 2.49
    await waitFor(() => expect(images().some((s) => s!.includes("full/2?v=tk-2"))).toBe(true));
  });

  it("never asks for the full-size picture when the original is no larger than the medium one", async () => {
    setup(photos(5), (id) => fakeDetail(id, { width: 1200, height: 600 }));
    await show(2);
    await waitFor(() => expect(fake.calls.some((c) => c.command === "photo_detail")).toBe(true));
    for (let i = 0; i < 15; i++) wheel(-100);
    expect(zoomOf()).toBe(8);
    expect(images().some((s) => s!.includes("full/"))).toBe(false);
  });

  it("does not zoom a video", async () => {
    setup(photos(5, (p) => ({ ...p, kind: "video", duration_ms: 5000 })));
    await show(2);
    wheel(-100);
    expect(zoomOf()).toBe(1);
    expect(miniMap()).toBeNull();
  });

  it("keeps showing the photo when a re-run moves it", async () => {
    await show(3);
    act(() => void album.change((list) => [{ id: 99, rel_path: "day/a0.jpg", width: 4000, height: 2000 }, ...list]));
    act(() => void grid().refresh());
    await waitFor(() => expect(grid().current).toEqual({ id: 3, index: 3 }));
    expect(images().every((s) => s!.includes("tk-3"))).toBe(true);
    expect(images().length).toBeGreaterThan(0);
  });

  it("has no serious accessibility findings, at fit or zoomed in", async () => {
    await show(2);
    expect(await seriousFindings(preview())).toEqual([]);
    wheel(-100);
    expect(await seriousFindings(preview())).toEqual([]);
  });

  it("centres the view on the point pressed in the mini map", async () => {
    await show(2);
    wheel(-100);
    wheel(-100);
    const map = miniMap()!;
    // 160 × 80 at the stage's top-right.
    map.getBoundingClientRect = () => ({ left: 630, top: 10, width: 160, height: 80, right: 790, bottom: 90, x: 630, y: 10, toJSON() {} });
    fireEvent.pointerDown(map, { button: 0, clientX: 630, clientY: 10, pointerId: 2 });
    // The top-left corner: clamped to x = 0; the height fits, so centred.
    expect(picture().style.transform).toBe("translate(0px, 12px) scale(1.44)");
    expect(stage()).not.toHaveClass("ow-zoom--grabbing");
  });
});

describe("Caption", () => {
  const base: Row = {
    id: 7,
    rel_path: "2024/Trip/IMG_0042.jpg",
    file_name: "IMG_0042.jpg",
    thumb_key: "k",
    aspect: 1.5,
    kind: "image",
    duration_ms: null,
    star: false,
    hidden: false,
    score: null,
    stack: null,
    edit_plan: null,
  };

  it("shows the path and the badges that apply", () => {
    render(
      <Caption
        row={{ ...base, star: true, hidden: true, stack: { id: 1, size: 5, open: true, face: false, place: 2 } }}
        selected={1}
        fullPath="D:\\Photos\\2024\\Trip\\IMG_0042.jpg"
        editTarget="album"
      />,
    );
    expect(screen.getByText("2024/Trip/IMG_0042.jpg")).toBeInTheDocument();
    expect(screen.getByText("Starred")).toBeInTheDocument();
    expect(screen.getByText("Hidden")).toBeInTheDocument();
    expect(screen.getByText("2 of 5 in stack")).toBeInTheDocument();
    expect(screen.queryByText(/selected/)).toBeNull();
    expect(screen.queryByText(/Edited/)).toBeNull();
  });

  it("starts with the selection count when more than one photo is selected, and shows the edit badge", () => {
    render(<Caption row={{ ...base, edit_plan: {} }} selected={3} fullPath={null} editTarget="file" />);
    expect(screen.getByText("3 selected ·")).toBeInTheDocument();
    expect(screen.getByText("Edited · not saved")).toBeInTheDocument();
    expect(screen.queryByText("Starred")).toBeNull();
  });
});

describe("zoom keys", () => {
  it("has a handler for each of the viewer's zoom rows, for the viewer (#22) to wire", () => {
    const rows = ZOOM_KEY_IDS.map((id) => keyRow(id));
    const handle: ZoomHandle = { zoomBy: vi.fn(), fit: vi.fn(), zoomed: () => true };
    const handlers = zoomKeyHandlers(handle);
    expect(handlerGaps(handlers, rows)).toEqual({ missing: [], unknown: [] });
    handlers["viewer.zoom_in"]();
    expect(handle.zoomBy).toHaveBeenLastCalledWith(1.2);
    handlers["viewer.zoom_out"]();
    expect(handle.zoomBy).toHaveBeenLastCalledWith(1 / 1.2);
    handlers["viewer.fit"]();
    expect(handle.fit).toHaveBeenCalled();
  });
});
