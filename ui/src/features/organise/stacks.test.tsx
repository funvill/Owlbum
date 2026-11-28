import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { defaultSettings } from "../../api/fake/app";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { useSettingsStore } from "../app";
import { BrowseGrid, resetGrid, selectById, useGridStore } from "../grid";
import { FilterBar } from "./FilterBar";
import { FiltersButton } from "./FiltersButton";
import { dragTargets, selectedTargets } from "../files";
import { toggleFlag } from "./flags";
import { nudge } from "./nudge";
import { toggleStack } from "./stacks";

// jsdom does no layout: the grid's box is given a size, and ResizeObserver,
// which jsdom lacks, is a stub (as in the grid's own tests).
class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

beforeAll(() => {
  for (const [prop, size] of [
    ["clientWidth", 420],
    ["clientHeight", 600],
  ] as const) {
    Object.defineProperty(HTMLElement.prototype, prop, {
      configurable: true,
      get(this: HTMLElement) {
        return this.classList.contains("ow-grid") ? size : 0;
      },
    });
  }
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
});

afterAll(() => {
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
  vi.unstubAllGlobals();
});

/** A burst of three (id 1; 3 is its keeper) and a plain photo, a minute apart. */
const ALBUM: FakePhoto[] = [
  { id: 1, rel_path: "a.jpg", date: "2024-06-01T10:01:00", burst_id: 1 },
  { id: 2, rel_path: "b.jpg", date: "2024-06-01T10:02:00", burst_id: 1 },
  {
    id: 3,
    rel_path: "c.jpg",
    date: "2024-06-01T10:03:00",
    burst_id: 1,
    keeper: true,
  },
  { id: 4, rel_path: "d.jpg", date: "2024-06-02T10:00:00" },
];

let fake: FakeApi;
const grid = () => useGridStore.getState();
const runs = () => fake.calls.filter((c) => c.command === "query_run");
const tiles = () => screen.queryAllByRole("gridcell");
const names = () =>
  tiles().map((t) =>
    t.getAttribute("aria-label")?.replace(", score not worked out yet", ""),
  );

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(createQueryFake(ALBUM).handlers);
  setApi(fake);
});

afterEach(() => resetGrid());

function page() {
  render(
    <>
      <FiltersButton />
      <FilterBar />
      <BrowseGrid onStack={toggleStack} />
    </>,
  );
}

