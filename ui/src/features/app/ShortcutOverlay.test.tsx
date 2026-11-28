import { beforeEach, describe, expect, it } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { KEYMAP, KEY_IDS } from "../../keys";
import { seriousFindings } from "../../test/axe";
import { keyLabels, shortcutsText as t } from "../../text/keys";
import { ShortcutOverlay } from "./ShortcutOverlay";
import { closeSettings, useSettingsOpen } from "./settingsOpen";
import { NOT_YET, closeShortcuts, sections, useShortcutKey, useShortcutsStore, type Where } from "./shortcuts";

function Harness({ here = "browse", view }: { here?: Where; view?: "grid" | "timeline" | "map" }) {
  useShortcutKey(() => here);
  return (
    <>
      <input aria-label="Search" />
      <ShortcutOverlay here={here} view={view} />
    </>
  );
}

beforeEach(() => closeShortcuts());

describe("the overlay's contents", () => {
  it("has a label for every row of the key table", () => {
    for (const id of KEY_IDS) expect(keyLabels[id], id).toBeTruthy();
  });

  it("puts the place the user is in first, Everywhere beside it, then the others", () => {
    expect(sections("viewer").map((s) => [s.place, s.here])).toEqual([
      ["viewer", true],
      ["everywhere", false],
      ["browse", false],
      ["edit", false],
    ]);
    expect(sections("browse").map((s) => s.place)).toEqual(["browse", "everywhere", "viewer", "edit"]);
  });

  it("lists every row but the development-only ones and those not built yet", () => {
    const listed = sections("browse").flatMap((s) => s.rows.map((r) => r.id));
    expect(listed).toHaveLength(KEYMAP.filter((r) => !r.dev && !NOT_YET.includes(r.id)).length);
    expect(listed).toContain("app.about");
    expect(listed).toContain("app.feedback");
    expect(listed).not.toContain("app.reload");
    expect(listed).not.toContain("app.developer_tools");
  });
});

describe("the overlay", () => {
  it("opens on ?, marks where you are, and closes on ? again", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("?");
    const dialog = screen.getByRole("dialog", { name: t.title });
    const first = within(dialog).getAllByRole("region")[0]!;
    expect(first).toHaveAccessibleName(t.places.browse);
    expect(within(first).getByText(t.whereYouAre)).toBeInTheDocument();
    expect(within(dialog).getByRole("region", { name: t.places.everywhere })).toBeInTheDocument();
    expect(within(first).getAllByText(keyLabels["browse.star"]).length).toBeGreaterThan(0);
    // Arrows are drawn as arrows; a hold key reads "hold \".
    expect(within(first).getAllByText("←").length).toBeGreaterThan(0);
    expect(within(first).getByText(t.hold)).toBeInTheDocument();
    expect(await seriousFindings(dialog)).toEqual([]);
    await user.keyboard("?");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens Settings from its Settings button", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("?");
    await user.click(screen.getByRole("button", { name: t.settings }));
    expect(useShortcutsStore.getState().open).toBe(false);
    expect(useSettingsOpen.getState().open).toBe(true);
    closeSettings();
  });

  it("closes on Esc", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("?");
    await user.keyboard("{Escape}");
    expect(useShortcutsStore.getState().open).toBe(false);
  });

  it("does not open while typing", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("textbox", { name: "Search" }));
    await user.keyboard("?");
    expect(useShortcutsStore.getState().open).toBe(false);
  });

  it("names the first list Timeline or Map in those views, and the viewer's list in the viewer", () => {
    act(() => useShortcutsStore.setState({ open: true }));
    const { rerender } = render(<Harness view="timeline" />);
    expect(screen.getAllByRole("region")[0]).toHaveAccessibleName(t.places.timeline);
    rerender(<Harness view="map" />);
    expect(screen.getAllByRole("region")[0]).toHaveAccessibleName(t.places.map);
    rerender(<Harness here="viewer" />);
    expect(screen.getAllByRole("region")[0]).toHaveAccessibleName(t.places.viewer);
  });
});
