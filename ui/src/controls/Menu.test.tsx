import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MenuButton, placeWithin, type MenuEntry } from "./Menu";
import { layerRoot } from "./layers";
import { seriousFindings } from "../test/axe";

function entries(spy: (id: string) => void): MenuEntry[] {
  return [
    { id: "copy", label: "Copy", icon: "copy", shortcut: "Ctrl+C", onSelect: () => spy("copy") },
    { id: "reveal", label: "Show in folder", icon: "reveal", onSelect: () => spy("reveal") },
    { id: "star", label: "Star", checked: true, onSelect: () => spy("star") },
    "separator",
    { id: "paste", label: "Paste", disabledReason: "Nothing was copied.", onSelect: () => spy("paste") },
    { id: "trash", label: "Move to Recycle Bin", icon: "trash", danger: true, onSelect: () => spy("trash") },
  ];
}

describe("MenuButton", () => {
  it("opens a menu in the layer root and reports it on the button", async () => {
    render(<MenuButton label="More" icon="more" entries={entries(() => {})} />);
    const button = screen.getByRole("button", { name: "More" });
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(button);
    const menu = screen.getByRole("menu");
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(layerRoot().contains(menu)).toBe(true);
    expect(screen.getByRole("menuitem", { name: /Copy/ })).toHaveFocus();
  });

  it("shows rows with icons, shortcuts, a checked row, a rule and a danger row", async () => {
    render(<MenuButton label="More" icon="more" entries={entries(() => {})} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    expect(screen.getByRole("menuitem", { name: /Copy/ })).toHaveTextContent("Ctrl+C");
    expect(screen.getByRole("menuitemcheckbox", { name: "Star" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("separator")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Move to Recycle Bin" })).toHaveClass("ow-menu__row--danger");
  });

  it("moves with the arrow keys, Home and End, skipping no row, and wraps", async () => {
    render(<MenuButton label="More" icon="more" entries={entries(() => {})} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Show in folder" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "Move to Recycle Bin" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: /Copy/ })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Move to Recycle Bin" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: /Copy/ })).toHaveFocus();
  });

  it("runs a row on Enter, closes, and returns focus to the button", async () => {
    const spy = vi.fn();
    render(<MenuButton label="More" icon="more" entries={entries(spy)} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(spy).toHaveBeenCalledWith("reveal");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "More" })).toHaveFocus();
  });

  it("runs a row on click", async () => {
    const spy = vi.fn();
    render(<MenuButton label="More" icon="more" entries={entries(spy)} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Move to Recycle Bin" }));
    expect(spy).toHaveBeenCalledWith("trash");
  });

  it("does not run a disabled row, and says why", async () => {
    const spy = vi.fn();
    render(<MenuButton label="More" icon="more" entries={entries(spy)} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    const paste = screen.getByRole("menuitem", { name: "Paste" });
    expect(paste).toHaveAttribute("aria-disabled", "true");
    expect(paste).toHaveAccessibleDescription("Nothing was copied.");
    await userEvent.click(paste);
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("closes on Escape and on a click outside", async () => {
    render(<MenuButton label="More" icon="more" entries={entries(() => {})} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "More" })).toHaveFocus();

    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("has no serious accessibility findings", async () => {
    render(<MenuButton label="More" icon="more" entries={entries(() => {})} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    expect(await seriousFindings()).toEqual([]);
  });
});

describe("placeWithin", () => {
  const view = { width: 1000, height: 600 };
  const size = { width: 220, height: 200 };

  it("opens where the click was when it fits", () => {
    expect(placeWithin({ x: 100, y: 100 }, size, view)).toEqual({ x: 100, y: 100 });
  });

  it("moves left and up to stay inside the window", () => {
    expect(placeWithin({ x: 900, y: 500 }, size, view)).toEqual({ x: 780, y: 400 });
  });

  it("never goes past the top left corner", () => {
    expect(placeWithin({ x: -20, y: -5 }, { width: 2000, height: 900 }, view)).toEqual({ x: 0, y: 0 });
  });
});
