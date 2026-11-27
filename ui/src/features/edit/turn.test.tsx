import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { fakePlan } from "../../api/fake/edit";
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
import { resetViewer } from "../viewer/store";
import { ROTATE_KEY_IDS, resetTurns, rotateKeyHandlers, turn } from "./turn";

class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

let fake: FakeApi;
let album: ReturnType<typeof createQueryFake>;
const grid = () => useGridStore.getState();
const initialAlbum = useAlbumStore.getState();

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({ id: i + 1, rel_path: `day/p${i + 1}.jpg`, width: 4000, height: 2000 }));
}

/** A fake album whose `edit_turn` turns each photo's plan and sends `album_changed` (reason `edit`). */
function setup(skipped = 0) {
  album = createQueryFake(photos(5));
  fake = createFakeApi({
    ...album.handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg` }),
    edit_turn: ({ photo_ids }) => {
      const revision = album.change((all) =>
        all.map((p) => (photo_ids.includes(p.id) ? { ...p, edit_plan: fakePlan(4000, 2000, 90) } : p)),
      );
      setTimeout(() => fake.emit("album_changed", { revision, reason: "edit", rekeyed: [] }), 0);
      return { changed: photo_ids, skipped, revision };
    },
  });
  setApi(fake);
}

const noLoad = (): Load => ({ done: new Promise<void>(() => {}), cancel: () => {} });

async function show(id: number, also: number[] = []) {
  render(<Review busy={() => false} />);
  await waitFor(() => expect(screen.getAllByRole("gridcell").length).toBeGreaterThan(0));
  act(() =>
    grid().setSelection({ current: { id, index: id - 1 }, anchor: { id, index: id - 1 }, selection: [id, ...also] }),
  );
}

const turns = () =>
  fake.calls.filter((c) => c.command === "edit_turn").map((c) => c.args as { photo_ids: number[]; op: string });

beforeEach(() => {
  resetGrid();
  resetViewer();
  resetFlags();
  resetTurns();
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
  resetGrid();
  resetViewer();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
});

describe("Rotate and flip outside edit mode", () => {
  it("has a handler for every rotate row", () => {
    const rows = ROTATE_KEY_IDS.map((id) => keyRow(id));
    expect(handlerGaps(rotateKeyHandlers(), rows)).toEqual({ missing: [], unknown: [] });
  });

  it("] with three photos selected calls edit_turn once with the three ids, current first", async () => {
    await show(2, [4, 5]);
    fireEvent.keyDown(document.body, { key: "]" });
    await waitFor(() => expect(turns()).toHaveLength(1));
    expect(turns()[0]).toEqual({ photo_ids: [2, 4, 5], op: "rotate_right" });
    // The grid's re-run draws the turned photos through their plan.
    await waitFor(() => expect(document.querySelectorAll(".ow-tile .ow-edited").length).toBe(3));
  });

  it("[ turns the current photo left; auto-repeat and a text field do nothing", async () => {
    await show(3);
    fireEvent.keyDown(document.body, { key: "[" });
    fireEvent.keyDown(document.body, { key: "[", repeat: true });
    await waitFor(() => expect(turns()).toEqual([{ photo_ids: [3], op: "rotate_left" }]));
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
    fireEvent.keyDown(input, { key: "]" });
    await new Promise((r) => setTimeout(r, 0));
    expect(turns()).toHaveLength(1);
    input.remove();
  });

  it("tells how many photos were skipped", async () => {
    setup(2);
    await show(1, [2, 3]);
    await act(() => turn("flip", "selection"));
    expect(turns()[0]!.op).toBe("flip");
    expect(useToasts.getState().toasts.map((t) => t.message)).toContain("Skipped 2 that cannot be edited");
  });

  it("turns only the shown photo in the viewer", async () => {
    await show(2, [3]);
    await act(() => turn("rotate_right", "viewer"));
    expect(turns()).toEqual([{ photo_ids: [2], op: "rotate_right" }]);
  });
});
