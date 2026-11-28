import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import type { PhotoTags } from "../../api/bindings/PhotoTags";
import type { Query } from "../../api/bindings/QueryShape";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { fakeDetail } from "../../api/fake/photo";
import { idleProgress } from "../../api/fake/tagging";
import { defaultQuery } from "../../api/query";
import { Toasts, useToasts } from "../../controls";
import { whatsInItText as t } from "../../text/details";
import { useAlbumStore } from "../album";
import { resetTagging, useTaggingStore } from "../album/tagging";
import { useSettingsOpen } from "../app/settingsOpen";
import { resetGrid, useGridStore } from "../grid";
import { resetRegenerate, useRegenerateStore } from "./regenerate";
import { WhatsInIt } from "./WhatsInIt";

const tags = (over: Partial<PhotoTags> = {}): PhotoTags => ({
  state: "done",
  description: "A dog running along a sandy beach.",
  written_by: "Florence-2 base",
  keywords: ["beach", "dog"],
  objects: [
    { label: "dog", count: 1 },
    { label: "person", count: 2 },
  ],
  error: null,
  can_regenerate: true,
  waiting_for: null,
  ...over,
});

const detail = (over: Partial<PhotoTags> = {}, revision = 1): PhotoDetail =>
  fakeDetail(7, { tags: tags(over), revision });

let fake: FakeApi;
let queries: Query[];

function show(d: PhotoDetail, disabledReason: string | null = null) {
  return render(
    <>
      <WhatsInIt detail={d} disabledReason={disabledReason} />
      <Toasts />
    </>,
  );
}

beforeEach(() => {
  resetGrid();
  resetTagging();
  resetRegenerate();
  useToasts.setState({ toasts: [] });
  useSettingsOpen.setState({ open: false, section: null });
  fake = createFakeApi();
  setApi(fake);
  useAlbumStore.setState({ status: fakeStatus("C:\\Photos") });
  queries = [];
  useGridStore.setState({
    query: defaultQuery(),
    setQuery: (q: Query) => {
      queries.push(q);
      useGridStore.setState({ query: q });
    },
  });
});

afterEach(() => resetGrid());

