import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toasts } from "./Toasts";
import { showToast, useToasts } from "./toastQueue";
import { toastMs } from "./timing";
import { layerRoot } from "./layers";
import { seriousFindings } from "../test/axe";

beforeEach(() => {
  vi.useFakeTimers();
  useToasts.setState({ toasts: [] });
});

afterEach(() => {
  vi.useRealTimers();
});

const user = () => userEvent.setup({ advanceTimers: vi.advanceTimersByTime });

describe("Toasts", () => {
  it("announces info and success politely and errors at once, in the layer root", () => {
    render(<Toasts />);
    act(() => {
      showToast({ kind: "info", message: "Copied 3 photos." });
      showToast({ kind: "error", message: "The file could not be moved." });
    });
    const status = screen.getByRole("status");
    const alert = screen.getByRole("alert");
    expect(within(status).getByText("Copied 3 photos.")).toBeInTheDocument();
    expect(within(alert).getByText("The file could not be moved.")).toBeInTheDocument();
    expect(layerRoot().contains(status)).toBe(true);
  });

  it("removes info and success after 4 seconds, and keeps errors until dismissed", () => {
    render(<Toasts />);
    act(() => {
      showToast({ kind: "success", message: "Saved." });
      showToast({ kind: "error", message: "Failed." });
    });
    act(() => vi.advanceTimersByTime(toastMs.plain - 1));
    expect(screen.getByText("Saved.")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByText("Saved.")).toBeNull();
    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.getByText("Failed.")).toBeInTheDocument();
  });

  it("keeps a toast with an action for 8 seconds, and runs the action", async () => {
    const undo = vi.fn();
    render(<Toasts />);
    act(() => showToast({ kind: "info", message: "Hid 2 photos.", action: { label: "Undo", onAction: undo } }));
    act(() => vi.advanceTimersByTime(toastMs.plain));
    expect(screen.getByText("Hid 2 photos.")).toBeInTheDocument();
    await user().click(screen.getByRole("button", { name: "Undo" }));
    expect(undo).toHaveBeenCalledOnce();
    expect(screen.queryByText("Hid 2 photos.")).toBeNull();

    act(() => showToast({ kind: "info", message: "Again.", action: { label: "Undo", onAction: undo } }));
    act(() => vi.advanceTimersByTime(toastMs.withAction));
    expect(screen.queryByText("Again.")).toBeNull();
  });

  it("dismisses from its x", async () => {
    render(<Toasts />);
    act(() => showToast({ kind: "error", message: "Failed." }));
    await user().click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("Failed.")).toBeNull();
  });

  it("stops its timer while hovered", async () => {
    render(<Toasts />);
    act(() => showToast({ kind: "info", message: "Copied." }));
    const toast = screen.getByText("Copied.").closest(".ow-toast")!;
    const pointer = user();
    await pointer.hover(toast);
    act(() => vi.advanceTimersByTime(toastMs.plain * 3));
    expect(screen.getByText("Copied.")).toBeInTheDocument();
    await pointer.unhover(toast);
    act(() => vi.advanceTimersByTime(toastMs.plain));
    expect(screen.queryByText("Copied.")).toBeNull();
  });

  it("shows at most three, newest at the bottom", () => {
    render(<Toasts />);
    act(() => {
      for (const n of [1, 2, 3, 4]) showToast({ kind: "error", message: `Toast ${n}` });
    });
    const shown = [...document.querySelectorAll(".ow-toast")].map((t) => t.querySelector(".ow-toast__message")?.textContent);
    expect(shown).toEqual(["Toast 2", "Toast 3", "Toast 4"]);
  });

  it("never takes focus", () => {
    render(<Toasts />);
    const before = document.activeElement;
    act(() => showToast({ kind: "error", message: "Failed." }));
    expect(document.activeElement).toBe(before);
  });

  it("has no serious accessibility findings", async () => {
    vi.useRealTimers();
    render(<Toasts />);
    act(() => showToast({ kind: "info", message: "Copied.", action: { label: "Undo", onAction: () => {} } }));
    expect(await seriousFindings(layerRoot())).toEqual([]);
  });
});
