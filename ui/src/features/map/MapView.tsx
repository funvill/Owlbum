import { useEffect, useRef, useState } from "react";
import { api, isAppError } from "../../api";
import type { MapPoints } from "../../api/bindings/MapPoints";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import { Button, EmptyState, Tooltip, showToast } from "../../controls";
import { mapText as t } from "../../text/map";
import { useSettingsStore } from "../app";
import { gridBlocks, pickById, useBrowseKeys, useGridStore } from "../grid";
import { PhotoMap } from "./photoMap";
import "./MapView.css";

export interface MapViewProps {
  /** The open folder: the map fits its photos once per album, and keeps its place between views. */
  album: string;
  /** Open the viewer on the current photo: Enter or Space. */
  onOpen?: (at: PhotoAt) => void;
}

/** Where the map was left, so switching views and back keeps it. */
let kept: { album: string; lat: number; lon: number; zoom: number } | null = null;

/** Forgets where the map was left (tests). */
export function resetMapPlace(): void {
  kept = null;
}

/**
 * Saves `maps_online`. Answers whether the save worked; a failure shows
 * the toast "Couldn't change the map setting.".
 */
async function saveMapsOnline(on: boolean): Promise<boolean> {
  const settings = useSettingsStore.getState();
  await settings.update({ maps_online: on });
  if (useSettingsStore.getState().error === null) return true;
  showToast({ kind: "error", message: t.saveFailed });
  return false;
}

/**
 * The Map view (docs/04-browsing.md, "Map view"): every photo of the result
 * that has a place, as a marker on a map, in place of the thumbnails. The
 * current photo, the selection and the keys are the grid's: Left, Right,
 * Home and End step through the map's whole result, located or not.
 */
export function MapView({ album, onOpen }: MapViewProps) {
  useBrowseKeys({ onOpen });
  const token = useGridStore((s) => s.result?.token ?? null);
  const current = useGridStore((s) => s.current);
  const pending = useGridStore((s) => s.pending);
  const anchor = useGridStore((s) => s.anchor);
  const mapsOnline = useSettingsStore((s) => (s.settings === null ? undefined : s.settings.maps_online));
  // Switching off takes effect at once, before the save answers, and stays off if it fails.
  const [forcedOff, setForcedOff] = useState(false);
  const tilesOn = mapsOnline === true && !forcedOff;
  const [answer, setAnswer] = useState<{ token: string; map: MapPoints } | null>(null);
  const [failing, setFailing] = useState(false);
  const stageRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<PhotoMap | null>(null);
  const fitted = useRef(false);

  // The map itself, for as long as the view is shown.
  useEffect(() => {
    const el = stageRef.current;
    if (el === null) return;
    const map = new PhotoMap(el, {
      onPick: (id, add) => void pickById(id, add),
      onTiles: setFailing,
    });
    mapRef.current = map;
    if (kept !== null && kept.album === album) {
      map.map.setView([kept.lat, kept.lon], kept.zoom, { animate: false });
      fitted.current = true;
    } else {
      fitted.current = false;
    }
    const remember = () => {
      const c = map.map.getCenter();
      kept = { album, lat: c.lat, lon: c.lng, zoom: map.map.getZoom() };
    };
    map.map.on("moveend", remember);
    let observer: ResizeObserver | undefined;
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => {
        map.resized();
        if (!fitted.current) fitted.current = map.fit();
      });
      observer.observe(el);
    }
    return () => {
      observer?.disconnect();
      map.map.off("moveend", remember);
      map.destroy();
      mapRef.current = null;
    };
  }, [album]);

  // The located photos of each new result.
  useEffect(() => {
    if (token === null) return;
    let live = true;
    api()
      .invoke("query_map", { token })
      .then(
        (map) => {
          if (live) setAnswer({ token, map });
        },
        (e: unknown) => {
          if (live && isAppError(e) && e.code === "query_expired") void useGridStore.getState().refresh();
        },
      );
    return () => {
      live = false;
    };
  }, [token]);

  // Markers follow the answer; the first points of an album are fitted once.
  useEffect(() => {
    const map = mapRef.current;
    if (map === null || answer === null) return;
    map.setPoints(answer.map.points);
    map.setCurrent(useGridStore.getState().current?.id ?? null, false);
    if (!fitted.current) fitted.current = map.fit();
  }, [answer]);

  // The current photo's marker, brought into view when it changes.
  const currentId = current?.id ?? null;
  useEffect(() => {
    mapRef.current?.setCurrent(currentId, true);
  }, [currentId]);

  // The grid is not drawn, so the rows around the current photo are asked
  // for here: the preview, the details and a step onto a row not fetched
  // yet all wait on them.
  const at = pending?.index ?? current?.index ?? null;
  const anchorAt = anchor?.index ?? null;
  useEffect(() => {
    if (token === null) return;
    const index = at ?? 0;
    gridBlocks().want({ first: index, last: index, span: 1, direction: 1, current: at, anchor: anchorAt });
  }, [token, at, anchorAt]);

  // The one tile layer, only while the user has said yes.
  useEffect(() => {
    // Removing the layer also clears the failure pill (through `onTiles`).
    mapRef.current?.setTiles(tilesOn);
  }, [tilesOn]);

  const turnOn = async () => {
    if (await saveMapsOnline(true)) setForcedOff(false);
  };
  const turnOff = () => {
    setForcedOff(true);
    void saveMapsOnline(false);
  };

  // The last answer stays until the next one arrives, so nothing flashes.
  const map = answer?.map ?? null;
  return (
    <div className="ow-map">
      {map !== null && map.unlocated > 0 && <p className="ow-map__note">{t.unlocated(map.unlocated)}</p>}
      <div className="ow-map__frame">
        <div ref={stageRef} className="ow-map__stage" role="region" aria-label={t.region} />
        {map !== null && map.points.length === 0 && (
          <div className="ow-map__empty">
            <EmptyState heading={t.empty} />
          </div>
        )}
        {mapsOnline === null && (
          <div className="ow-map__consent" role="group" aria-label={t.consent.load}>
            <p>{t.consent.text}</p>
            <div className="ow-map__consent-buttons">
              <Button kind="primary" onClick={() => void turnOn()}>
                {t.consent.load}
              </Button>
              <Button onClick={() => void saveMapsOnline(false)}>{t.consent.notNow}</Button>
            </div>
          </div>
        )}
        {(mapsOnline === true || mapsOnline === false) && (
          <div className="ow-map__toggle">
            <Tooltip text={tilesOn ? t.toggle.onlineTip : t.toggle.offlineTip}>
              <Button size="sm" onClick={tilesOn ? turnOff : () => void turnOn()}>
                {tilesOn ? t.toggle.online : t.toggle.offline}
              </Button>
            </Tooltip>
          </div>
        )}
        {tilesOn && failing && (
          <p className="ow-map__offline" role="status">
            {t.offline}
          </p>
        )}
      </div>
    </div>
  );
}
