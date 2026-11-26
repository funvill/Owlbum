import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, IconButton } from "./Button";
import { tooltipDelayMs } from "./timing";
import { seriousFindings } from "../test/axe";

describe("Button", () => {
  it("is a default button unless told otherwise", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Keep</Button>);
    const button = screen.getByRole("button", { name: "Keep" });
    expect(button).toHaveClass("ow-button", "ow-button--default");
    expect(button).toHaveAttribute("type", "button");
    await userEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("draws the primary and small kinds", () => {
    render(
      <>
        <Button kind="primary">Open folder</Button>
        <Button size="sm">Undo</Button>
      </>,
    );
    expect(screen.getByRole("button", { name: "Open folder" })).toHaveClass("ow-button--primary");
    expect(screen.getByRole("button", { name: "Undo" })).toHaveClass("ow-button--sm");
  });

  it("marks a button that deletes, and never makes it primary", () => {
    render(
      <Button kind="primary" danger>
        Move to Recycle Bin
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Move to Recycle Bin" });
    expect(button).toHaveClass("ow-button--default", "ow-button--danger");
    expect(button).not.toHaveClass("ow-button--primary");
  });

  it("shows its icon before the label", () => {
    render(<Button icon="folder">Open folder</Button>);
    expect(screen.getByRole("button", { name: "Open folder" }).querySelector("svg")).not.toBeNull();
  });

  it("ignores clicks while busy, keeps its label and shows a spinner", async () => {
    const onClick = vi.fn();
    render(
      <Button icon="folder" busy onClick={onClick}>
        Open folder
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Open folder" });
    expect(button).toHaveAttribute("aria-busy", "true");
    expect(button.querySelector(".ow-spinner")).not.toBeNull();
    expect(button.querySelector("svg:not(.ow-spinner)")).toBeNull();
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("stays focusable when disabled, ignores clicks and says why in a tooltip", async () => {
    const onClick = vi.fn();
    render(
      <Button disabledReason="The album is view-only." onClick={onClick}>
        Save
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    await userEvent.tab();
    expect(button).toHaveFocus();
    expect(screen.getByRole("tooltip")).toHaveTextContent("The album is view-only.");
    expect(button).toHaveAccessibleDescription("The album is view-only.");
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
});

describe("IconButton", () => {
  it("is named by its label, hides its icon and shows the label with the shortcut as a tooltip", async () => {
    vi.useFakeTimers();
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      render(<IconButton icon="eye-off" label="Hide" shortcut="H" />);
      const button = screen.getByRole("button", { name: "Hide" });
      expect(button).toHaveAttribute("aria-keyshortcuts", "H");
      expect(button.querySelector("svg")).toHaveAttribute("aria-hidden", "true");

      await user.hover(button);
      expect(screen.queryByRole("tooltip")).toBeNull();
      act(() => vi.advanceTimersByTime(tooltipDelayMs));
      expect(screen.getByRole("tooltip")).toHaveTextContent("Hide (H)");

      await user.unhover(button);
      expect(screen.queryByRole("tooltip")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("reports a toggle's state with aria-pressed", async () => {
    const onClick = vi.fn();
    render(<IconButton icon="details" label="Details panel" pressed onClick={onClick} />);
    const button = screen.getByRole("button", { name: "Details panel" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveClass("ow-icon-button--on");
    await userEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("has no serious accessibility findings", async () => {
    render(
      <>
        <Button kind="primary">Open folder</Button>
        <IconButton icon="settings" label="Settings" />
        <IconButton icon="star" label="Star" pressed={false} disabledReason="Nothing is selected." />
      </>,
    );
    expect(await seriousFindings()).toEqual([]);
  });
});
