// The place picker (docs/04-browsing.md, "The place picker"): a map, the
// same as the Map view's, holding the album's other located photos, where
// a click sets the place of the photos being changed.

import { useEffect, useRef, useState } from "react";
import { api, toAppError } from "../../api";
import type { MapPoint } from "../../api/bindings/MapPoint";
import { Button, Dialog, showToast } from "../../controls";
import { mapText } from "../../text/map";
import { useGridStore } from "../grid";
import { PhotoMap } from "./photoMap";
import { TileControls, useMapTiles } from "./tiles";
import "./MapView.css";

const t = mapText.picker;

export interface PlacePickerProps {
  open: boolean;
  /** The photos whose place is set: the current photo first. */
  ids: readonly number[];
  onClose(): void;
}

/** A coordinate to 6 decimals (about 10 cm), so nothing longer is stored than a click can mean. */
const round = (n: number) => Math.round(n * 1e6) / 1e6;

/**
 * Every located photo of the album, whatever the filter: a query of its
 * own, with no filter and no stacks, in the grid's order.
 */
async function albumPoints(): Promise<MapPoint[]> {
  const shown = useGridStore.getState().query;
  const result = await api().invoke("query_run", {
    query: { ...shown, view: "map", stack: false, open_stacks: [], filter: {}, search: null, want_facets: false },
    keep: null,
  });
  const map = await api().invoke("query_map", { token: result.token });
  return map.points;
}

/**
 * Sends the picked place for `ids`: latitude, longitude and, as the place
 * name, the country there (none at sea), as typed info in one undoable step.
 */
async function savePlace(ids: readonly number[], lat: number, lon: number): Promise<boolean> {
  try {
    const name = await api().invoke("app_country_at", { lat, lon });
    await api().invoke("photo_set_info", { ids: [...ids], patch: { location: { lat, lon, name } } });
    return true;
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
    return false;
  }
}

/**
 * The photos a pick changes: the whole selection when the photo shown is
 * the grid's current photo and several are selected, else that photo.
 */
export function pickTargets(photoId: number): number[] {
  const grid = useGridStore.getState();
  if (grid.current?.id !== photoId || grid.selection.length <= 1) return [photoId];
  return [photoId, ...grid.selection.filter((id) => id !== photoId)];
}

/**
 * The Place section's "Pick on map" button and its picker. Disabled, with
 * the reason as its tooltip, in a view-only album.
 */
export function PickPlaceButton({ photoId, disabledReason }: { photoId: number; disabledReason: string | null }) {
  const [ids, setIds] = useState<number[] | null>(null);
  return (
    <>
      <Button
        size="sm"
        icon="map-pin"
        title={disabledReason === null ? t.openTip : undefined}
        disabledReason={disabledReason ?? undefined}
        onClick={() => setIds(pickTargets(photoId))}
      >
        {t.open}
      </Button>
      {ids !== null && <PlacePicker open ids={ids} onClose={() => setIds(null)} />}
    </>
  );
}

export function PlacePicker({ open, ids, onClose }: PlacePickerProps) {
  return (
    <Dialog open={open} title={t.title(ids.length)} size="wide" onClose={onClose} actions={<Button onClick={onClose}>{t.cancel}</Button>}>
      <PickerMap ids={ids} onDone={onClose} />
    </Dialog>
  );
}

function PickerMap({ ids, onDone }: { ids: readonly number[]; onDone(): void }) {
  const stageRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<PhotoMap | null>(null);
  const tiles = useMapTiles();
  const [failing, setFailing] = useState(false);
  const [saving, setSaving] = useState(false);
  // What a pick does, kept current without making the map again. One pick
  // at a time.
  const busy = useRef(false);
  const pick = useRef<(lat: number, lon: number) => void>(() => {});
  useEffect(() => {
    pick.current = (lat, lon) => {
      if (busy.current) return;
      busy.current = true;
      setSaving(true);
      void savePlace(ids, round(lat), round(lon)).then((ok) => {
        if (ok) {
          onDone();
          return;
        }
        busy.current = false;
        setSaving(false);
      });
    };
  });
  const key = ids.join(",");

  useEffect(() => {
    const el = stageRef.current;
    if (el === null) return;
    const targets = new Set(key.split(",").map(Number));
    const map = new PhotoMap(el, {
      // Another photo's marker: use its place.
      onPick: (id) => {
        const p = map.shown().get(id);
        if (p !== undefined) pick.current(p[1], p[2]);
      },
      onTiles: setFailing,
    });
    mapRef.current = map;
    map.map.on("click", (e) => {
      const at = e.latlng.wrap();
      pick.current(at.lat, at.lng);
    });
    let fitted = false;
    const fit = () => {
      if (!fitted) fitted = map.fit();
    };
    let live = true;
    albumPoints().then(
      (points) => {
        if (!live) return;
        // The album's other located photos; with none the map stays on the world.
        map.setPoints(points.filter((p) => !targets.has(p[0])));
        void map.whenLoaded().then(() => {
          if (live) fit();
        });
      },
      (e: unknown) => showToast({ kind: "error", message: toAppError(e).message }),
    );
    let observer: ResizeObserver | undefined;
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => {
        map.resized();
        fit();
      });
      observer.observe(el);
    }
    return () => {
      live = false;
      observer?.disconnect();
      map.destroy();
      mapRef.current = null;
    };
  }, [key]);

  useEffect(() => {
    mapRef.current?.setTiles(tiles.tilesOn);
  }, [tiles.tilesOn]);

  return (
    <div className="ow-picker">
      <p className="ow-picker__hint" role="status">
        {saving ? t.saving : t.hint}
      </p>
      <div className="ow-map__frame ow-picker__frame">
        <div ref={stageRef} className="ow-map__stage ow-picker__stage" role="region" aria-label={t.region} />
        <TileControls tiles={tiles} failing={failing} />
      </div>
    </div>
  );
}
