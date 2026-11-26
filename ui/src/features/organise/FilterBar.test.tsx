import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { createQueryFake } from "../../api/fake/query";
import { defaultQuery } from "../../api/query";
import { seriousFindings } from "../../test/axe";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { FilterBar } from "./FilterBar";

let fake: FakeApi;
const runs = () => fake.calls.filter((c) => c.command === "query_run");
const grid = () => useGridStore.getState();

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(
    createQueryFake([
      { id: 1, rel_path: "b.jpg", date: "2024-01-01T00:00:00", score: 40 },
      { id: 2, rel_path: "a.jpg", date: "2024-01-02T00:00:00", score: 70 },
      { id: 3, rel_path: "c.jpg", date: "2024-01-03T00:00:00", score: null },
    ]).handlers,
  );
  setApi(fake);
});

afterEach(() => resetGrid());

describe("FilterBar", () => {
  it("is a search landmark holding the Sort control with its five options", async () => {
    const { container } = render(<FilterBar />);
    const bar = screen.getByRole("search", { name: "Sort and filter" });
    const sort = within(bar).getByRole("combobox", { name: "Sort" });
    expect(sort).toHaveValue("date");
    expect(within(sort).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Date taken",
      "Photo score",
      "Most interesting",
      "Main colour",
      "File name",
    ]);
    expect(await seriousFindings(container)).toEqual([]);
  });

  it("runs the query in the chosen sort, keeping the selection", async () => {
    await grid().start();
    act(() => grid().setSelection({ current: { id: 2, index: 1 }, anchor: { id: 2, index: 1 }, selection: [2] }));
    render(<FilterBar />);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Sort" }), "score");
    await vi.waitFor(() => expect(runs()).toHaveLength(2));
    expect(runs()[1]!.args).toMatchObject({
      query: { ...defaultQuery(), sort: "score" },
      keep: { current: { id: 2, index: 1 }, selection: [2] },
    });
    // Highest score first: the current photo moves to the top and stays current.
    await vi.waitFor(() => expect(grid().current).toEqual({ id: 2, index: 0 }));
    expect(grid().selection).toEqual([2]);
    expect(screen.getByRole("combobox", { name: "Sort" })).toHaveValue("score");
  });

  it("is disabled in the timeline view, where the order is by date", () => {
    useGridStore.setState({ query: { ...defaultQuery(), view: "timeline", sort: "name" } });
    render(<FilterBar />);
    const sort = screen.getByRole("combobox", { name: "Sort" });
    expect(sort).toBeDisabled();
    expect(sort).toHaveValue("date");
  });
});
