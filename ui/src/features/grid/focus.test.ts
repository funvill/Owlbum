import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi } from "../../api";
import type { Row } from "../../api/bindings/Row";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { FOCUS_DELAY_MS, FOCUS_MAX, focusIds, reportShown, startFocus } from "./focus";
import { gridBlocks, resetGrid, useGridStore } from "./store";

let fake: FakeApi;
let stop: () => void;

const sent = () => fake.calls.filter((c) => c.command === "tagging_focus").map((c) => (c.args as { ids: number[] }).ids);

beforeEach(() => {
  vi.useFakeTimers();
  resetGrid();
  fake = createFakeApi();
  setApi(fake);
  vi.spyOn(gridBlocks(), "row").mockImplementation((i: number) => ({ id: 100 + i }) as Row);
  stop = startFocus();
});

afterEach(() => {
  stop();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("the focus list", () => {
  it("is the selected photo, then the rows on screen, at most 200", () => {
    useGridStore.setState({ current: { id: 105, index: 5 } });
    reportShown(0, 500);
    const ids = focusIds();
    expect(ids[0]).toBe(105);
    expect(ids.slice(1, 4)).toEqual([100, 101, 102]);
    expect(ids).toHaveLength(FOCUS_MAX);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("is sent 500 ms after the last change, and only when it differs", () => {
    reportShown(0, 2);
    vi.advanceTimersByTime(FOCUS_DELAY_MS - 1);
    useGridStore.setState({ current: { id: 7, index: 9 } });
    vi.advanceTimersByTime(FOCUS_DELAY_MS - 1);
    expect(sent()).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(sent()).toEqual([[7, 100, 101, 102]]);
    // A scroll that shows the same rows sends nothing new.
    reportShown(0, 1);
    reportShown(0, 2);
    vi.advanceTimersByTime(FOCUS_DELAY_MS * 4);
    expect(sent()).toHaveLength(1);
    reportShown(1, 3);
    vi.advanceTimersByTime(FOCUS_DELAY_MS);
    expect(sent()).toEqual([
      [7, 100, 101, 102],
      [7, 101, 102, 103],
    ]);
  });

  it("asks for nothing by itself (no polling)", () => {
    vi.advanceTimersByTime(60_000);
    expect(fake.calls).toEqual([]);
  });
});
