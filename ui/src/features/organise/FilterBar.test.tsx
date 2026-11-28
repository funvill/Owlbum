import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakeFormats } from "../../api/fake/app";
import { createQueryFake } from "../../api/fake/query";
import { defaultQuery } from "../../api/query";
import { seriousFindings } from "../../test/axe";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { focusOf } from "../../keys/match";
import { ViewBar } from "../timeline";
import { FilterBar } from "./FilterBar";
import { QueryBox } from "./QueryBox";
import { SortSelect } from "./SortSelect";

/** The search box (in the top bar) with the filter bar, as the window has them. */
function Bars() {
  return (
    <>
      <QueryBox />
      <FilterBar />
    </>
  );
}

let fake: FakeApi;
const runs = () => fake.calls.filter((c) => c.command === "query_run");
const grid = () => useGridStore.getState();

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(
    createQueryFake([
      { id: 1, rel_path: "b.jpg", date: "2024-01-01T00:00:00", score: 40, star: true },
      { id: 2, rel_path: "a.jpg", date: "2024-01-02T00:00:00", score: 70, people: ["Ann"] },
      { id: 3, rel_path: "Trip/c.jpg", date: "2024-01-03T00:00:00", score: null },
      { id: 4, rel_path: "d.jpg", date: "2024-01-04T00:00:00", hidden: true },
    ]).handlers,
  );
  setApi(fake);
});

afterEach(() => resetGrid());

