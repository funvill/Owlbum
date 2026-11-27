import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { seriousFindings } from "../../test/axe";
import { timelineText } from "../../text/timeline";
import { useSettingsStore } from "../app";
import { BrowseGrid, resetGrid, useGridStore } from "../grid";
import { ViewBar } from "./ViewBar";

// jsdom does no layout: the grid's box is given a size (4 columns of 160).
const box = { width: 4 * 168 + 16, height: 600 };

beforeAll(() => {
  const isGrid = (el: Element) => el.classList.contains("ow-grid");
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
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterAll(() => {
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
  vi.unstubAllGlobals();
});

/** Five photos over two July days, out of name order, and one with no date. */
const PHOTOS: FakePhoto[] = [
  { id: 1, rel_path: "e.jpg", date: "2026-07-15T08:00:00" },
  { id: 2, rel_path: "d.jpg", date: "2026-07-14T23:30:00" },
  { id: 3, rel_path: "c.jpg", date: "2026-07-14T09:00:00" },
  { id: 4, rel_path: "b.jpg", date: "2026-07-14T10:00:00" },
  { id: 5, rel_path: "a.jpg", date: null },
];

let fake: FakeApi;

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(createQueryFake(PHOTOS).handlers);
  setApi(fake);
});

afterEach(() => resetGrid());

function screenWith() {
  return render(
    <div>
      <ViewBar />
      <BrowseGrid />
    </div>,
  );
}

const runs = () => fake.calls.filter((c) => c.command === "query_run").map((c) => (c.args as { query: { view: string; sort: string; timeline_unit: string } }).query);

describe("the timeline", () => {
  it("is chosen in the view switch, which keeps the sort and saves the view", async () => {
    screenWith();
    useGridStore.getState().setQuery({ ...useGridStore.getState().query, sort: "name" });
    const tabs = await screen.findByRole("tablist", { name: timelineText.views.label });
    expect(screen.getByRole("tab", { name: timelineText.views.map })).toBeEnabled();
    await userEvent.click(screen.getByRole("tab", { name: timelineText.views.timeline }));
    await waitFor(() => expect(runs().at(-1)?.view).toBe("timeline"));
    expect(runs().at(-1)?.sort).toBe("name");
    await waitFor(() => expect(useSettingsStore.getState().settings?.view).toBe("timeline"));
    expect(tabs).toBeInTheDocument();
  });

  it("draws a heading over each day, in date order, and the unit switch shows the unit used", async () => {
    screenWith();
    await userEvent.click(await screen.findByRole("tab", { name: timelineText.views.timeline }));
    const first = await screen.findByRole("button", { name: /Tuesday 14 July 2026 · 3 photos/ });
    expect(first).toHaveTextContent("Tuesday 14 July 2026 · 3 photos");
    expect(screen.getByRole("button", { name: /Wednesday 15 July 2026 · 1 photo\./ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /No date · 1 photo\./ })).toBeInTheDocument();
    const units = screen.getByRole("radiogroup", { name: timelineText.unit.label });
    expect(units).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: timelineText.unit.day })).toBeChecked();
    expect(await seriousFindings()).toEqual([]);
  });

  it("groups by month once the user picks Month, and saves the pick", async () => {
    screenWith();
    await userEvent.click(await screen.findByRole("tab", { name: timelineText.views.timeline }));
    await userEvent.click(await screen.findByRole("radio", { name: timelineText.unit.month }));
    expect(await screen.findByRole("button", { name: /July 2026 · 4 photos/ })).toBeInTheDocument();
    expect(runs().at(-1)?.timeline_unit).toBe("month");
    await waitFor(() => expect(useSettingsStore.getState().settings?.timeline_unit).toBe("month"));
  });

  it("selects a group with a click on its heading, and adds it with Ctrl", async () => {
    screenWith();
    await userEvent.click(await screen.findByRole("tab", { name: timelineText.views.timeline }));
    const day = await screen.findByRole("button", { name: /Tuesday 14 July 2026/ });
    await waitFor(() => expect(screen.getAllByRole("gridcell")).toHaveLength(5));
    await userEvent.click(day);
    await waitFor(() => expect(useGridStore.getState().selection).toEqual([3, 4, 2]));
    expect(useGridStore.getState().current).toEqual({ id: 3, index: 0 });
    fireEvent.click(screen.getByRole("button", { name: /No date/ }), { ctrlKey: true });
    await waitFor(() => expect(useGridStore.getState().selection).toEqual([3, 4, 2, 5]));
    expect(useGridStore.getState().current).toEqual({ id: 5, index: 4 });
  });

  it("opens in the view last chosen", async () => {
    const settings = await fake.invoke("app_settings_set", { patch: { view: "timeline", timeline_unit: "month" } });
    useSettingsStore.setState({ settings });
    screenWith();
    await waitFor(() => expect(runs()[0]).toMatchObject({ view: "timeline", timeline_unit: "month" }));
    expect(screen.getByRole("tab", { name: timelineText.views.timeline })).toHaveAttribute("aria-selected", "true");
  });
});

