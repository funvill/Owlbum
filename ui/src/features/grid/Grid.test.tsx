import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi, thumbUrl } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakeFormats } from "../../api/fake/app";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import type { Row } from "../../api/bindings/Row";
import { seriousFindings } from "../../test/axe";
import { gridText } from "../../text/grid";
import { Grid } from "./Grid";
import { BLOCK, FETCHES } from "./blocks";
import { MAX_SCROLL, PAD, gridLayout, scrollRatio } from "./layout";
import { resetGrid, useGridStore } from "./store";
import { ThumbSize } from "./ThumbSize";

// jsdom does no layout: the grid's box is given a size here, and
// ResizeObserver, which jsdom lacks, is a stub the test can fire.
const box = { width: 420, height: 600 };
const observers = new Set<() => void>();

class FakeResizeObserver {
  private readonly fire: () => void;
  constructor(callback: ResizeObserverCallback) {
    this.fire = () => callback([], this as unknown as ResizeObserver);
  }
  observe() {
    observers.add(this.fire);
  }
  unobserve() {}
  disconnect() {
    observers.delete(this.fire);
  }
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
  // Remove the stubs: the getters on Element.prototype show through again.
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
  vi.unstubAllGlobals();
});

function photos(n: number, extra: (i: number) => Partial<FakePhoto> = () => ({})): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({
    id: i + 1,
    rel_path: `2024/IMG_${i + 1}.jpg`,
    // A distinct date each, so the fake sorts quickly.
    date: new Date(Date.UTC(2000, 0, 1) + i * 60_000).toISOString().slice(0, 19),
    ...extra(i),
  }));
}

let fake: FakeApi;

/** A fake backend over `list`. `overrides` may wrap the album's own handlers, so tokens stay valid. */
function useAlbum(
  list: FakePhoto[],
  overrides: (album: ReturnType<typeof createQueryFake>) => Parameters<typeof createFakeApi>[0] = () => ({}),
) {
  const album = createQueryFake(list);
  fake = createFakeApi({ ...album.handlers, ...overrides(album) });
  setApi(fake);
  return album;
}

const windows = () =>
  fake.calls.filter((c) => c.command === "query_window").map((c) => c.args as { offset: number; limit: number });

const gridBox = () => document.querySelector(".ow-grid") as HTMLElement;
const tiles = () => screen.queryAllByRole("gridcell");

async function scrollTo(top: number) {
  const el = gridBox();
  el.scrollTop = top;
  fireEvent.scroll(el);
  await act(async () => {
    await new Promise((r) => requestAnimationFrame(() => r(null)));
  });
}

beforeEach(() => {
  box.width = 420;
  box.height = 600;
  resetGrid();
});

afterEach(() => {
  resetGrid();
});

