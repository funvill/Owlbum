import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { useToasts } from "../../controls/toastQueue";
import { handlerGaps, row as keyRow } from "../../keys";
import { useAlbumStore } from "../album";
import { Review } from "../album/Review";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { resetFlags } from "../organise";
import { resetImageCache, type Load } from "../preview";
import { openViewer } from "../viewer";
import { resetViewer } from "../viewer/store";
import { bin, resetFileNotes } from "./actions";
import { resetCapabilities } from "./capabilities";
import { resetDrag } from "./drag";
import { FILE_KEY_IDS, fileKeyHandlers } from "./keys";

class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
const isStage = (el: HTMLElement) => el.classList.contains("ow-zoom");

let fake: FakeApi;
const grid = () => useGridStore.getState();
const initialAlbum = useAlbumStore.getState();

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({ id: i + 1, rel_path: `day/p${i + 1}.jpg`, width: 4000, height: 2000 }));
}

const noLoad = (): Load => ({ done: new Promise<void>(() => {}), cancel: () => {} });

function setup(overrides: Parameters<typeof createFakeApi>[0] = {}) {
  const album = createQueryFake(photos(6));
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg` }),
    ...overrides,
  });
  setApi(fake);
}

/** Renders the review screen with photo `id` current and `also` selected too. */
async function show(id: number, also: number[] = []) {
  render(<Review busy={() => false} />);
  await waitFor(() => expect(screen.getAllByRole("gridcell").length).toBeGreaterThan(0));
  act(() =>
    grid().setSelection({ current: { id, index: id - 1 }, anchor: { id, index: id - 1 }, selection: [id, ...also] }),
  );
}

const press = (key: string, init: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(document.activeElement ?? document.body, { key, ...init });

const callsOf = (command: string) => fake.calls.filter((c) => c.command === command).map((c) => c.args);
const toasts = () => useToasts.getState().toasts.map((t) => t.message);
const tile = (index: number) => document.querySelector<HTMLElement>(`[role="gridcell"][data-index="${index}"]`)!;

beforeEach(() => {
  resetGrid();
  resetViewer();
  resetFlags();
  resetFileNotes();
  resetCapabilities();
  resetDrag();
  resetImageCache(noLoad);
  useToasts.setState({ toasts: [] });
  useAlbumStore.setState(initialAlbum, true);
  useSettingsStore.setState({ settings: null, error: null });
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isStage(this) ? 800 : 1000;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return isStage(this) ? 600 : 800;
    },
  });
  setup();
});

afterEach(() => {
  resetGrid();
  resetViewer();
  resetDrag();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("keys", () => {
  it("has a handler for each of its rows", () => {
    const rows = FILE_KEY_IDS.map((id) => keyRow(id));
    expect(handlerGaps(fileKeyHandlers(), rows)).toEqual({ missing: [], unknown: [] });
  });
});

describe("the Delete key", () => {
  it("sends the selected photos in the order of the result", async () => {
    await show(4, [2]);
    press("Delete");
    await waitFor(() => expect(callsOf("file_trash")).toEqual([{ photo_ids: [2, 4] }]));
  });

  it("does nothing in a text field, an auto-repeat, the viewer or a view-only album", async () => {
    await show(2);
    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    fireEvent.keyDown(field, { key: "Delete" });
    field.remove();
    press("Delete", { repeat: true });
    act(() => openViewer({ id: 2, index: 1 }));
    press("Delete");
    act(() => resetViewer());
    useAlbumStore.setState({ status: fakeStatus("D:/Photos", { view_only: "read_only" }) });
    press("Delete");
    await new Promise((r) => setTimeout(r, 0));
    expect(callsOf("file_trash")).toEqual([]);
  });

  it("reports files that could not be moved", async () => {
    setup({
      file_trash: ({ photo_ids }) => ({
        trashed: [],
        failed: [{ id: photo_ids[0]!, name: "p3.jpg", message: "in use" }],
        cancelled: false,
        revision: 1,
      }),
    });
    await show(3);
    press("Delete");
    await waitFor(() => expect(toasts()).toContain(`Couldn't move p3.jpg to the ${bin()}: in use`));
  });
});

