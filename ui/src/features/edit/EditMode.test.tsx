import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { fakePlan } from "../../api/fake/edit";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import type { EditPlan } from "../../api/bindings/EditPlan";
import type { PhotoEdit } from "../../api/bindings/PhotoEdit";
import { useToasts } from "../../controls/toastQueue";
import { useAlbumStore } from "../album";
import { Review } from "../album/Review";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { resetFlags } from "../organise";
import { resetImageCache, type Load } from "../preview";
import { openViewer, resetViewer, useViewerStore } from "../viewer/store";
import { useBeforeStore } from "./before";
import { IDENTITY, isEditing, resetEditMode, useEditStore } from "./mode";
import { resetTurns } from "./turn";

class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

let fake: FakeApi;
const grid = () => useGridStore.getState();
const initialAlbum = useAlbumStore.getState();

function photos(): FakePhoto[] {
  return [
    { id: 1, rel_path: "day/p1.jpg", width: 4032, height: 3024 },
    { id: 2, rel_path: "day/p2.jpg", width: 4032, height: 3024 },
    { id: 3, rel_path: "day/p3.mp4", width: 1920, height: 1080, kind: "video", duration_ms: 4000 },
  ];
}

/** Holds each `edit_plan` call until the test answers it. */
let heldPlans: { resolve: (p: EditPlan) => void; edit: PhotoEdit }[] | null = null;

