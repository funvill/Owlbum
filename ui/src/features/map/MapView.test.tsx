import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import type { MapPoint } from "../../api/bindings/MapPoint";
import { useToasts } from "../../controls";
import { read, sourceFiles, srcDir } from "../../test/sources";
import { mapText } from "../../text/map";
import { timelineText } from "../../text/timeline";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { ViewBar } from "../timeline";
import { MapView, resetMapPlace } from "./MapView";
import { PhotoMap } from "./photoMap";
import { diffPoints } from "./points";
import { join } from "node:path";

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

/** Four photos in name order a..d; b and d have no place. */
const PHOTOS: FakePhoto[] = [
  { id: 1, rel_path: "a.jpg", place: [49.25, -123.1] },
  { id: 2, rel_path: "b.jpg" },
  { id: 3, rel_path: "c.jpg", place: [-33.9, 151.2] },
  { id: 4, rel_path: "d.jpg", place: null },
];

let fake: FakeApi;

beforeEach(() => {
  resetGrid();
  resetMapPlace();
  useToasts.setState({ toasts: [] });
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi(createQueryFake(PHOTOS).handlers);
  setApi(fake);
});

afterEach(() => resetGrid());

const grid = () => useGridStore.getState();

async function openMap() {
  render(
    <div>
      <ViewBar />
      <MapView album="/photos" />
    </div>,
  );
  await grid().start();
  grid().setQuery({ ...grid().query, view: "map", sort: "name" });
  await waitFor(() => expect(fake.calls.some((c) => c.command === "query_map")).toBe(true));
}

describe("the map view", () => {
  it("is chosen in the view switch and saved as the view", async () => {
    render(<ViewBar />);
    await grid().start();
    await userEvent.click(await screen.findByRole("tab", { name: timelineText.views.map }));
    await waitFor(() => expect(grid().query.view).toBe("map"));
    await waitFor(() => expect(useSettingsStore.getState().settings?.view).toBe("map"));
  });

  it("reads the located photos and says how many have no place", async () => {
    await openMap();
    expect(await screen.findByText("2 photos have no location")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: mapText.region })).toBeInTheDocument();
    expect(screen.queryByText(mapText.empty)).toBeNull();
  });

  it("shows the empty state when no photo has a place", async () => {
    fake = createFakeApi(createQueryFake([{ id: 1, rel_path: "a.jpg" }]).handlers);
    setApi(fake);
    await openMap();
    expect(await screen.findByText(mapText.empty)).toBeInTheDocument();
    expect(await screen.findByText("1 photo has no location")).toBeInTheDocument();
  });

  it("asks before loading tiles, and Load maps saves the answer and loads them", async () => {
    await openMap();
    expect(await screen.findByText(mapText.consent.text)).toBeInTheDocument();
    expect(document.querySelector(".leaflet-tile-pane .leaflet-layer")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: mapText.consent.load }));
    await waitFor(() => expect(useSettingsStore.getState().settings?.maps_online).toBe(true));
    expect(await screen.findByRole("button", { name: mapText.toggle.online })).toBeInTheDocument();
    expect(screen.queryByText(mapText.consent.text)).toBeNull();
    expect(document.querySelector(".leaflet-tile-pane .leaflet-layer")).not.toBeNull();
    expect(screen.getByText(mapText.attribution)).toBeInTheDocument();

    // Switching off removes the layer at once, then saves.
    await userEvent.click(screen.getByRole("button", { name: mapText.toggle.online }));
    expect(document.querySelector(".leaflet-tile-pane .leaflet-layer")).toBeNull();
    expect(screen.getByRole("button", { name: mapText.toggle.offline })).toBeInTheDocument();
    await waitFor(() => expect(useSettingsStore.getState().settings?.maps_online).toBe(false));
  });

  it("Not now answers no: markers stay on a plain background", async () => {
    await openMap();
    await userEvent.click(await screen.findByRole("button", { name: mapText.consent.notNow }));
    expect(await screen.findByRole("button", { name: mapText.toggle.offline })).toBeInTheDocument();
    expect(document.querySelector(".leaflet-tile-pane .leaflet-layer")).toBeNull();
  });

  it("keeps tiles off and shows a toast when the answer can't be saved", async () => {
    fake.handle("app_settings_set", () => {
      throw appError("internal", "disk full");
    });
    await openMap();
    await userEvent.click(await screen.findByRole("button", { name: mapText.consent.load }));
    await waitFor(() => expect(useToasts.getState().toasts.map((x) => x.message)).toEqual([mapText.saveFailed]));
    expect(document.querySelector(".leaflet-tile-pane .leaflet-layer")).toBeNull();
  });

  it("steps through the whole result with the arrow keys, located or not", async () => {
    await openMap();
    await screen.findByText("2 photos have no location");
    // The first photo is current once the result arrives.
    await waitFor(() => expect(grid().current).toEqual({ id: 1, index: 0 }));
    fireEvent.keyDown(window, { key: "ArrowRight" });
    await waitFor(() => expect(grid().current).toEqual({ id: 2, index: 1 }));
    fireEvent.keyDown(window, { key: "End" });
    await waitFor(() => expect(grid().current).toEqual({ id: 4, index: 3 }));
  });

  it("asks for the rows around the current photo, as the grid would", async () => {
    await openMap();
    await screen.findByText("2 photos have no location");
    await waitFor(() => expect(fake.calls.some((c) => c.command === "query_window")).toBe(true));
  });
});

describe("the map's markers", () => {
  const point = (id: number, lat = 1, lon = 2, key = `tk-${id}`): MapPoint => [id, lat, lon, key, `${id}.jpg`];

  it("keeps markers whose id, place and thumbnail are unchanged", () => {
    const shown = new Map([point(1), point(2), point(3)].map((p) => [p[0], p]));
    expect(diffPoints(shown, [point(1), point(2, 5), point(4)])).toEqual({
      add: [point(2, 5), point(4)],
      remove: [2, 3],
    });
    expect(diffPoints(shown, [point(3, 1, 2, "new")]).remove).toEqual([1, 2, 3]);
  });

  it("a click picks the photo, with Ctrl adding it", () => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    const picks: [number, boolean][] = [];
    const map = new PhotoMap(el, { onPick: (id, add) => picks.push([id, add]), onTiles: () => {} });
    act(() => map.setPoints([point(1), point(2)]));
    const marker = map.marker(2)!;
    expect(marker.options.title).toBe("2.jpg");
    marker.fire("click", { originalEvent: new MouseEvent("click") });
    marker.fire("click", { originalEvent: new MouseEvent("click", { ctrlKey: true }) });
    expect(picks).toEqual([
      [2, false],
      [2, true],
    ]);
    map.setPoints([point(1)]);
    expect(map.marker(2)).toBeUndefined();
    expect(map.marker(1)).toBeDefined();
    map.destroy();
    el.remove();
  });

  it("never uses the library's default marker image", () => {
    const el = document.createElement("div");
    const map = new PhotoMap(el, { onPick: () => {}, onTiles: () => {} });
    map.setPoints([point(1)]);
    const icon = map.marker(1)!.options.icon!;
    expect(icon.options.className).toBe("ow-map-pin");
    expect((icon.options as { html?: string }).html).toContain("thumb/tk-1/s");
    map.destroy();
  });
});

describe("the tile address", () => {
  it("appears only in the tile layer's set-up", () => {
    const files = sourceFiles([".ts", ".tsx"]).filter(
      (f) => !/\.test\.tsx?$/.test(f) && read(join(srcDir, f)).includes("tile.openstreetmap.org"),
    );
    expect(files).toEqual(["features/map/photoMap.ts"]);
  });
});
