import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { defaultSettings } from "../../api/fake/app";
import { fakeStatus } from "../../api/fake/album";
import { fakePlan } from "../../api/fake/edit";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { useToasts } from "../../controls/toastQueue";
import { useAlbumStore } from "../album";
import { resetAlbumSession } from "../album/session";
import { Review } from "../album/Review";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { resetImageCache, type Load } from "../preview";
import { resetViewer } from "../viewer/store";
import { SaveAllButton } from "./SaveAllButton";
import { resetSaves, savedToast, useSaveStore } from "./save";

class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

let fake: FakeApi;
const grid = () => useGridStore.getState();
const initialAlbum = useAlbumStore.getState();
const noLoad = (): Load => ({
  done: new Promise<void>(() => {}),
  cancel: () => {},
});

/** Photo 1 and 3 have an edit; photo 3 is a RAW file, which can only be saved as a copy. */
const photos: FakePhoto[] = [
  {
    id: 1,
    rel_path: "day/p1.jpg",
    width: 4000,
    height: 2000,
    edit_plan: fakePlan(4000, 2000),
  },
  { id: 2, rel_path: "day/p2.jpg", width: 4000, height: 2000 },
  {
    id: 3,
    rel_path: "day/p3.cr3",
    kind: "raw",
    width: 4000,
    height: 2000,
    edit_plan: fakePlan(4000, 2000),
  },
];