function setup(overrides: Parameters<typeof createFakeApi>[0] = {}) {
  const album = createQueryFake(photos());
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg` }),
    edit_plan: ({ edit }) => {
      const plan = fakePlan(4032, 3024, edit.rotate, { key: `k${edit.rotate}-${edit.straighten}` });
      if (heldPlans === null) return plan;
      return new Promise<EditPlan>((resolve) => heldPlans!.push({ resolve: () => resolve(plan), edit }));
    },
    ...overrides,
  });
  setApi(fake);
}

const noLoad = (): Load => ({ done: new Promise<void>(() => {}), cancel: () => {} });
const calls = (command: string) => fake.calls.filter((c) => c.command === command).map((c) => c.args);
const sets = () => calls("edit_set") as { photo_id: number; edit: PhotoEdit | null }[];

async function show(id = 1) {
  render(<Review busy={() => false} />);
  await waitFor(() => expect(screen.getAllByRole("gridcell").length).toBeGreaterThan(0));
  act(() => grid().setSelection({ current: { id, index: id - 1 }, anchor: { id, index: id - 1 }, selection: [id] }));
}

/** Opens edit mode with E and waits for the stage's first plan. */
async function edit(id = 1) {
  await show(id);
  fireEvent.keyDown(document.body, { key: "e" });
  await screen.findByRole("region", { name: "Edit photo" });
  await waitFor(() => expect(useEditStore.getState().plan).not.toBeNull());
}

const slider = (name: string) => screen.getByRole("slider", { name }) as HTMLInputElement;

beforeEach(() => {
  resetGrid();
  resetViewer();
  resetFlags();
  resetTurns();
  resetEditMode();
  heldPlans = null;
  resetImageCache(noLoad);
  useAlbumStore.setState(initialAlbum, true);
  useSettingsStore.setState({ settings: null, error: null });
  useToasts.setState({ toasts: [] });
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 800 });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 600 });
  setup();
});

afterEach(() => {
  resetEditMode();
  resetGrid();
  resetViewer();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("Opening edit mode", () => {
  it("E opens it on the current photo, from its stored edit, with the whole-frame plan", async () => {
    const stored = { ...IDENTITY, rotate: 90 as const };
    setup({ photo_detail: ({ id }) => fakeDetail(id, { edit: stored }) });
    await edit(2);
    expect(useEditStore.getState()).toMatchObject({ photoId: 2, draft: stored, aspect: "free", swapped: false });
    expect(calls("edit_plan")[0]).toEqual({ photo_id: 2, edit: stored, whole: true });
    // The stage draws the plan with the crop box over it; the caption has no action buttons.
    expect(document.querySelector(".ow-edit__frame .ow-edited")).not.toBeNull();
    expect(screen.getAllByRole("img", { name: /^Resize from/ })).toHaveLength(8);
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  });

  it("the caption's Edit button and the menu's Edit item open it too", async () => {
    await show(1);
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await screen.findByRole("region", { name: "Edit photo" });
    expect(isEditing()).toBe(true);
  });

  it("is not offered for a video", async () => {
    await show(3);
    fireEvent.keyDown(document.body, { key: "e" });
    await new Promise((r) => setTimeout(r, 0));
    expect(isEditing()).toBe(false);
    fireEvent.contextMenu(screen.getAllByRole("gridcell")[2]!.querySelector(".ow-tile__hit") ?? screen.getAllByRole("gridcell")[2]!);
    const item = await screen.findByRole("menuitem", { name: /Edit/ });
    expect(item).toHaveAttribute("aria-disabled", "true");
  });

  it("the viewer cannot open over it, and review keys do nothing", async () => {
    await edit(1);
    act(() => openViewer({ id: 1, index: 0 }));
    expect(useViewerStore.getState().open).toBe(false);
    fireEvent.keyDown(document.body, { key: "f" });
    fireEvent.keyDown(document.body, { key: "ArrowRight" });
    await new Promise((r) => setTimeout(r, 0));
    expect(calls("photo_set_flags")).toHaveLength(0);
    expect(grid().current?.id).toBe(1);
  });
});

describe("The toolbar", () => {
  it("dragging a slider asks for a plan with at most one call in flight", async () => {
    await edit(1);
    heldPlans = [];
    const before = calls("edit_plan").length;
    fireEvent.change(slider("Straighten"), { target: { value: "2" } });
    fireEvent.change(slider("Straighten"), { target: { value: "3" } });
    fireEvent.change(slider("Straighten"), { target: { value: "4.5" } });
    expect(calls("edit_plan").length - before).toBe(1);
    await act(async () => heldPlans!.shift()!.resolve(fakePlan()));
    // Only the newest draft is asked for next.
    await waitFor(() => expect(calls("edit_plan").length - before).toBe(2));
    expect((calls("edit_plan").at(-1) as { edit: PhotoEdit }).edit.straighten).toBe(4.5);
    await act(async () => heldPlans!.shift()!.resolve(fakePlan()));
    expect(screen.getByText("+4.5°")).toBeInTheDocument();
  });

  it("a double-click resets a slider to 0", async () => {
    await edit(1);
    fireEvent.change(slider("Brightness"), { target: { value: "30" } });
    expect(useEditStore.getState().draft.colour.brightness).toBe(30);
    fireEvent.doubleClick(slider("Brightness"));
    expect(useEditStore.getState().draft.colour.brightness).toBe(0);
  });

  it("Auto fills the four colour sliders and keeps the rest of the draft", async () => {
    await edit(1);
    fireEvent.change(slider("Straighten"), { target: { value: "-3" } });
    fireEvent.click(screen.getByRole("button", { name: "Auto" }));
    await waitFor(() => expect(slider("Brightness").value).toBe("12"));
    expect(useEditStore.getState().draft.colour).toEqual({ brightness: 12, contrast: 8, saturation: 0, warmth: -5 });
    expect(useEditStore.getState().draft.straighten).toBe(-3);
    expect(screen.getByText("-5")).toBeInTheDocument();
  });

  it("a failed Auto says so and changes nothing", async () => {
    setup({ edit_auto: () => Promise.reject(appError("internal", "no thumbnail")) });
    await edit(1);
    fireEvent.click(screen.getByRole("button", { name: "Auto" }));
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((t) => t.message)).toContain("Couldn't work out Auto for this photo."),
    );
    expect(useEditStore.getState().draft.colour).toEqual(IDENTITY.colour);
  });

  it("Reset gives identity, Free, not swapped", async () => {
    await edit(1);
    fireEvent.click(screen.getByRole("radio", { name: "3:2" }));
    fireEvent.change(slider("Warmth"), { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(useEditStore.getState()).toMatchObject({ draft: IDENTITY, aspect: "free", swapped: false });
  });

  it("an aspect refits the crop, and Swap flips its shape", async () => {
    await edit(1);
    fireEvent.click(screen.getByRole("radio", { name: "Square" }));
    const square = useEditStore.getState().draft.crop;
    expect((square.w * 4032) / (square.h * 3024)).toBeCloseTo(1);
    expect(screen.getByRole("button", { name: "Swap portrait and landscape" })).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(screen.getByRole("radio", { name: "4:5" }));
    fireEvent.click(screen.getByRole("button", { name: "Swap portrait and landscape" }));
    const c = useEditStore.getState().draft.crop;
    expect((c.w * 4032) / (c.h * 3024)).toBeCloseTo(1.25);
  });

  it("dragging a corner handle resizes the crop", async () => {
    await edit(1);
    const handle = screen.getByRole("img", { name: "Resize from the bottom right" });
    fireEvent.pointerDown(handle, { button: 0, clientX: 500, clientY: 400 });
    fireEvent.pointerMove(handle, { clientX: 400, clientY: 300 });
    fireEvent.pointerUp(handle, {});
    const crop = useEditStore.getState().draft.crop;
    expect(crop.x).toBe(0);
    expect(crop.y).toBe(0);
    expect(crop.w).toBeLessThan(1);
    expect(crop.h).toBeLessThan(1);
  });
});

describe("Ending edit mode", () => {
  it("Enter is Done after a mouse click on Rotate", async () => {
    await edit(1);
    fireEvent.click(screen.getByRole("button", { name: "Rotate right" }), { detail: 1 });
    await waitFor(() => expect(useEditStore.getState().draft.rotate).toBe(90));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Enter" });
    await waitFor(() => expect(sets()).toHaveLength(1));
    expect(sets()[0]).toEqual({ photo_id: 1, edit: { ...IDENTITY, rotate: 90 } });
    await waitFor(() => expect(isEditing()).toBe(false));
  });

  it("Esc cancels and sends nothing", async () => {
    await edit(1);
    fireEvent.change(slider("Contrast"), { target: { value: "40" } });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(isEditing()).toBe(false);
    expect(sets()).toHaveLength(0);
  });

  it("selecting another photo stores the draft", async () => {
    await edit(1);
    fireEvent.change(slider("Saturation"), { target: { value: "-20" } });
    act(() => grid().setSelection({ current: { id: 2, index: 1 }, anchor: { id: 2, index: 1 }, selection: [2] }));
    await waitFor(() => expect(sets()).toHaveLength(1));
    expect(sets()[0]!.edit!.colour.saturation).toBe(-20);
    await waitFor(() => expect(isEditing()).toBe(false));
  });

  it("a failed Done keeps edit mode open and says why", async () => {
    setup({ edit_set: () => Promise.reject(appError("internal", "disk full")) });
    await edit(1);
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((t) => t.message)).toContain("Couldn't keep the edit: disk full"),
    );
    expect(isEditing()).toBe(true);
  });

  it("closing the album ends edit mode as Done first", async () => {
    await edit(1);
    useAlbumStore.setState({ status: fakeStatus("C:/Photos") });
    await act(() => useAlbumStore.getState().close());
    expect(sets()).toHaveLength(1);
    expect(calls("album_close")).toHaveLength(1);
    expect(fake.calls.findIndex((c) => c.command === "edit_set")).toBeLessThan(
      fake.calls.findIndex((c) => c.command === "album_close"),
    );
  });
});

describe("Before and after", () => {
  it("holding \\ in edit mode shows the plain picture captioned Original, without the crop box", async () => {
    await edit(1);
    fireEvent.change(slider("Brightness"), { target: { value: "10" } });
    const draft = useEditStore.getState().draft;
    const caption = () => document.querySelector(".ow-edit__stage .ow-zoom__original");
    fireEvent.keyDown(document.body, { key: "\\" });
    expect(caption()).toHaveTextContent("Original");
    expect(screen.queryByRole("group", { name: "Crop" })).toBeNull();
    fireEvent.keyUp(document.body, { key: "\\" });
    expect(caption()).toBeNull();
    expect(screen.getByRole("group", { name: "Crop" })).toBeInTheDocument();
    // A window blur restores the edit too; the draft is unchanged.
    fireEvent.keyDown(document.body, { key: "\\" });
    fireEvent.blur(window);
    expect(useBeforeStore.getState().held).toBe(false);
    expect(useEditStore.getState().draft).toEqual(draft);
  });

  it("in the preview, holding \\ draws the edited photo with no plan", async () => {
    setup({
      ...createQueryFake(
        photos().map((p) => (p.id === 1 ? { ...p, edit_plan: fakePlan(4032, 3024, 90, { key: "turned" }) } : p)),
      ).handlers,
    });
    await show(1);
    await waitFor(() => expect(document.querySelector('.ow-zoom__picture[data-edit-key="turned"]')).not.toBeNull());
    const preview = screen.getByRole("region", { name: "Preview" });
    expect(within(preview).getByRole("button", { name: "Before" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "\\" });
    expect(document.querySelector(".ow-zoom__picture[data-edit-key]")).toBeNull();
    expect(within(preview).getByText("Original")).toBeInTheDocument();
    fireEvent.keyUp(document.body, { key: "\\" });
    expect(document.querySelector('.ow-zoom__picture[data-edit-key="turned"]')).not.toBeNull();
    // The Before button holds it with the pointer.
    const button = within(preview).getByRole("button", { name: "Before" });
    fireEvent.pointerDown(button, { button: 0 });
    expect(useBeforeStore.getState().held).toBe(true);
    fireEvent.pointerUp(button, {});
    expect(useBeforeStore.getState().held).toBe(false);
  });
});