describe("Ctrl+C", () => {
  it("copies the picture of one photo", async () => {
    setup({ file_copy_image: () => ({ scaled: true, file: false }) });
    await show(2);
    press("c", { ctrlKey: true });
    await waitFor(() => expect(callsOf("file_copy_image")).toEqual([{ photo_id: 2 }]));
    expect(toasts()).toContain("Copied, made smaller to fit the clipboard");
  });

  it("copies the files of several, saying when edits are left out", async () => {
    setup({ file_copy_files: () => ({ copied: 500, dropped: 100, as: "files", with_edits: true }) });
    await show(1, [3]);
    press("c", { ctrlKey: true });
    await waitFor(() => expect(callsOf("file_copy_files")).toEqual([{ photo_ids: [1, 3] }]));
    expect(toasts()).toContain("Copied the first 500 photos (without their edits)");
    expect(toasts().some((t) => t.startsWith("Photos are dragged as they are on disk"))).toBe(true);
  });

  it("says when paths were copied instead of files", async () => {
    setup({ file_copy_files: () => ({ copied: 2, dropped: 0, as: "paths", with_edits: false }) });
    await show(1, [3]);
    press("c", { ctrlKey: true });
    await waitFor(() =>
      expect(toasts()).toContain("Copied 2 paths. Copying files is not supported on Linux yet."),
    );
  });

  it("copies the shown photo in the viewer", async () => {
    await show(2, [3]);
    act(() => openViewer({ id: 2, index: 1 }));
    press("c", { ctrlKey: true });
    await waitFor(() => expect(callsOf("file_copy_image")).toEqual([{ photo_id: 2 }]));
  });

  it("shows Rust's message when copying fails", async () => {
    setup({
      file_copy_image: () => {
        throw appError("clipboard_failed", "Couldn't copy the photo: busy");
      },
    });
    await show(2);
    press("c", { ctrlKey: true });
    await waitFor(() => expect(toasts()).toContain("Couldn't copy the photo: busy"));
  });
});