function setup(
  target: "album" | "file",
  overrides: Parameters<typeof createFakeApi>[0] = {},
) {
  const album = createQueryFake(photos);
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg` }),
    app_settings_get: () => ({ ...defaultSettings, edit_target: target }),
    edit_status: () => ({
      edited: 2,
      waiting: target === "file" ? 2 : 0,
      saving: 0,
    }),
    ...overrides,
  });
  setApi(fake);
}

async function show(id: number, also: number[] = []) {
  render(<Review busy={() => false} />);
  await waitFor(() =>
    expect(screen.getAllByRole("gridcell").length).toBeGreaterThan(0),
  );
  act(() =>
    grid().setSelection({
      current: { id, index: id - 1 },
      anchor: { id, index: id - 1 },
      selection: [id, ...also],
    }),
  );
}

const tile = (index: number) =>
  document.querySelector<HTMLElement>(
    `[role="gridcell"][data-index="${index}"]`,
  )!;
const callsOf = (command: string) =>
  fake.calls.filter((c) => c.command === command).map((c) => c.args);
const toasts = () => useToasts.getState().toasts.map((t) => t.message);

async function menuLabels() {
  const menu = await screen.findByRole("menu", { name: "Photo" });
  return within(menu)
    .getAllByRole("menuitem")
    .map((i) => i.querySelector(".ow-menu__label")?.textContent);
}

beforeEach(() => {
  resetGrid();
  resetViewer();
  resetSaves();
  resetImageCache(noLoad);
  useToasts.setState({ toasts: [] });
  useAlbumStore.setState(initialAlbum, true);
  useSettingsStore.setState({ settings: null, error: null });
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => 1000,
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => 800,
  });
});

afterEach(() => {
  resetGrid();
  resetViewer();
  resetAlbumSession();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("the save items of the photo menu", () => {
  it("in file mode: Save, Save as…, Save all (N) and Revert to original, after Flip", async () => {
    setup("file");
    await show(1);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    await waitFor(async () =>
      expect(await menuLabels()).toContain("Save all (2)"),
    );
    expect((await menuLabels()).slice(3, 8)).toEqual([
      "Flip horizontally",
      "Save",
      "Save as…",
      "Save all (2)",
      "Revert to original",
    ]);
  });

  it("in album mode: only Save as… and Revert to original", async () => {
    setup("album");
    await show(1);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    const labels = await menuLabels();
    expect(labels).toContain("Save as…");
    expect(labels).toContain("Revert to original");
    expect(labels).not.toContain("Save");
    expect(labels.some((l) => l?.startsWith("Save all"))).toBe(false);
  });

  it("has no save items for a photo without an edit", async () => {
    setup("file", {
      edit_status: () => ({ edited: 0, waiting: 0, saving: 0 }),
    });
    await show(2);
    fireEvent.contextMenu(tile(1), { clientX: 10, clientY: 10 });
    const labels = await menuLabels();
    expect(
      labels.filter((l) => l?.startsWith("Save") || l === "Revert to original"),
    ).toEqual([]);
  });

  it("saves the targets that have an edit and says how it went", async () => {
    setup("file");
    await show(1, [2]);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /^Save$/ }));
    await waitFor(() =>
      expect(callsOf("edit_save_to_file")).toEqual([{ photo_ids: [1] }]),
    );
    await waitFor(() => expect(toasts()).toContain("Saved 1 photo"));
  });

  it("disables Save for a photo that can only be saved as a copy", async () => {
    setup("file");
    await show(3);
    fireEvent.contextMenu(tile(2), { clientX: 10, clientY: 10 });
    const save = await screen.findByRole("menuitem", { name: /^Save$/ });
    expect(save).toHaveAttribute("aria-disabled", "true");
    expect(document.body.textContent).toContain(
      "Canon RAW photos can only be saved as a copy",
    );
    expect(
      screen.getByRole("menuitem", { name: /Save as/ }),
    ).not.toHaveAttribute("aria-disabled");
  });

  it("reverts every target with an edit as one call", async () => {
    setup("album");
    await show(1, [2, 3]);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(
      await screen.findByRole("menuitem", { name: /Revert to original/ }),
    );
    await waitFor(() =>
      expect(callsOf("edit_revert")).toEqual([{ photo_ids: [1, 3] }]),
    );
  });

  it("Save as… shows the copy's name, and nothing when cancelled", async () => {
    setup("album", {
      edit_save_as: () => ({
        cancelled: false,
        saved_name: "p1-edited.jpg",
        in_album: true,
        photo_id: 9,
        warning: null,
        revision: 3,
      }),
    });
    await show(1);
    fireEvent.contextMenu(tile(0), { clientX: 10, clientY: 10 });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Save as/ }));
    await waitFor(() =>
      expect(toasts()).toContain("Saved a copy as p1-edited.jpg"),
    );
    expect(callsOf("edit_save_as")).toEqual([{ photo_id: 1 }]);
  });
});

describe("the save toast", () => {
  it("counts, names up to three failures, and says what was skipped", () => {
    const failed = (name: string) => ({ id: 1, name, message: "no" });
    const said = (out: Parameters<typeof savedToast>[0]) => {
      useToasts.setState({ toasts: [] });
      savedToast(out);
      return toasts();
    };
    const none = {
      saved: [],
      failed: [],
      skipped: 0,
      warnings: [],
      revision: 1,
    };
    expect(said({ ...none, saved: [1, 2] })).toEqual(["Saved 2 photos"]);
    expect(said({ ...none, saved: [1], skipped: 2 })).toEqual([
      "Saved 1 photo. Skipped 2 that can only be saved as a copy",
    ]);
    expect(
      said({ ...none, saved: [1], failed: ["a", "b", "c", "d"].map(failed) }),
    ).toEqual(["Saved 1, 4 failed: a, b, c…"]);
    expect(said({ ...none, failed: [failed("a")] })).toEqual(["no"]);
    expect(
      said({
        ...none,
        saved: [1],
        warnings: ["A later change to x was not saved"],
      }),
    ).toEqual(["Saved 1 photo", "A later change to x was not saved"]);
  });
});

describe("Save all in the top bar", () => {
  it("shows in file mode with the count, and the progress while saving", () => {
    setup("file");
    useSettingsStore.setState({
      settings: { ...defaultSettings, edit_target: "file" },
    });
    useSaveStore.setState({ status: { edited: 3, waiting: 3, saving: 0 } });
    const { rerender } = render(<SaveAllButton />);
    fireEvent.click(screen.getByRole("button", { name: /Save all \(3\)/ }));
    expect(callsOf("edit_save_all")).toEqual([{}]);
    act(() =>
      useSaveStore.setState({
        running: true,
        progress: { done: 1, total: 3, current: "b.jpg" },
      }),
    );
    rerender(<SaveAllButton />);
    expect(
      screen.getByRole("button", { name: /Saving 2 of 3…/ }),
    ).toHaveAttribute("aria-disabled", "true");
  });

  it("is not shown in album mode", () => {
    setup("album");
    useSettingsStore.setState({
      settings: { ...defaultSettings, edit_target: "album" },
    });
    useSaveStore.setState({ status: { edited: 3, waiting: 0, saving: 0 } });
    render(<SaveAllButton />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("the close handshake", () => {
  it("answers app_closing with app_close_ready after saving the session", async () => {
    setup("album");
    await act(() => useAlbumStore.getState().start());
    act(() => fake.emit("app_closing", {}));
    await waitFor(() => expect(callsOf("app_close_ready")).toHaveLength(1));
  });

  it("shows Finishing saving… in the place of Save all while the close waits for a save", async () => {
    setup("file", {
      edit_status: () => ({ edited: 2, waiting: 2, saving: 1 }),
    });
    await act(() => useAlbumStore.getState().start());
    await waitFor(() => expect(useSaveStore.getState().status?.saving).toBe(1));
    render(<SaveAllButton />);
    act(() => fake.emit("app_closing", {}));
    expect(
      await screen.findByRole("button", { name: /Finishing saving…/ }),
    ).toHaveAttribute("aria-disabled", "true");
  });

  it("shows a recovery message from album_open as a toast", async () => {
    const message =
      "p1.jpg was being saved when Owlbum stopped. Its original is in the Recycle Bin; restore it to get the photo back.";
    setup("file", {
      album_open: ({ path }) => ({
        status: fakeStatus(path),
        session: null,
        notices: [message],
      }),
    });
    await act(() => useAlbumStore.getState().open("C:/photos"));
    await waitFor(() => expect(toasts()).toContain(message));
  });

  it("still answers when a step fails", async () => {
    setup("album", {
      app_session_save: () => Promise.reject(appError("internal", "disk")),
    });
    await act(() => useAlbumStore.getState().start());
    act(() => fake.emit("app_closing", {}));
    await waitFor(() => expect(callsOf("app_close_ready")).toHaveLength(1));
  });
});