describe("What's in it", () => {
  it("says tagging is off, with a Settings link at Photo tagging", async () => {
    const user = userEvent.setup();
    show(detail({ state: "off", description: null, keywords: [], objects: [], can_regenerate: false }));
    expect(screen.getByText(t.off)).toBeInTheDocument();
    expect(screen.queryByText(t.tags)).toBeNull();
    await user.click(screen.getByRole("button", { name: t.settings }));
    expect(useSettingsOpen.getState()).toEqual({ open: true, section: "tagging" });
  });

  it("shows the description, keywords and objects with counts", () => {
    show(detail());
    expect(screen.getByText("A dog running along a sandy beach.")).toBeInTheDocument();
    expect(screen.getByText(t.tags)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "beach" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getAllByRole("button", { name: "dog" })).toHaveLength(2);
    expect(screen.getByRole("button", { name: "person ×2" })).toBeInTheDocument();
    // One row of chips: the keywords, then the objects.
    expect(document.querySelectorAll(".ow-whatsinit__chips")).toHaveLength(1);
    expect(screen.queryByText(t.notWorking)).toBeNull();
  });

  it("says not looked at yet, or waits for the chosen model under other results", () => {
    const { unmount } = show(
      detail({ state: "waiting", description: null, keywords: [], objects: [], can_regenerate: false }),
    );
    expect(screen.getByText(t.notLookedAt)).toBeInTheDocument();
    unmount();
    show(detail({ state: "waiting", waiting_for: "Florence-2 large", can_regenerate: false }));
    expect(screen.getByText(t.waitingFor("Florence-2 large"))).toBeInTheDocument();
    expect(screen.getByText("A dog running along a sandy beach.")).toBeInTheDocument();
    expect(screen.queryByText(t.notLookedAt)).toBeNull();
  });

  it("says it couldn't look, with Try again, and keeps the part that worked", async () => {
    const user = userEvent.setup();
    show(detail({ state: "failed", description: null, objects: [], error: "The model refused it.", can_regenerate: false }));
    expect(screen.getByText(t.couldNotLook)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "beach" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.tryAgain }));
    expect(fake.calls.map((c) => c.command)).toContain("tagging_retry_failed");
  });

  it("shows the not-working note above the rest", () => {
    useTaggingStore.setState({ progress: { ...idleProgress, total: 3, not_working: ["describe"] } });
    show(detail());
    expect(screen.getByText(t.notWorking)).toBeInTheDocument();
  });

  it("toggles a keyword or object filter from its chip", async () => {
    const user = userEvent.setup();
    show(detail());
    await user.click(screen.getByRole("button", { name: "beach" }));
    expect(queries.at(-1)?.filter.tags).toEqual([{ kind: "keyword", value: "beach" }]);
    expect(screen.getByRole("button", { name: "beach" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "person ×2" }));
    expect(queries.at(-1)?.filter.tags).toEqual([
      { kind: "keyword", value: "beach" },
      { kind: "object", value: "person" },
    ]);
    await user.click(screen.getByRole("button", { name: "beach" }));
    await user.click(screen.getByRole("button", { name: "person ×2" }));
    expect(queries.at(-1)?.filter.tags).toBeUndefined();
    expect(screen.getByRole("button", { name: "beach" })).toHaveAttribute("aria-pressed", "false");
  });

  it("regenerates: busy text that lasts, one request per photo, and the reply replaces the tags", async () => {
    const user = userEvent.setup();
    let finish!: (v: PhotoTags) => void;
    fake.handle("tagging_regenerate", () => new Promise<PhotoTags>((res) => (finish = res)));
    const { unmount } = show(detail());
    await user.click(screen.getByRole("button", { name: t.regenerateLabel }));
    expect(screen.getByRole("button", { name: t.regenerateLabel })).toHaveTextContent(t.regenerating);
    expect(screen.getByRole("button", { name: t.regenerateLabel })).toHaveAttribute("aria-busy", "true");
    // Shown again later (another photo, the panel closed): still busy.
    unmount();
    show(detail());
    expect(screen.getByRole("button", { name: t.regenerateLabel })).toHaveTextContent(t.regenerating);
    await user.click(screen.getByRole("button", { name: t.regenerateLabel }));
    expect(fake.calls.filter((c) => c.command === "tagging_regenerate")).toHaveLength(1);
    await act(async () => finish(tags({ description: "Another look at the beach." })));
    expect(await screen.findByText("Another look at the beach.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t.regenerateLabel })).toHaveTextContent(t.regenerate);
  });

  it("raises a toast when Regenerate fails, and keeps the old sentence", async () => {
    const user = userEvent.setup();
    fake.handle("tagging_regenerate", () => {
      throw appError("tagging_timeout", "Florence-2 base took too long. Try again.");
    });
    show(detail());
    await user.click(screen.getByRole("button", { name: t.regenerateLabel }));
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((x) => x.message)).toEqual(["Florence-2 base took too long. Try again."]),
    );
    expect(screen.getByText("A dog running along a sandy beach.")).toBeInTheDocument();
  });

  it("drops the reply when another album was opened meanwhile", async () => {
    const user = userEvent.setup();
    let fail!: (e: unknown) => void;
    fake.handle("tagging_regenerate", () => new Promise<PhotoTags>((_, rej) => (fail = rej)));
    show(detail());
    await user.click(screen.getByRole("button", { name: t.regenerateLabel }));
    useAlbumStore.setState({ status: fakeStatus("D:\\Other") });
    await act(async () => fail(appError("tagging_stopped", "Tagging stopped.")));
    await waitFor(() => expect(useRegenerateStore.getState().busy).toEqual([]));
    expect(useToasts.getState().toasts).toEqual([]);
  });

  it("disables Regenerate in a view-only album, with the banner's first sentence", () => {
    show(detail({ can_regenerate: false }), "View only: this folder can't be written to.");
    const button = screen.getByRole("button", { name: t.regenerateLabel });
    expect(button).toHaveAttribute("aria-disabled", "true");
  });

  it("draws Regenerate with its icon as well as its word", () => {
    show(detail());
    const button = screen.getByRole("button", { name: t.regenerateLabel });
    expect(button.querySelector("svg")).not.toBeNull();
    expect(button).toHaveTextContent(t.regenerate);
  });

  it("asks for nothing by itself (no polling)", () => {
    vi.useFakeTimers();
    try {
      show(detail());
      vi.advanceTimersByTime(60_000);
      expect(fake.calls).toEqual([]);
    } finally {
      vi.useRealTimers();
    }
  });
});
