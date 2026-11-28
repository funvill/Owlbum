// Map-tile consent, shared by the Map view and the place picker
// (docs/04-browsing.md, "Loading map tiles"; docs/08-desktop-and-release.md,
// "Map-tile consent"): the first-use card, the online/offline switch and
// the pill shown when a tile fails.

import { useState } from "react";
import { Button, Tooltip, showToast } from "../../controls";
import { mapText as t } from "../../text/map";
import { useSettingsStore } from "../app";

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

export interface MapTiles {
  /** The setting: unset (null), true or false; undefined before settings are read. */
  mapsOnline: boolean | null | undefined;
  /** Whether the tile layer should exist now. */
  tilesOn: boolean;
  turnOn(): Promise<void>;
  turnOff(): void;
  notNow(): void;
}

/** The tile setting and its switch. Switching off takes effect at once; switching on once saved. */
export function useMapTiles(): MapTiles {
  const mapsOnline = useSettingsStore((s) => (s.settings === null ? undefined : s.settings.maps_online));
  // Switching off takes effect at once, before the save answers, and stays off if it fails.
  const [forcedOff, setForcedOff] = useState(false);
  return {
    mapsOnline,
    tilesOn: mapsOnline === true && !forcedOff,
    turnOn: async () => {
      if (await saveMapsOnline(true)) setForcedOff(false);
    },
    turnOff: () => {
      setForcedOff(true);
      void saveMapsOnline(false);
    },
    notNow: () => void saveMapsOnline(false),
  };
}

/** The card, the switch and the failure pill, drawn over a map stage. */
export function TileControls({ tiles, failing }: { tiles: MapTiles; failing: boolean }) {
  const { mapsOnline, tilesOn } = tiles;
  return (
    <>
      {mapsOnline === null && (
        <div className="ow-map__consent" role="group" aria-label={t.consent.load}>
          <p>{t.consent.text}</p>
          <div className="ow-map__consent-buttons">
            <Button kind="primary" onClick={() => void tiles.turnOn()}>
              {t.consent.load}
            </Button>
            <Button onClick={tiles.notNow}>{t.consent.notNow}</Button>
          </div>
        </div>
      )}
      {(mapsOnline === true || mapsOnline === false) && (
        <div className="ow-map__toggle">
          <Tooltip text={tilesOn ? t.toggle.onlineTip : t.toggle.offlineTip}>
            <Button size="sm" onClick={tilesOn ? tiles.turnOff : () => void tiles.turnOn()}>
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
    </>
  );
}