describe("stacks", () => {
  it("folds a burst into its keeper with a ×N badge, and counts every photo", async () => {
    page();
    await waitFor(() =>
      expect(names()).toEqual(["c.jpg, stack of 3", "d.jpg"]),
    );
    expect(screen.getByText("4 photos")).toBeInTheDocument();
    const badge = within(tiles()[0]!).getByRole("button", {
      name: "Show 3 similar photos",
    });
    expect(badge).toHaveTextContent("×3");
    expect(badge).toHaveAttribute("title", "Show 3 similar photos");
  });

  it("opens the stack from its badge with the outline on every member, and folds it again", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    await userEvent.click(
      within(tiles()[0]!).getByRole("button", {
        name: "Show 3 similar photos",
      }),
    );
    await waitFor(() => expect(tiles()).toHaveLength(4));
    expect(runs().at(-1)!.args).toMatchObject({ query: { open_stacks: [1] } });
    expect(names()).toEqual(["c.jpg, stack of 3", "a.jpg", "b.jpg", "d.jpg"]);
    expect(
      tiles().map((t) => t.classList.contains("ow-tile--stack-open")),
    ).toEqual([true, true, true, false]);

    await userEvent.click(
      within(tiles()[0]!).getByRole("button", {
        name: "Fold 3 similar photos",
      }),
    );
    await waitFor(() => expect(tiles()).toHaveLength(2));
    expect(runs().at(-1)!.args).toMatchObject({ query: { open_stacks: [] } });
  });

  it("gives the face the selection when a stack closes on one of its other members", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    await userEvent.click(
      within(tiles()[0]!).getByRole("button", {
        name: "Show 3 similar photos",
      }),
    );
    await waitFor(() => expect(tiles()).toHaveLength(4));
    await userEvent.click(tiles()[2]!);
    expect(grid().current).toEqual({ id: 2, index: 2 });
    await userEvent.click(
      within(tiles()[0]!).getByRole("button", {
        name: "Fold 3 similar photos",
      }),
    );
    await waitFor(() => expect(grid().current).toEqual({ id: 3, index: 0 }));
    expect(grid().selection).toEqual([3]);
  });

  it("acts on the face only when a closed stack is clicked: star, hide, nudge, delete and drag", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    await userEvent.click(tiles()[0]!);
    expect(grid().selection).toEqual([3]);
    await waitFor(() => expect(grid().current).toEqual({ id: 3, index: 0 }));
    const sent = (command: string) =>
      fake.calls
        .filter((c) => c.command === command)
        .map((c) => (c.args as { ids: number[] }).ids);
    await toggleFlag("star", "selection");
    await toggleFlag("hidden", "selection");
    await nudge(1, "selection");
    expect(sent("photo_set_info")).toEqual([[3], [3]]);
    expect(sent("photo_nudge")).toEqual([[3]]);
    expect(selectedTargets()).toEqual([3]);
    expect(dragTargets(3)).toEqual([3]);
  });

  it("puts the face in the selection when a stack folds on a selected member that is not current", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    await userEvent.click(
      within(tiles()[0]!).getByRole("button", {
        name: "Show 3 similar photos",
      }),
    );
    await waitFor(() => expect(tiles()).toHaveLength(4));
    const user = userEvent.setup();
    await user.click(tiles()[2]!);
    await user.keyboard("{Control>}");
    await user.click(tiles()[3]!);
    await user.keyboard("{/Control}");
    expect(grid().current).toEqual({ id: 4, index: 3 });
    expect([...grid().selection].sort()).toEqual([2, 4]);
    await userEvent.click(
      within(tiles()[0]!).getByRole("button", {
        name: "Fold 3 similar photos",
      }),
    );
    await waitFor(() => expect(grid().current).toEqual({ id: 4, index: 1 }));
    expect([...grid().selection].sort()).toEqual([3, 4]);
  });

  it("opens the stack a folded photo is in when it must be shown", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    await act(() => selectById(2));
    expect(grid().query.open_stacks).toEqual([1]);
    expect(grid().current).toEqual({ id: 2, index: 2 });
  });

  it("counts every photo of a closed stack in its timeline heading", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    act(() =>
      grid().setQuery({
        ...grid().query,
        view: "timeline",
        timeline_unit: "day",
      }),
    );
    await waitFor(() =>
      expect(screen.getByText(/· 3 photos$/)).toBeInTheDocument(),
    );
    expect(screen.getByText(/· 1 photo$/)).toBeInTheDocument();
    expect(tiles()).toHaveLength(2);
  });

  it("turns stacking off and on with Stack similar, remembered as the setting", async () => {
    page();
    await waitFor(() => expect(tiles()).toHaveLength(2));
    // Stack similar is in the Filters panel (issue 196).
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    const box = screen.getByRole("checkbox", { name: "Stack similar" });
    expect(box).toBeChecked();
    await userEvent.click(box);
    await waitFor(() => expect(tiles()).toHaveLength(4));
    expect(runs().at(-1)!.args).toMatchObject({ query: { stack: false } });
    expect(names()).toEqual(["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    expect(useSettingsStore.getState().settings?.stack_similar).toBe(false);
  });

  it("starts with stacking off when the setting says so", async () => {
    useSettingsStore.setState({
      settings: { ...defaultSettings, stack_similar: false },
    });
    page();
    await waitFor(() => expect(tiles()).toHaveLength(4));
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    expect(
      screen.getByRole("checkbox", { name: "Stack similar" }),
    ).not.toBeChecked();
  });
});