describe("the heading text", () => {
  it("reads the key as written, in any time zone", () => {
    expect(timelineText.heading("2026-07-14", 37)).toBe("Tuesday 14 July 2026 · 37 photos");
    expect(timelineText.heading("2026-07", 2140)).toBe("July 2026 · 2,140 photos");
    expect(timelineText.heading("none", 1)).toBe("No date · 1 photo");
  });
});

describe("the month rail", () => {
  /** December 2025, January 2026, two days of July 2026 and one with no date. */
  const SPREAD: FakePhoto[] = [
    { id: 1, rel_path: "a.jpg", date: "2025-12-01T10:00:00" },
    { id: 2, rel_path: "b.jpg", date: "2026-01-05T10:00:00" },
    { id: 3, rel_path: "c.jpg", date: "2026-07-14T10:00:00" },
    { id: 4, rel_path: "d.jpg", date: "2026-07-15T10:00:00" },
    { id: 5, rel_path: "e.jpg", date: null },
  ];

  const rail = () => screen.getByRole("navigation", { name: timelineText.rail.label });
  const gridBox = () => screen.getByRole("grid");

  async function timeline(photos: FakePhoto[]) {
    fake = createFakeApi(createQueryFake(photos).handlers);
    setApi(fake);
    screenWith();
    await userEvent.click(await screen.findByRole("tab", { name: timelineText.views.timeline }));
    // More than 90 days: grouped by month.
    await screen.findByRole("button", { name: /^December 2025 · 1 photo/ });
  }

  it("lists the years and the months that have photos, and marks the month in view", async () => {
    await timeline(SPREAD);
    const buttons = Array.from(rail().querySelectorAll("button")).map((b) => b.getAttribute("aria-label") ?? b.textContent);
    expect(buttons).toEqual(["2025", "December 2025", "2026", "January 2026", "July 2026"]);
    expect(screen.getByRole("button", { name: "July 2026" })).toHaveTextContent("Jul");
    expect(screen.getByRole("button", { name: "December 2025" })).toHaveAttribute("aria-current", "true");
    expect(await seriousFindings()).toEqual([]);
  });

  it("scrolls a month's first heading to the top on a click, and marks it", async () => {
    await timeline(SPREAD);
    await userEvent.click(screen.getByRole("button", { name: "July 2026" }));
    // Each of the first two months: a 32 px heading, one 168 px row and 8 px of space.
    expect(gridBox().scrollTop).toBe(12 + 2 * (32 + 168 + 8));
    fireEvent.scroll(gridBox());
    await waitFor(() => expect(screen.getByRole("button", { name: "July 2026" })).toHaveAttribute("aria-current", "true"));
    expect(screen.getByRole("button", { name: "December 2025" })).not.toHaveAttribute("aria-current");
    // A year goes to its first month.
    await userEvent.click(screen.getByRole("button", { name: "2026" }));
    expect(gridBox().scrollTop).toBe(12 + 32 + 168 + 8);
  });

  it("shows only the years when the months do not fit", async () => {
    const height = box.height;
    box.height = 100;
    try {
      await timeline(SPREAD);
      const buttons = Array.from(rail().querySelectorAll("button")).map((b) => b.textContent);
      expect(buttons).toEqual(["2025", "2026"]);
      expect(screen.getByRole("button", { name: "2025" })).toHaveAttribute("aria-current", "true");
    } finally {
      box.height = height;
    }
  });

  it("is not shown with fewer than two months", async () => {
    screenWith();
    await userEvent.click(await screen.findByRole("tab", { name: timelineText.views.timeline }));
    await screen.findByRole("button", { name: /Tuesday 14 July 2026/ });
    expect(screen.queryByRole("navigation", { name: timelineText.rail.label })).toBeNull();
  });
});