describe("the grid", () => {
  it("is a grid of the whole result, drawing only the rows in view", async () => {
    useAlbum(photos(1000));
    render(<Grid />);
    const grid = await screen.findByRole("grid", { name: gridText.label });
    expect(grid).toHaveAttribute("aria-multiselectable", "true");
    await waitFor(() => expect(grid).toHaveAttribute("aria-rowcount", "500"));
    expect(grid).toHaveAttribute("aria-colcount", "2");
    const l = gridLayout(420, 1000, 160);
    const inView = Math.ceil(600 / l.stride) + 4;
    expect(tiles().length).toBeLessThanOrEqual(inView * 2);
    expect(tiles().length).toBeGreaterThan(0);
    const rows = screen.getAllByRole("row");
    expect(rows[0]).toHaveAttribute("aria-rowindex", "1");
  });

  it("shows placeholders until rows arrive, then the small thumbnails", async () => {
    let answer: (() => void) | undefined;
    useAlbum(photos(10), (album) => ({
      query_window: (args) =>
        new Promise<Row[]>((resolve) => {
          answer = () => resolve(album.handlers.query_window(args) as Row[]);
        }),
    }));
    render(<Grid />);
    await waitFor(() => expect(tiles().length).toBe(10));
    expect(tiles()[0]).toHaveAccessibleName(gridText.loading);
    expect(document.querySelector("img")).toBeNull();
    await act(async () => answer!());
    await waitFor(() => expect(tiles()[0]).toHaveAccessibleName("IMG_1.jpg, score not worked out yet"));
    const img = tiles()[0]!.querySelector("img")!;
    expect(img.getAttribute("src")).toBe(thumbUrl("tk-1", "s"));
    expect(img).toHaveAttribute("decoding", "async");
  });

  it("asks for medium thumbnails when a tile needs more than 512 pixels", async () => {
    useAlbum(photos(4));
    box.width = 700;
    vi.stubGlobal("devicePixelRatio", 2);
    try {
      render(<Grid />);
      act(() => useGridStore.getState().setThumbSize(320));
      await waitFor(() => expect(tiles()[0]!.querySelector("img")?.getAttribute("src")).toBe(thumbUrl("tk-1", "m")));
    } finally {
      vi.stubGlobal("devicePixelRatio", 1);
    }
  });

  it("gives each tile its path as a tooltip", async () => {
    useAlbum(photos(3));
    render(<Grid />);
    await waitFor(() => expect(tiles()[1]).toHaveAccessibleName(/IMG_2\.jpg/));
    vi.useFakeTimers();
    try {
      fireEvent.pointerEnter(tiles()[1]!.parentElement!);
      act(() => vi.advanceTimersByTime(600));
      expect(screen.getByRole("tooltip")).toHaveTextContent("2024/IMG_2.jpg");
    } finally {
      vi.useRealTimers();
    }
  });

  it("draws the marks of docs/04", async () => {
    // Animated formats come from the registry (app_formats), never from a list here.
    const gif = fakeFormats.find((f) => f.animated)!;
    const moving = { ...gif, id: "moving", extensions: [".moving"], label: "Moving" };
    useAlbum(
      [
        { id: 1, rel_path: "a.jpg", date: "2024-01-01T00:00:01", star: true, score: 74 },
        { id: 2, rel_path: "b.jpg", date: "2024-01-01T00:00:02" },
        { id: 3, rel_path: "c.mp4", date: "2024-01-01T00:00:03", kind: "video", duration_ms: 65_400 },
        { id: 4, rel_path: `d${gif.extensions[0]}`, date: "2024-01-01T00:00:04", duration_ms: 900 },
        { id: 5, rel_path: `e${moving.extensions[0]}`, date: "2024-01-01T00:00:05", duration_ms: 900 },
        { id: 6, rel_path: "f.mov", date: "2024-01-01T00:00:06", kind: "video", duration_ms: 3_725_000 },
      ],
    );
    fake.handle("app_formats", () => [...fakeFormats, moving]);
    // Rows as Rust would send them with a stack, an edit and a hidden photo shown.
    fake.handle("query_window", ({ offset, limit }) => {
      const rows: Row[] = [
        { id: 1, file_name: "a.jpg", rel_path: "a.jpg", star: true, score: 74 },
        { id: 2, file_name: "b.jpg", rel_path: "b.jpg", hidden: true },
        { id: 3, file_name: "c.mp4", rel_path: "c.mp4", kind: "video" as const, duration_ms: 65_400 },
        { id: 4, file_name: `d${gif.extensions[0]}`, rel_path: "d", duration_ms: 900 },
        { id: 5, file_name: `e${moving.extensions[0]}`, rel_path: "e", duration_ms: 900 },
        { id: 6, file_name: "f.mov", rel_path: "f.mov", kind: "video" as const, duration_ms: 3_725_000 },
      ].map((r) => ({
        thumb_key: `tk-${r.id}`,
        aspect: 1,
        kind: "image" as const,
        duration_ms: null,
        star: false,
        hidden: false,
        score: null,
        stack: r.id === 1 ? { id: 1, size: 3, open: false, face: true, place: 1 } : null,
        edit_plan: r.id === 1 ? {} : null,
        ...r,
      }));
      return rows.slice(offset, offset + limit);
    });
    render(<Grid onStar={() => {}} onStack={() => {}} />);
    await waitFor(() => expect(tiles()[0]).toHaveAccessibleName("a.jpg, starred, score 74, stack of 3, edited"));
    const [a, b, c, d, e, f] = tiles() as HTMLElement[];
    expect(within(a!).getByRole("button", { name: gridText.unstar })).toBeInTheDocument();
    expect(within(a!).getByText("74")).toBeInTheDocument();
    expect(within(a!).getByRole("button", { name: gridText.stackShow(3) })).toHaveTextContent("×3");
    expect(within(a!).getByLabelText(gridText.editedAlbum)).toBeInTheDocument();

    expect(b).toHaveAccessibleName("b.jpg, hidden, score not worked out yet");
    expect(within(b!).getByLabelText(gridText.hidden)).toBeInTheDocument();
    expect(b).toHaveClass("ow-tile--hidden");
    expect(within(b!).getByText("–")).toBeInTheDocument();
    expect(within(b!).getByLabelText(gridText.noScore)).toBeInTheDocument();
    expect(within(b!).getByRole("button", { name: gridText.star })).toBeInTheDocument();

    expect(within(c!).getByText("1:05")).toBeInTheDocument();
    expect(within(d!).getByText(gif.label)).toBeInTheDocument();
    expect(within(e!).getByText(moving.label)).toBeInTheDocument();
    expect(within(f!).getByText("1:02:05")).toBeInTheDocument();
    expect(within(c!).queryByText(gif.label)).toBeNull();
  });

  it("shows a warning and the file name when the thumbnail does not load", async () => {
    useAlbum(photos(2, (i) => (i === 1 ? { kind: "video" } : {})));
    render(<Grid />);
    await waitFor(() => expect(tiles()[0]!.querySelector("img")).not.toBeNull());
    fireEvent.error(tiles()[0]!.querySelector("img")!);
    fireEvent.error(tiles()[1]!.querySelector("img")!);
    await waitFor(() => expect(within(tiles()[0]!).getByText("IMG_1.jpg")).toBeInTheDocument());
    expect(tiles()[0]!.querySelector("img")).toBeNull();
    expect(tiles()[0]!.querySelector(".ow-tile__missing")).not.toBeNull();
    expect(tiles()[1]!.querySelector(".ow-tile__missing--video")).not.toBeNull();
  });

  it("marks the selection and the current photo, with one tile in the Tab order", async () => {
    useAlbum(photos(6));
    render(<Grid />);
    await waitFor(() => expect(tiles()[2]).toHaveAccessibleName(/IMG_3/));
    act(() =>
      useGridStore.getState().setSelection({
        current: { id: 3, index: 2 },
        anchor: { id: 2, index: 1 },
        selection: [2, 3],
      }),
    );
    const t = tiles();
    expect(t[1]).toHaveAttribute("aria-selected", "true");
    expect(t[2]).toHaveAttribute("aria-selected", "true");
    expect(t[0]).toHaveAttribute("aria-selected", "false");
    expect(t[2]).toHaveClass("ow-tile--current");
    expect(t[1]).not.toHaveClass("ow-tile--current");
    expect(t.filter((x) => x.tabIndex === 0)).toEqual([t[2]]);
  });

  it("hands clicks to the page, and placeholders take none", async () => {
    let answer: (() => void) | undefined;
    useAlbum(photos(4), (album) => ({
      query_window: (args) =>
        new Promise<Row[]>((resolve) => {
          answer = () => resolve(album.handlers.query_window(args) as Row[]);
        }),
    }));
    const onTileClick = vi.fn();
    const onOpen = vi.fn();
    const onStar = vi.fn();
    render(<Grid onTileClick={onTileClick} onOpen={onOpen} onStar={onStar} />);
    await waitFor(() => expect(tiles()).toHaveLength(4));
    await userEvent.click(tiles()[1]!);
    expect(onTileClick).not.toHaveBeenCalled();
    await act(async () => answer!());
    await waitFor(() => expect(tiles()[1]).toHaveAccessibleName(/IMG_2/));
    await userEvent.click(tiles()[1]!);
    expect(onTileClick).toHaveBeenCalledWith({ id: 2, index: 1 }, expect.objectContaining({ type: "click" }));
    await userEvent.dblClick(tiles()[2]!);
    expect(onOpen).toHaveBeenCalledWith({ id: 3, index: 2 });
    onTileClick.mockClear();
    await userEvent.click(within(tiles()[3]!).getByRole("button", { name: gridText.star }));
    expect(onStar).toHaveBeenCalledWith(expect.objectContaining({ id: 4 }));
    expect(onTileClick).not.toHaveBeenCalled();
  });

  it("says when no photos match the filter, with a way to clear it", async () => {
    // Starred photos only: none. The rest is the album's own engine, so its tokens hold.
    useAlbum(photos(3), (album) => ({
      query_run: (args) =>
        args.query.filter.star === "yes"
          ? {
              token: "none",
              revision: 0,
              count: 0,
              photos: 0,
              total: 3,
              album_photos: 3,
              filtered: true,
              panel_count: 1,
              chips: [],
              kept: null,
            }
          : album.handlers.query_run(args),
    }));
    render(<Grid />);
    act(() => useGridStore.getState().setQuery({ ...useGridStore.getState().query, filter: { star: "yes" } }));
    expect(await screen.findByText(gridText.noMatch)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: gridText.clearFilter }));
    await waitFor(() => expect(useGridStore.getState().result?.count).toBe(3));
    expect(useGridStore.getState().query.filter).toEqual({});
  });

  it("never says no photos match while the album holds none", async () => {
    useAlbum([]);
    render(<Grid />);
    await waitFor(() => expect(useGridStore.getState().result).not.toBeNull());
    expect(screen.queryByText(gridText.noMatch)).toBeNull();
  });

  it("changes columns with the thumbnail size, keeping the top-left photo in the top row", async () => {
    useAlbum(photos(1000));
    box.width = 1000;
    render(<Grid />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(grid).toHaveAttribute("aria-colcount", "5"));
    const before = gridLayout(1000, 1000, 160);
    await scrollTo(40 * before.stride + 5); // photo 200 at the top-left
    act(() => useGridStore.getState().setThumbSize(320));
    await waitFor(() => expect(grid).not.toHaveAttribute("aria-colcount", "5"));
    const after = gridLayout(1000, 1000, 320);
    expect(gridBox().scrollTop).toBe(Math.floor(200 / after.cols) * after.stride);
  });

  it("scrolls just enough to show the current photo when it changes", async () => {
    useAlbum(photos(1000));
    render(<Grid />);
    await screen.findByRole("grid");
    const l = gridLayout(420, 1000, 160);
    act(() => useGridStore.getState().setSelection({ current: { id: 201, index: 200 }, anchor: null, selection: [201] }));
    await waitFor(() => expect(gridBox().scrollTop).toBe(PAD + 100 * l.stride + l.side + PAD - 600));
    act(() => useGridStore.getState().setSelection({ current: { id: 3, index: 2 }, anchor: null, selection: [3] }));
    await waitFor(() => expect(gridBox().scrollTop).toBe(l.stride));
  });

  it("follows the size of its box", async () => {
    useAlbum(photos(100));
    render(<Grid />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(grid).toHaveAttribute("aria-colcount", "2"));
    box.width = 1000;
    act(() => observers.forEach((fire) => fire()));
    await waitFor(() => expect(grid).toHaveAttribute("aria-colcount", "5"));
  });

  it("has no serious accessibility findings", async () => {
    useAlbum(photos(20, (i) => ({ star: i % 3 === 0, score: i })));
    render(<Grid onStar={() => {}} />);
    await waitFor(() => expect(tiles()[0]).toHaveAccessibleName(/IMG_1\.jpg/));
    expect(await seriousFindings()).toEqual([]);
  });
});

