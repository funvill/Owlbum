import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { defaultSettings } from "../../api/fake/app";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { Review } from "./Review";

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({ id: i + 1, rel_path: `day/p${i + 1}.jpg` }));
}

let fake: FakeApi;
const grid = () => useGridStore.getState();
const calls = (command: string) => fake.calls.filter((c) => c.command === command);

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi({
    ...createQueryFake(photos(5)).handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg`, rel_path: `day/p${id}.jpg` }),
  });
  setApi(fake);
});

afterEach(() => resetGrid());

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
    fireEvent.pointerMove(thumbs, { pointerId: 1, clientX: 0 });
    expect(thumbs).toHaveAttribute("aria-valuenow", "200");
    expect(saves()).toEqual([]);
    fireEvent.pointerUp(thumbs, { pointerId: 1, clientX: 0 });
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
