import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tooltip } from "./Tooltip";
import { tooltipDelayMs } from "./timing";
import { layerRoot } from "./layers";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

const user = () => userEvent.setup({ advanceTimers: vi.advanceTimersByTime });

function Subject() {
  return (
    <Tooltip text="Copy the full path">
      <button type="button">Path</button>
    </Tooltip>
  );
}

describe("Tooltip", () => {
  it("appears after the pointer rests for 500 ms, in the layer root", async () => {
    render(<Subject />);
    await user().hover(screen.getByRole("button", { name: "Path" }));
    act(() => vi.advanceTimersByTime(tooltipDelayMs - 1));
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => vi.advanceTimersByTime(1));
    const tip = screen.getByRole("tooltip");
    expect(tip).toHaveTextContent("Copy the full path");
    expect(layerRoot().contains(tip)).toBe(true);
    expect(screen.getByRole("button", { name: "Path" })).toHaveAccessibleDescription("Copy the full path");
  });

  it("appears at once on keyboard focus, and goes on Escape", async () => {
    render(<Subject />);
    await user().tab();
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    await user().keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("goes on any key press and on blur", async () => {
    render(<Subject />);
    await user().tab();
    await user().keyboard("a");
    expect(screen.queryByRole("tooltip")).toBeNull();
    await user().tab({ shift: true });
    await user().tab();
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    await user().tab();
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("does not appear when a mouse click gives focus", () => {
    render(<Subject />);
    const button = screen.getByRole("button", { name: "Path" });
    fireEvent.pointerDown(button);
    fireEvent.focus(button);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("goes when the pointer leaves before the delay", async () => {
    render(<Subject />);
    const button = screen.getByRole("button", { name: "Path" });
    await user().hover(button);
    await user().unhover(button);
    act(() => vi.advanceTimersByTime(tooltipDelayMs));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