describe("the photo menu", () => {
  it("opens on a right-click with the file items, labelled for the selection", async () => {
    await show(1, [2, 3]);
    fireEvent.contextMenu(tile(1), { clientX: 10, clientY: 10 });
    const menu = await screen.findByRole("menu", { name: "Photo" });
    const labels = within(menu)
      .getAllByRole("menuitem")
      .map((i) => i.querySelector(".ow-menu__label")?.textContent);
    expect(labels).toEqual([
      "Edit",
      "Rotate left",
      "Rotate right",
      "Flip horizontally",
      "Copy 3 photos", "Copy 3 paths", "Show in folder", "Star", "Hide", `Move to ${bin()}`]);
  });

  it("copies paths, shows in folder and moves to the bin", async () => {
    await show(1, [2]);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Copy 2 paths/ }));
    await waitFor(() => expect(callsOf("file_copy_paths")).toEqual([{ photo_ids: [1, 2] }]));
    expect(toasts()).toContain("Copied 2 paths");
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Show in folder/ }));
    await waitFor(() => expect(callsOf("file_reveal")).toEqual([{ photo_id: 1 }]));

    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Move to/ }));
    await waitFor(() => expect(callsOf("file_trash")).toEqual([{ photo_ids: [1, 2] }]));
  });

  it("disables what changes the album in a view-only album, with the banner's reason", async () => {
    await show(1);
    useAlbumStore.setState({ status: fakeStatus("D:/Photos", { view_only: "read_only" }) });
    fireEvent.contextMenu(tile(1), { clientX: 10, clientY: 10 });
    const trash = await screen.findByRole("menuitem", { name: /Move to/ });
    expect(trash).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("menuitem", { name: /Rotate left/ })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("menuitem", { name: /Copy path/ })).not.toHaveAttribute("aria-disabled");
  });

  it("rotates and flips every target as one edit_turn", async () => {
    await show(1, [2]);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Rotate right/ }));
    await waitFor(() => expect(callsOf("edit_turn")).toEqual([{ photo_ids: [1, 2], op: "rotate_right" }]));
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Flip horizontally/ }));
    await waitFor(() => expect(callsOf("edit_turn")).toHaveLength(2));
    expect(callsOf("edit_turn")[1]).toEqual({ photo_ids: [1, 2], op: "flip" });
  });

  it("closes on a review key, which then acts; its own keys keep it open", async () => {
    await show(1, [2]);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    await screen.findByRole("menu");
    press("ArrowDown");
    expect(screen.getByRole("menu")).toBeInTheDocument();
    press("Delete");
    expect(screen.queryByRole("menu")).toBeNull();
    await waitFor(() => expect(callsOf("file_trash")).toEqual([{ photo_ids: [1, 2] }]));
  });

  it("opens in the viewer on a right-click; Esc closes the menu and not the viewer", async () => {
    await show(2);
    act(() => openViewer({ id: 2, index: 1 }));
    const viewer = await screen.findByRole("dialog");
    fireEvent.contextMenu(viewer.querySelector<HTMLElement>(".ow-zoom")!, { clientX: 10, clientY: 10 });
    await screen.findByRole("menu", { name: "Photo" });
    press("Escape");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // A viewer key closes the menu and then acts: Ctrl+C copies the shown photo.
    fireEvent.contextMenu(viewer.querySelector<HTMLElement>(".ow-zoom")!, { clientX: 10, clientY: 10 });
    await screen.findByRole("menu");
    press("c", { ctrlKey: true });
    expect(screen.queryByRole("menu")).toBeNull();
    await waitFor(() => expect(callsOf("file_copy_image")).toEqual([{ photo_id: 2 }]));
  });

  it("closes when the window loses focus", async () => {
    await show(1);
    fireEvent.contextMenu(tile(1), { clientX: 10, clientY: 10 });
    await screen.findByRole("menu");
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("dragging out", () => {
  it("prepares on pointer-down and starts after a 5 px move, not a 3 px one", async () => {
    await show(2);
    // The review screen asks once what the platform offers.
    await waitFor(() => expect(callsOf("app_capabilities").length).toBe(1));
    fireEvent.pointerDown(tile(1), { button: 0, clientX: 100, clientY: 100 });
    await waitFor(() => expect(callsOf("file_prepare_drag")).toEqual([{ photo_ids: [2] }]));
    fireEvent.pointerMove(window, { buttons: 1, clientX: 103, clientY: 100 });
    expect(callsOf("file_start_drag")).toEqual([]);
    fireEvent.pointerMove(window, { buttons: 1, clientX: 105, clientY: 100 });
    await waitFor(() => expect(callsOf("file_start_drag")).toEqual([{ photo_ids: [2] }]));
  });

  it("starts from the preview at fit", async () => {
    await show(2);
    await waitFor(() => expect(callsOf("app_capabilities").length).toBe(1));
    const stage = document.querySelector<HTMLElement>(".ow-preview .ow-zoom")!;
    fireEvent.pointerDown(stage, { button: 0, clientX: 100, clientY: 100 });
    await waitFor(() => expect(callsOf("file_prepare_drag")).toEqual([{ photo_ids: [2] }]));
    fireEvent.pointerMove(window, { buttons: 1, clientX: 110, clientY: 100 });
    await waitFor(() => expect(callsOf("file_start_drag")).toEqual([{ photo_ids: [2] }]));
  });

  it("makes no drag call where dragging out is not offered", async () => {
    setup({ app_capabilities: () => ({ drag_out: false, copy_files: "paths" }) });
    await show(2);
    await waitFor(() => expect(callsOf("app_capabilities").length).toBe(1));
    fireEvent.pointerDown(tile(1), { button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(window, { buttons: 1, clientX: 120, clientY: 100 });
    await new Promise((r) => setTimeout(r, 0));
    expect(callsOf("file_prepare_drag")).toEqual([]);
    expect(callsOf("file_start_drag")).toEqual([]);
  });

  it("shows the message when the drag fails", async () => {
    setup({
      file_start_drag: () => {
        throw appError("drag_failed", "Couldn't drag the photos: refused");
      },
    });
    await show(2);
    await waitFor(() => expect(callsOf("app_capabilities").length).toBe(1));
    fireEvent.pointerDown(tile(1), { button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(window, { buttons: 1, clientX: 110, clientY: 100 });
    await waitFor(() => expect(toasts()).toContain("Couldn't drag the photos: refused"));
  });
});
