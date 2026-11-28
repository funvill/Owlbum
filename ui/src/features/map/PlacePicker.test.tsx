import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import L from "leaflet";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { useToasts } from "../../controls";
import { mapText } from "../../text/map";
import { seriousFindings } from "../../test/axe";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { PhotoMap } from "./photoMap";
import { PickPlaceButton, PlacePicker, pickTargets } from "./PlacePicker";

const t = mapText.picker;

beforeAll(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterAll(() => vi.unstubAllGlobals());

/** a and c have a place; b has none; d is in Paris. */
const PHOTOS: FakePhoto[] = [
  { id: 1, rel_path: "a.jpg", place: [49.25, -123.1] },
  { id: 2, rel_path: "b.jpg" },
  { id: 3, rel_path: "c.jpg", place: [-33.9, 151.2] },
  { id: 4, rel_path: "d.jpg", place: [48.85, 2.29] },
];

let fake: FakeApi;
let setPoints: MockInstance<PhotoMap["setPoints"]>;

beforeEach(() => {
  resetGrid();
  useToasts.setState({ toasts: [] });
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(createQueryFake(PHOTOS).handlers);
  setApi(fake);
  setPoints = vi.spyOn(PhotoMap.prototype, "setPoints");
});

afterEach(() => {
  vi.restoreAllMocks();
  resetGrid();
});

async function openPicker(ids: number[], onClose = () => {}) {
  render(<PlacePicker open ids={ids} onClose={onClose} />);
  await waitFor(() => expect(setPoints).toHaveBeenCalled());
  return setPoints.mock.contexts[0] as PhotoMap;
}

const setInfo = () => fake.calls.filter((c) => c.command === "photo_set_info").map((c) => c.args);

describe("the place picker", () => {
  it("shows the album's other located photos, whatever the filter", async () => {
    useGridStore.setState({ query: { ...useGridStore.getState().query, filter: { star: "yes" } } });
    const map = await openPicker([1]);
    expect([...map.shown().keys()].sort()).toEqual([3, 4]);
    const run = fake.calls.find((c) => c.command === "query_run");
    expect(run?.args).toMatchObject({ query: { view: "map", filter: {}, stack: false }, keep: null });
    expect(screen.getByRole("dialog", { name: t.title(1) })).toBeInTheDocument();
    expect(await seriousFindings()).toEqual([]);
  });

  it("sets the clicked place, with the country there as its name, for every photo", async () => {
    let closed = false;
    const map = await openPicker([2, 3], () => (closed = true));
    expect(screen.getByRole("dialog", { name: t.title(2) })).toBeInTheDocument();
    act(() => {
      map.map.fire("click", { latlng: L.latLng(46.1234567, 362.5) });
    });
    await waitFor(() => expect(closed).toBe(true));
    expect(setInfo()).toEqual([{ ids: [2, 3], patch: { location: { lat: 46.123457, lon: 2.5, name: "France" } } }]);
  });

  it("uses another photo's place when its marker is clicked", async () => {
    const map = await openPicker([2]);
    await map.whenLoaded();
    act(() => {
      map.marker(3)!.fire("click", { originalEvent: new MouseEvent("click") });
    });
    await waitFor(() => expect(setInfo()).toHaveLength(1));
    expect(setInfo()[0]).toEqual({ ids: [2], patch: { location: { lat: -33.9, lon: 151.2, name: "Australia" } } });
  });

  it("keeps the picker open and says why when the save fails (a pick at sea has no name)", async () => {
    let closed = false;
    fake = createFakeApi({
      ...createQueryFake(PHOTOS).handlers,
      photo_set_info: () => {
        throw appError("view_only", "This album is view-only.");
      },
    });
    setApi(fake);
    const map = await openPicker([2], () => (closed = true));
    act(() => {
      map.map.fire("click", { latlng: L.latLng(0, -30) });
    });
    await waitFor(() => expect(useToasts.getState().toasts).toHaveLength(1));
    expect(closed).toBe(false);
    expect(fake.calls.find((c) => c.command === "app_country_at")?.args).toEqual({ lat: 0, lon: -30 });
  });

  it("fills the window from its full-screen button; Esc then leaves full screen, not the picker", async () => {
    const onClose = vi.fn();
    await openPicker([1], onClose);
    fireEvent.click(screen.getByRole("button", { name: mapText.fullScreen }));
    const frame = () => document.querySelector(".ow-picker__frame");
    expect(frame()).toHaveClass("ow-map__frame--full");
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(frame()).not.toHaveClass("ow-map__frame--full");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("asks before loading map tiles", async () => {
    useSettingsStore.setState({ settings: { ...(await fake.invoke("app_settings_get")), maps_online: null } });
    const map = await openPicker([2]);
    expect(screen.getByRole("button", { name: mapText.consent.load })).toBeInTheDocument();
    expect(map.hasTiles()).toBe(false);
  });
});

describe("the Pick on map button", () => {
  it("changes the whole selection when the photo is the current one", () => {
    useGridStore.setState({ current: { id: 3, index: 2 }, selection: [1, 3] });
    expect(pickTargets(3)).toEqual([3, 1]);
    expect(pickTargets(4)).toEqual([4]);
    useGridStore.setState({ current: { id: 3, index: 2 }, selection: [3] });
    expect(pickTargets(3)).toEqual([3]);
  });

  it("opens the picker, and is disabled in a view-only album", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<PickPlaceButton photoId={2} disabledReason={null} />);
    await user.click(screen.getByRole("button", { name: t.open }));
    expect(await screen.findByRole("dialog", { name: t.title(1) })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t.cancel }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    unmount();
    render(<PickPlaceButton photoId={2} disabledReason="View only" />);
    await user.click(screen.getByRole("button", { name: t.open }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
