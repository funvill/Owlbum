import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { defaultSettings } from "../../api/fake/app";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { Review } from "./Review";
import { STRIP_CHROME } from "./columns";
import { setGridPlace } from "./panels";
import { PhotoMap } from "../map/photoMap";
import { resetMapPlace } from "../map";
import { mapText } from "../../text/map";
import { read, srcDir } from "../../test/sources";
import { join } from "node:path";

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({ id: i + 1, rel_path: `day/p${i + 1}.jpg` }));
}

let fake: FakeApi;
const grid = () => useGridStore.getState();
const calls = (command: string) => fake.calls.filter((c) => c.command === command);
/** The fake's settings forget earlier patches; these keep them, as Rust does. */
const keepSettings = () =>
  fake.handle("app_settings_set", ({ patch }) => ({ ...useSettingsStore.getState().settings!, ...patch }));

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi({
    ...createQueryFake(photos(5)).handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg`, rel_path: `day/p${id}.jpg` }),
  });
  setApi(fake);
});

afterEach(() => {
  resetGrid();
  resetMapPlace();
});

describe("Review", () => {
  it("shows the grid's current photo in the details panel", async () => {
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result?.count).toBe(5));
    act(() => grid().setSelection({ current: null, anchor: null, selection: [] }));
    expect(screen.getByRole("complementary", { name: "Details" })).toHaveTextContent("No photo selected");

    act(() => grid().setSelection({ current: { id: 3, index: 2 }, anchor: { id: 3, index: 2 }, selection: [3] }));
    await screen.findByText("Taken");
    expect(calls("photo_detail").at(-1)?.args).toEqual({ id: 3 });
  });

  it("re-runs the query when the photo is not found", async () => {
    fake.handle("photo_detail", () => {
      throw appError("photo_not_found", "That photo is no longer in this album.");
    });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    const before = calls("query_run").length;
    act(() => grid().setSelection({ current: { id: 4, index: 3 }, anchor: { id: 4, index: 3 }, selection: [4] }));
    await waitFor(() => expect(calls("query_run").length).toBe(before + 1));
  });
});

describe("Review splitters", () => {
  // The row is 1440 px wide: free = 1440 - 300 - 12 = 1128.
  beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get: () => 1440,
    });
  });
  afterAll(() => {
    delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  });

  const splitter = (name: string) => screen.getByRole("separator", { name });
  const saves = () => calls("app_settings_set").map((c) => (c.args as { patch: unknown }).patch);

  it("shows the stored widths on two labelled splitters", async () => {
    useSettingsStore.setState({
      settings: {
        ...defaultSettings,
        "panel.grid_width": 500,
        "panel.details_width": 300,
      },
    });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    const thumbs = splitter("Resize thumbnail column");
    expect(thumbs).toHaveAttribute("aria-orientation", "vertical");
    expect(thumbs).toHaveAttribute("aria-valuenow", "500");
    expect(thumbs).toHaveAttribute("aria-valuemin", "200");
    expect(thumbs).toHaveAttribute("aria-valuemax", "868");
    expect(splitter("Resize details panel")).toHaveAttribute("aria-valuenow", "300");
    expect(splitter("Resize details panel")).toHaveAttribute("aria-valuemax", "560");
    expect(document.querySelector<HTMLElement>(".ow-review__photos")?.style.width).toBe("500px");
    expect(document.querySelector<HTMLElement>(".ow-review__details")?.style.width).toBe("300px");
  });

  it("moves 16 px with Left and Right, goes to the ends with Home and End, and saves after the keys", async () => {
    useSettingsStore.setState({ settings: { ...defaultSettings } });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    const thumbs = splitter("Resize thumbnail column");
    fireEvent.keyDown(thumbs, { key: "ArrowRight" });
    fireEvent.keyDown(thumbs, { key: "ArrowRight" });
    expect(thumbs).toHaveAttribute("aria-valuenow", "452");
    expect(saves()).toEqual([]);
    await waitFor(() => expect(saves()).toEqual([{ "panel.grid_width": 452 }]));
    await waitFor(() => expect(useSettingsStore.getState().settings?.["panel.grid_width"]).toBe(452));

    // The details splitter narrows its column when it moves right.
    const details = splitter("Resize details panel");
    fireEvent.keyDown(details, { key: "ArrowRight" });
    expect(details).toHaveAttribute("aria-valuenow", "324");
    fireEvent.keyDown(details, { key: "Home" });
    expect(details).toHaveAttribute("aria-valuenow", "260");
    fireEvent.keyDown(details, { key: "End" });
    expect(details).toHaveAttribute("aria-valuenow", "560");
    await waitFor(() => expect(saves().at(-1)).toEqual({ "panel.details_width": 560 }));
    expect(saves()).toHaveLength(2);
  });

  it("drags within the limits, saves on release, and puts the default back on a double-click", async () => {
    useSettingsStore.setState({ settings: { ...defaultSettings } });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    const thumbs = splitter("Resize thumbnail column");
    fireEvent.pointerDown(thumbs, { button: 0, pointerId: 1, clientX: 420 });
    fireEvent.pointerMove(thumbs, { pointerId: 1, clientX: 380.4 });
    expect(thumbs).toHaveAttribute("aria-valuenow", "380");
    // Past the minimum, but not far enough to hide the column.
    fireEvent.pointerMove(thumbs, { pointerId: 1, clientX: 160 });
    expect(thumbs).toHaveAttribute("aria-valuenow", "200");
    expect(saves()).toEqual([]);
    fireEvent.pointerUp(thumbs, { pointerId: 1, clientX: 160 });
    await waitFor(() => expect(saves()).toEqual([{ "panel.grid_width": 200 }]));

    const details = splitter("Resize details panel");
    fireEvent.pointerDown(details, { button: 0, pointerId: 2, clientX: 1000 });
    fireEvent.pointerMove(details, { pointerId: 2, clientX: 900 });
    expect(details).toHaveAttribute("aria-valuenow", "440");
    fireEvent.pointerUp(details, { pointerId: 2 });
    await waitFor(() => expect(saves().at(-1)).toEqual({ "panel.details_width": 440 }));

    fireEvent.doubleClick(thumbs);
    await waitFor(() => expect(saves().at(-1)).toEqual({ "panel.grid_width": 420 }));
    expect(thumbs).toHaveAttribute("aria-valuenow", "420");
  });

  it("hides a panel dragged well past its minimum, keeping its stored width", async () => {
    useSettingsStore.setState({ settings: { ...defaultSettings, "panel.grid_width": 300 } });
    keepSettings();
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    // Just past the minimum it stops there and stays.
    const thumbs = splitter("Resize thumbnail column");
    fireEvent.pointerDown(thumbs, { button: 0, pointerId: 1, clientX: 300 });
    fireEvent.pointerMove(thumbs, { pointerId: 1, clientX: 180 });
    expect(thumbs).toHaveAttribute("aria-valuenow", "200");
    expect(thumbs).not.toHaveClass("ow-splitter--collapsing");
    // Well past it, letting go hides the thumbnails.
    fireEvent.pointerMove(thumbs, { pointerId: 1, clientX: 100 });
    expect(thumbs).toHaveClass("ow-splitter--collapsing");
    fireEvent.pointerUp(thumbs, { pointerId: 1 });
    await waitFor(() => expect(useSettingsStore.getState().settings?.["panel.grid_shown"]).toBe(false));
    expect(saves()).toEqual([{ "panel.grid_shown": false }]);

    const details = splitter("Resize details panel");
    fireEvent.pointerDown(details, { button: 0, pointerId: 2, clientX: 1000 });
    fireEvent.pointerMove(details, { pointerId: 2, clientX: 1200 });
    fireEvent.pointerUp(details, { pointerId: 2 });
    await waitFor(() => expect(useSettingsStore.getState().settings?.["panel.details_shown"]).toBe(false));
    expect(useSettingsStore.getState().settings?.["panel.grid_width"]).toBe(300);

    // Each comes back from its button at the preview's edge, at its old width.
    fireEvent.click(screen.getByRole("button", { name: "Show thumbnails" }));
    await waitFor(() => expect(splitter("Resize thumbnail column")).toHaveAttribute("aria-valuenow", "300"));
    expect(document.querySelector(".ow-review__edge--left")).toBeNull();
    expect(document.querySelector(".ow-review__edge--right")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show details panel" }));
    await waitFor(() => expect(screen.getByRole("complementary", { name: "Details" })).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "Show details panel" })).toBeNull();
  });

  it("narrows the columns in a narrow row without changing the stored widths", async () => {
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get: () => 960,
    });
    useSettingsStore.setState({ settings: { ...defaultSettings } });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    expect(splitter("Resize thumbnail column")).toHaveAttribute("aria-valuenow", "388");
    expect(splitter("Resize details panel")).toHaveAttribute("aria-valuenow", "260");
    expect(saves()).toEqual([]);
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get: () => 1440,
    });
  });
});

describe("Review panels", () => {
  const settings = () => useSettingsStore.getState().settings;
  const photosColumn = () => document.querySelector<HTMLElement>(".ow-review__photos");

  it("hides and shows the details panel with its button and with I, and remembers it", async () => {
    useSettingsStore.setState({ settings: { ...defaultSettings } });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    const button = screen.getByRole("button", { name: "Details panel" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(button);
    await waitFor(() => expect(settings()?.["panel.details_shown"]).toBe(false));
    expect(screen.queryByRole("complementary", { name: "Details" })).toBeNull();
    expect(screen.queryByRole("separator", { name: "Resize details panel" })).toBeNull();
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.keyDown(window, { key: "i", code: "KeyI" });
    await waitFor(() => expect(settings()?.["panel.details_shown"]).toBe(true));
    expect(screen.getByRole("complementary", { name: "Details" })).toBeInTheDocument();
  });

  it("hides the thumbnails with T while the keys and the selection go on working", async () => {
    useSettingsStore.setState({ settings: { ...defaultSettings } });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().current?.index).toBe(0));
    fireEvent.keyDown(window, { key: "t", code: "KeyT" });
    await waitFor(() => expect(settings()?.["panel.grid_shown"]).toBe(false));
    expect(photosColumn()).not.toBeVisible();
    expect(screen.queryByRole("separator", { name: "Resize thumbnail column" })).toBeNull();
    fireEvent.keyDown(window, { key: "ArrowRight", code: "ArrowRight" });
    await waitFor(() => expect(grid().current?.index).toBe(1));
    fireEvent.click(screen.getByRole("button", { name: "Thumbnails" }));
    await waitFor(() => expect(photosColumn()).toBeVisible());
  });

  it("toggles each panel exactly once for one press of T or I", async () => {
    useSettingsStore.setState({ settings: { ...defaultSettings } });
    keepSettings();
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().current?.index).toBe(0));
    const patches = () => calls("app_settings_set").map((c) => (c.args as { patch: unknown }).patch);
    fireEvent.keyDown(window, { key: "t", code: "KeyT" });
    await waitFor(() => expect(settings()?.["panel.grid_shown"]).toBe(false));
    fireEvent.keyDown(window, { key: "i", code: "KeyI" });
    await waitFor(() => expect(settings()?.["panel.details_shown"]).toBe(false));
    // Nothing else toggles them back afterwards.
    await new Promise((r) => setTimeout(r, 50));
    expect(patches()).toEqual([{ "panel.grid_shown": false }, { "panel.details_shown": false }]);
    expect(settings()?.["panel.grid_shown"]).toBe(false);
    // The hidden thumbnails' button sits at the preview's left edge.
    expect(document.querySelector(".ow-review__edge--left")).not.toBeNull();
  });

  it("keeps the grid's scrollbar room so a width settles (no shaking)", () => {
    const css = read(join(srcDir, "features/grid/Grid.css"));
    expect(css).toMatch(/\.ow-grid \{[^}]*scrollbar-gutter: stable;/);
    expect(css).toMatch(/\.ow-grid--across \{[^}]*overflow-x: scroll;/);
  });

  it("keeps the map's markers when the thumbnails move to the bottom and back, and when hidden and shown", async () => {
    // Issue 216: the map made again in the strip waited for a new result
    // that never came, so it showed the same area with no markers.
    fake = createFakeApi({
      ...createQueryFake(photos(5).map((p, i) => ({ ...p, place: [49 + i, -123] as [number, number] }))).handlers,
      photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg`, rel_path: `day/p${id}.jpg` }),
    });
    setApi(fake);
    keepSettings();
    const setPoints = vi.spyOn(PhotoMap.prototype, "setPoints");
    useSettingsStore.setState({ settings: { ...defaultSettings, view: "map" } });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(setPoints.mock.calls.at(-1)?.[0]).toHaveLength(5));
    const runs = calls("query_run").length;
    const maps = calls("query_map").length;
    setPoints.mockClear();

    await act(() => setGridPlace("bottom"));
    expect(document.querySelector(".ow-review--bottom .ow-review__photos .ow-map__stage")).not.toBeNull();
    await waitFor(() => expect(setPoints.mock.calls.at(-1)?.[0]).toHaveLength(5));
    expect(screen.queryByText(mapText.empty)).toBeNull();

    // T hides the strip and shows it again: the same map stays, markers and all.
    const stage = document.querySelector(".ow-map__stage");
    fireEvent.keyDown(window, { key: "t", code: "KeyT" });
    await waitFor(() => expect(settings()?.["panel.grid_shown"]).toBe(false));
    fireEvent.keyDown(window, { key: "t", code: "KeyT" });
    await waitFor(() => expect(settings()?.["panel.grid_shown"]).toBe(true));
    expect(document.querySelector(".ow-map__stage")).toBe(stage);

    setPoints.mockClear();
    await act(() => setGridPlace("left"));
    await waitFor(() => expect(setPoints.mock.calls.at(-1)?.[0]).toHaveLength(5));
    // Moving the panel asks Rust for nothing new.
    expect(calls("query_run").length).toBe(runs);
    expect(calls("query_map").length).toBe(maps);
    setPoints.mockRestore();
  });

  it("sizes the strip in rows of tiles: one row, or two when the tile size allows", async () => {
    useSettingsStore.setState({
      settings: { ...defaultSettings, "panel.grid_place": "bottom", "panel.strip_height": 900 },
    });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().current?.index).toBe(0));
    // Two rows of 160 px tiles under the view switch, not the 900 stored.
    const two = STRIP_CHROME + 2 * 12 + 2 * 160 + 8;
    expect(photosColumn()?.style.height).toBe(`${two}px`);
    const strip = screen.getByRole("separator", { name: "Resize thumbnail strip" });
    expect(strip).toHaveAttribute("aria-valuemax", String(two));
    // A double-click puts it back to one row.
    fireEvent.doubleClick(strip);
    await waitFor(() => expect(settings()?.["panel.strip_height"]).toBe(STRIP_CHROME + 2 * 12 + 160));
  });

  it("puts the thumbnails in a strip along the bottom that scrolls sideways", async () => {
    useSettingsStore.setState({
      settings: { ...defaultSettings, "panel.grid_place": "bottom", "panel.strip_height": 240 },
    });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().current?.index).toBe(0));
    expect(document.querySelector(".ow-review--bottom")).not.toBeNull();
    expect(photosColumn()?.style.height).toBe("240px");
    expect(screen.getByRole("grid")).toHaveClass("ow-grid--across");
    const strip = screen.getByRole("separator", { name: "Resize thumbnail strip" });
    expect(strip).toHaveAttribute("aria-orientation", "horizontal");
    expect(screen.queryByRole("separator", { name: "Resize thumbnail column" })).toBeNull();
    // Up makes the strip taller: it sits under the splitter.
    fireEvent.keyDown(strip, { key: "ArrowUp" });
    expect(strip).toHaveAttribute("aria-valuenow", "256");
    fireEvent.keyDown(window, { key: "ArrowRight", code: "ArrowRight" });
    await waitFor(() => expect(grid().current?.index).toBe(1));
  });
});