describe("FilterBar", () => {
  it("is a search landmark without Sort or the search box, which moved out", () => {
    render(<FilterBar />);
    const bar = screen.getByRole("search", { name: "Filter photos" });
    expect(within(bar).queryByRole("combobox", { name: "Sort" })).not.toBeInTheDocument();
    expect(within(bar).queryByRole("searchbox")).not.toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "Filters" })).toBeInTheDocument();
  });

  it("has the Sort control beside the view switch, with its five options", async () => {
    const { container } = render(<ViewBar />);
    const sort = screen.getByRole("combobox", { name: "Sort" });
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
    render(<SortSelect />);
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

  it("shows Sort beside the view switch only in the grid view, keeping the chosen sort for its return", () => {
    for (const view of ["timeline", "map"] as const) {
      useGridStore.setState({ query: { ...defaultQuery(), view, sort: "name" } });
      const { unmount } = render(<ViewBar />);
      expect(screen.queryByRole("combobox", { name: "Sort" })).not.toBeInTheDocument();
      unmount();
    }
    useGridStore.setState({ query: { ...defaultQuery(), view: "grid", sort: "name" } });
    render(<ViewBar />);
    const views = screen.getByRole("tablist");
    const sort = screen.getByRole("combobox", { name: "Sort" });
    expect(sort).toHaveValue("name");
    expect(views.compareDocumentPosition(sort) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("opens the panel with its counts, focus on the first control", async () => {
    await grid().start();
    const { container } = render(<FilterBar />);
    expect(await screen.findByText("3 photos")).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Filters" });
    expect(button).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(button);
    const panel = screen.getByRole("group", { name: "Filters" });
    const stars = within(panel).getByRole("combobox", { name: "Stars" });
    expect(stars).toHaveFocus();
    expect(fake.calls.some((c) => c.command === "query_facets")).toBe(true);
    await vi.waitFor(() =>
      expect(within(stars).getAllByRole("option").map((o) => o.textContent)).toEqual([
        "All",
        "Starred (1)",
        "Not starred (2)",
      ]),
    );
    expect(within(panel).getByRole("combobox", { name: "Hidden" })).toHaveTextContent("Show (1)");
    expect(within(panel).getByRole("checkbox", { name: "Has people (1)" })).not.toBeChecked();
    // Two folders: the Folder select is shown, the top folder first.
    const folder = within(panel).getByRole("combobox", { name: "Folder" });
    expect(within(folder).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "All folders",
      "Top folder (2)",
      "Trip (1)",
    ]);
    expect(within(panel).getByText(/Pick a keyword/)).toBeInTheDocument();
    // The grid's keys do nothing inside the panel.
    expect(focusOf(stars)).toBe("dialog");
    expect(await seriousFindings(container)).toEqual([]);
  });

  it("filters by media type: no videos, or one format alone, from the registry's formats", async () => {
    const second = fakeFormats[1]!;
    fake = createFakeApi(
      createQueryFake([
        { id: 1, rel_path: "one", date: "2024-01-01T00:00:00" },
        { id: 2, rel_path: "two", date: "2024-01-02T00:00:00", kind: "video", duration_ms: 4000 },
        { id: 3, rel_path: "three", date: "2024-01-03T00:00:00", format: second.id },
      ]).handlers,
    );
    setApi(fake);
    await grid().start();
    const { container } = render(<FilterBar />);
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    const kinds = screen.getByRole("group", { name: "Kinds shown" });
    const videos = await within(kinds).findByRole("checkbox", { name: "Videos (1)" });
    expect(within(kinds).getByRole("checkbox", { name: "Stills (2)" })).toBeChecked();
    expect(videos).toBeChecked();
    const formats = screen.getByRole("group", { name: "Only these formats" });
    expect(within(formats).getAllByRole("checkbox")).toHaveLength(2);
    expect(within(formats).getByRole("checkbox", { name: `${fakeFormats[0]!.label} (2)` })).not.toBeChecked();

    await userEvent.click(videos);
    await vi.waitFor(() =>
      expect(runs().at(-1)!.args).toMatchObject({ query: { filter: { kinds: ["still", "animated", "raw"] } } }),
    );
    const row = await screen.findByRole("region", { name: "Active filters" });
    expect(within(row).getByText("No videos")).toBeInTheDocument();
    expect(within(row).getByText("Showing 2 of 3")).toBeInTheDocument();

    await userEvent.click(within(formats).getByRole("checkbox", { name: `${second.label} (1)` }));
    await vi.waitFor(() =>
      expect(runs().at(-1)!.args).toMatchObject({
        query: { filter: { kinds: ["still", "animated", "raw"], formats: [second.id] } },
      }),
    );
    expect(await within(row).findByText(second.label)).toBeInTheDocument();
    expect(within(row).getByText("Showing 1 of 3")).toBeInTheDocument();
    expect(await seriousFindings(container)).toEqual([]);
  });

  it("runs a chosen filter and shows its chip, the count and Clear filter", async () => {
    await grid().start();
    render(<FilterBar />);
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Stars" }), "yes");
    await vi.waitFor(() => expect(runs().at(-1)!.args).toMatchObject({ query: { filter: { star: "yes" }, want_facets: true } }));
    const row = await screen.findByRole("region", { name: "Active filters" });
    expect(within(row).getByText("Starred")).toBeInTheDocument();
    expect(within(row).getByText("Showing 1 of 3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Filters · 1" })).toBeInTheDocument();
    expect(screen.queryByText("3 photos")).not.toBeInTheDocument();

    await userEvent.click(within(row).getByRole("button", { name: "Remove filter Starred" }));
    await vi.waitFor(() => expect(grid().query.filter).toEqual({}));
    await vi.waitFor(() => expect(screen.queryByRole("region", { name: "Active filters" })).not.toBeInTheDocument());

    // The press on the chip was outside the panel, which closed it.
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    await userEvent.click(screen.getByRole("checkbox", { name: /Has people/ }));
    await screen.findByText("Showing 1 of 3");
    await userEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    await vi.waitFor(() => expect(grid().query).toMatchObject({ filter: {}, search: null }));
  });

  it("closes on Esc, returning focus to the button, and on a press outside", async () => {
    await grid().start();
    render(
      <>
        <FilterBar />
        <p>outside</p>
      </>,
    );
    const button = screen.getByRole("button", { name: "Filters" });
    await userEvent.click(button);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "Filters" })).not.toBeInTheDocument();
    expect(button).toHaveFocus();
    expect(grid().query.want_facets).toBe(false);

    await userEvent.click(button);
    expect(grid().query.want_facets).toBe(true);
    await userEvent.click(screen.getByText("outside"));
    expect(screen.queryByRole("group", { name: "Filters" })).not.toBeInTheDocument();
    expect(grid().facets).toBeNull();
  });

  it("has the search box with its hint line", async () => {
    const { container } = render(<Bars />);
    const box = screen.getByRole("searchbox", { name: "Search photos" });
    expect(box).toHaveAttribute("placeholder", "Search names, people, places, notes and keywords");
    expect(screen.getByText("star:yes")).toBeInTheDocument();
    expect(focusOf(box)).not.toBe("grid");
    expect(await seriousFindings(container)).toEqual([]);
  });

  it("turns query words into chips and the rest into search text", async () => {
    await grid().start();
    render(<Bars />);
    const box = screen.getByRole("searchbox", { name: "Search photos" });
    await userEvent.type(box, "ann star:no ");
    await vi.waitFor(() => expect(grid().query.filter).toEqual({ star: "no" }));
    expect(box).toHaveValue("ann ");
    const row = await screen.findByRole("region", { name: "Active filters" });
    expect(within(row).getByText("Not starred")).toBeInTheDocument();
    // The search text re-runs the query after a pause.
    await vi.waitFor(() => expect(grid().query.search).toBe("ann"));
    await vi.waitFor(() => expect(within(row).getByText("Showing 1 of 3")).toBeInTheDocument());

    await userEvent.type(box, "has:people");
    expect(grid().query.filter).toEqual({ star: "no" });
    await userEvent.keyboard("{Enter}");
    await vi.waitFor(() => expect(grid().query.filter).toEqual({ star: "no", has_people: true }));
    expect(box).toHaveValue("ann");

    await userEvent.click(within(row).getByRole("button", { name: "Clear filter" }));
    await vi.waitFor(() => expect(grid().query).toMatchObject({ filter: {}, search: null }));
    expect(box).toHaveValue("");
  });

  it("shows a bad query word's error and searches its text", async () => {
    await grid().start();
    render(<Bars />);
    const box = screen.getByRole("searchbox", { name: "Search photos" });
    await userEvent.type(box, "star:maybe lake{Enter}");
    expect(await screen.findByText(`"star:maybe" isn't a filter: try star:yes or star:no`)).toBeInTheDocument();
    expect(screen.queryByText("star:yes")).not.toBeInTheDocument();
    await vi.waitFor(() => expect(grid().query.search).toBe("star:maybe lake"));
    expect(grid().query.filter).toEqual({});
    await userEvent.type(box, "s");
    expect(screen.getByText("star:yes")).toBeInTheDocument();
  });

  it("never sets From after To", async () => {
    await grid().start();
    render(<FilterBar />);
    await userEvent.click(screen.getByRole("button", { name: "Filters" }));
    fireEvent.change(screen.getByLabelText("To"), { target: { value: "2024-01-02" } });
    await vi.waitFor(() => expect(grid().query.filter).toEqual({ to: "2024-01-02" }));
    expect(screen.getByLabelText("From")).toHaveAttribute("max", "2024-01-02");
    fireEvent.change(screen.getByLabelText("From"), { target: { value: "2024-01-05" } });
    fireEvent.change(screen.getByLabelText("From"), { target: { value: "2024-01-01" } });
    await vi.waitFor(() => expect(grid().query.filter).toEqual({ from: "2024-01-01", to: "2024-01-02" }));
    expect(await screen.findByText("2024-01-01 → 2024-01-02")).toBeInTheDocument();
  });
});