describe("100,000 photos", () => {
  const N = 100_000;
  const all = photos(N);

  it("fetches and draws only the window in view, even across a drag to the end", async () => {
    let inFlight = 0;
    let most = 0;
    useAlbum(all, (album) => ({
      query_window: async (args) => {
        inFlight++;
        most = Math.max(most, inFlight);
        await new Promise((r) => setTimeout(r, 1));
        inFlight--;
        return album.handlers.query_window(args) as Row[];
      },
    }));
    render(<Grid />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(grid).toHaveAttribute("aria-rowcount", String(N / 2)), { timeout: 10_000 });
    await waitFor(() => expect(tiles()[0]).toHaveAccessibleName(/IMG_1\.jpg/));

    const l = gridLayout(420, N, 160);
    expect(l.total).toBeGreaterThan(MAX_SCROLL);
    const inner = gridBox().firstElementChild as HTMLElement;
    // jsdom writes CSS numbers to 6 significant figures.
    expect(Math.abs(parseFloat(inner.style.height) - l.height)).toBeLessThan(10);
    const drawnMax = (Math.ceil(600 / l.stride) + 8) * 2;
    expect(tiles().length).toBeLessThanOrEqual(drawnMax);
    // At the start: the first block only.
    expect(windows().map((w) => w.offset)).toEqual([0]);

    // Drag the scrollbar to the very end.
    await scrollTo(l.height - 600);
    await waitFor(() => expect(tiles().at(-1)).toHaveAccessibleName(/IMG_100000\.jpg/), { timeout: 5000 });
    const asked = windows().map((w) => w.offset);
    for (const w of windows()) {
      expect(w.offset % BLOCK).toBe(0);
      expect(w.limit).toBe(BLOCK);
    }
    // Nothing between the start and the end was fetched.
    expect(asked.filter((o) => o > 0 && o < N - 3 * BLOCK)).toEqual([]);
    expect(asked).toContain(N - BLOCK);
    expect(most).toBeLessThanOrEqual(FETCHES);
    expect(tiles().length).toBeLessThanOrEqual(drawnMax);

    // Rows are placed near the drawn window, never at their real height.
    for (const row of screen.getAllByRole("row")) {
      const y = Number(/translateY\((-?[\d.]+)px\)/.exec(row.style.transform)?.[1]);
      expect(y).toBeLessThan(100 * l.stride);
    }

    // The middle, then back.
    await scrollTo((l.height - 600) / 2);
    await waitFor(() => expect(tiles()[tiles().length >> 1]).toHaveAccessibleName(/IMG_\d+\.jpg/));
    expect(windows().length).toBeLessThan(12);
  }, 30_000);

  it("keeps the view still above the height limit when a scan adds photos", async () => {
    const album = useAlbum(all);
    render(<Grid />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(grid).toHaveAttribute("aria-rowcount", String(N / 2)), { timeout: 10_000 });
    const l = gridLayout(420, N, 160);
    await scrollTo((l.height - 600) / 2);
    const firstRow = () => screen.getAllByRole("row")[0]!.getAttribute("aria-rowindex");
    const before = firstRow();
    const revision = album.change((ps) => [
      ...ps,
      ...Array.from({ length: 2000 }, (_, i) => ({ id: N + 1 + i, rel_path: `new/${i}.jpg`, date: "2099-01-01T00:00:00" })),
    ]);
    fake.emit("album_changed", { reason: "scan", revision });
    await waitFor(() => expect(grid).toHaveAttribute("aria-rowcount", String((N + 2000) / 2)), { timeout: 10_000 });
    await act(async () => {
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    });
    expect(firstRow()).toBe(before);
  }, 30_000);

  it("moves about one row per wheel notch above the height limit", async () => {
    useAlbum(all);
    render(<Grid />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(grid).toHaveAttribute("aria-rowcount", String(N / 2)), { timeout: 10_000 });
    const l = gridLayout(420, N, 160);
    const ratio = scrollRatio(l, 600);
    const el = gridBox();
    el.scrollTop = 1000;
    const wheel = new WheelEvent("wheel", { deltaY: 100, cancelable: true, bubbles: true });
    el.dispatchEvent(wheel);
    expect(wheel.defaultPrevented).toBe(true);
    expect(el.scrollTop).toBeCloseTo(1000 + 100 / ratio, 3);
  }, 30_000);
});

describe("thumbnail size control", () => {
  it("is a slider from 96 to 320 in steps of 8 that sets the size", async () => {
    render(<ThumbSize />);
    const slider = screen.getByRole("slider", { name: gridText.thumbSize });
    expect(slider).toHaveAttribute("min", "96");
    expect(slider).toHaveAttribute("max", "320");
    expect(slider).toHaveAttribute("step", "8");
    expect(slider).toHaveValue("160");
    fireEvent.change(slider, { target: { value: "240" } });
    expect(useGridStore.getState().thumbSize).toBe(240);
  });
});
