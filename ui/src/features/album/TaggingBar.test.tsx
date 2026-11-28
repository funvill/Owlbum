import { beforeEach, describe, expect, it } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { TaggingProgress } from "../../api/bindings/TaggingProgress";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { idleProgress } from "../../api/fake/tagging";
import { Toasts, useToasts } from "../../controls";
import { taggingBarText as t } from "../../text/album";
import { useSettingsOpen } from "../app/settingsOpen";
import { useAlbumStore } from "./session";
import { TaggingBar } from "./TaggingBar";
import { resetTagging } from "./tagging";

const p = (over: Partial<TaggingProgress>): TaggingProgress => ({ ...idleProgress, ...over });

async function show(start: TaggingProgress, open = true): Promise<FakeApi> {
  const fake = createFakeApi({ tagging_status: () => start });
  setApi(fake);
  useAlbumStore.setState({ status: open ? fakeStatus("C:\\Photos") : null });
  render(
    <>
      <TaggingBar />
      <Toasts />
    </>,
  );
  await waitFor(() => expect(fake.calls.some((c) => c.command === "tagging_status")).toBe(true));
  await act(async () => {});
  return fake;
}

const commands = (fake: FakeApi) => fake.calls.map((c) => c.command).filter((c) => c !== "tagging_status");

beforeEach(() => {
  resetTagging();
  useToasts.setState({ toasts: [] });
  useSettingsOpen.setState({ open: false, section: null });
});

describe("the tagging bar", () => {
  it("shows nothing when total is 0, or with no album open", async () => {
    await show(p({ total: 0, running: true }));
    expect(screen.queryByRole("progressbar")).toBeNull();
    resetTagging();
    document.body.innerHTML = "";
    await show(p({ total: 10, running: true }), false);
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("shows the bar, the count with grouping, the failures and Pause while running", async () => {
    const user = userEvent.setup();
    const fake = await show(p({ total: 2140, done: 1201, failed: 3, running: true }));
    const bar = screen.getByRole("progressbar", { name: t.label });
    expect(bar).toHaveAttribute("aria-valuenow", "1204");
    expect(bar).toHaveAttribute("aria-valuemax", "2140");
    expect(screen.getByText("Tagging 1,204 of 2,140 · 3 failed")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.pause }));
    expect(fake.calls.filter((c) => c.command === "tagging_pause").map((c) => c.args)).toEqual([{ paused: true }]);
    // The page does not guess: the bar changes with the next event.
    expect(screen.getByRole("button", { name: t.pause })).toBeInTheDocument();
    act(() => fake.emit("tagging_progress", p({ total: 2140, done: 1201, failed: 0, paused: true })));
    expect(screen.getByText("Tagging 1,201 of 2,140 · paused")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.resume }));
    expect(fake.calls.filter((c) => c.command === "tagging_pause").map((c) => c.args)).toEqual([
      { paused: true },
      { paused: false },
    ]);
  });

  it("says tagging stopped, with Try again", async () => {
    const user = userEvent.setup();
    const fake = await show(p({ total: 10, problem: "The database could not be written." }));
    expect(screen.getByText("Tagging stopped: The database could not be written.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.tryAgain }));
    expect(commands(fake)).toEqual(["tagging_restart"]);
  });

  it("says tagging is not working, with Settings and Try again", async () => {
    const user = userEvent.setup();
    const fake = await show(p({ total: 10, not_working: ["describe"] }));
    expect(screen.getByText(t.notWorking)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.settings }));
    expect(useSettingsOpen.getState()).toEqual({ open: true, section: "tagging" });
    await user.click(screen.getByRole("button", { name: t.tryAgain }));
    expect(commands(fake)).toEqual(["tagging_restart"]);
  });

  it("shows the runtime that would not start as not working", async () => {
    await show(
      p({
        total: 10,
        not_working: ["describe", "keywords"],
        problem: "The model runtime would not start: onnxruntime.dll was not found",
      }),
    );
    expect(screen.getByText(/The model runtime would not start/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t.tryAgain })).toBeInTheDocument();
  });

  it("says how many photos could not be tagged, retries them, and closes until the counts change", async () => {
    const user = userEvent.setup();
    const fake = await show(p({ total: 10, done: 9, failed: 1 }));
    expect(screen.getByText("1 photo could not be tagged.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.retryFailed }));
    expect(commands(fake)).toEqual(["tagging_retry_failed"]);
    await user.click(screen.getByRole("button", { name: t.hide }));
    expect(screen.queryByText("1 photo could not be tagged.")).toBeNull();
    act(() => fake.emit("tagging_progress", p({ total: 11, done: 9, failed: 2 })));
    expect(screen.getByText("2 photos could not be tagged.")).toBeInTheDocument();
  });

  it("shows nothing when finished with nothing failed", async () => {
    await show(p({ total: 10, done: 10 }));
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("raises a toast when a button's command fails", async () => {
    const user = userEvent.setup();
    const fake = await show(p({ total: 10, done: 1, running: true }));
    fake.handle("tagging_pause", () => {
      throw appError("view_only", "View only: this folder can't be written to.");
    });
    await user.click(screen.getByRole("button", { name: t.pause }));
    await waitFor(() => expect(useToasts.getState().toasts.map((x) => x.message)).toEqual(["View only: this folder can't be written to."]));
  });
});