describe("FilterBar in a narrow window", () => {
  // A window under 1100 px (docs/09-visual-design.md, "The bars").
  beforeEach(() => {
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: query === "(max-width: 1099px)",
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  });
  afterEach(() => vi.unstubAllGlobals());

  it("hides the hint line, keeps its examples in the box's tooltip, and shows the count as the number alone", async () => {
    await grid().start();
    const { container } = render(<Bars />);
    const bar = screen.getByRole("search", { name: "Filter photos" });
    const box = screen.getByRole("searchbox", { name: "Search photos" });
    expect(screen.queryByText("star:yes")).not.toBeInTheDocument();
    expect(screen.queryByText("Try")).not.toBeInTheDocument();
    // The count: "3" on screen, "3 photos" to a screen reader and in its tooltip.
    const count = bar.querySelector(".ow-filter-bar__count")!;
    expect(count.firstChild?.textContent).toBe("3");
    expect(count).toHaveTextContent("3 photos");
    expect(count).toHaveAttribute("title", "3 photos");
    // The tooltip shows on keyboard focus; the box is the bar's first stop.
    await userEvent.tab();
    expect(box).toHaveFocus();
    expect(await screen.findByRole("tooltip")).toHaveTextContent(
      "Try star:yes shape:portrait from:2024-06-01 has:people tag:lake",
    );
    expect(await seriousFindings(container)).toEqual([]);
  });

  it("still shows a bad query word's error", async () => {
    await grid().start();
    render(<Bars />);
    await userEvent.type(screen.getByRole("searchbox", { name: "Search photos" }), "star:maybe{Enter}");
    expect(await screen.findByText(`"star:maybe" isn't a filter: try star:yes or star:no`)).toBeInTheDocument();
  });
});
