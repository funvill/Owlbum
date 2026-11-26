import { useEffect } from "react";
import { albumText } from "../../text/album";
import { BrowseGrid, useGridStore } from "../grid";
import { useAlbumStore } from "./session";
import { StartScreen } from "./StartScreen";

/** Which of the body's states shows (docs/04-browsing.md, "Body states"). */
export type BodyState = "start" | "opening" | "looking" | "empty" | "review";

/** The body state for what the store holds. */
export function bodyState(s: {
  opening: unknown;
  status: { photos: number; scanning: boolean } | null;
  reading: unknown;
}): BodyState {
  if (s.opening !== null) return "opening";
  if (s.status === null) return "start";
  if (s.status.photos > 0) return "review";
  return s.status.scanning || s.reading !== null ? "looking" : "empty";
}

/** A scan is running, so the grid spaces its re-runs a second apart. */
const scanRunning = () => {
  const s = useAlbumStore.getState();
  return s.reading !== null || (s.status?.scanning ?? false);
};

/**
 * The body: the start screen, a message, or the review screen, which
 * holds the grid (#18). While a drag is over the window it shows the drop
 * outline.
 */
export function Body() {
  const state = useAlbumStore(bodyState);
  const opening = useAlbumStore((s) => s.opening);
  const dropOver = useAlbumStore((s) => s.dropOver);
  const albumOpen = useAlbumStore((s) => s.status !== null);

  // The grid's result belongs to the open album: it goes when the album closes
  // (opening another album closes this one first).
  useEffect(() => {
    if (!albumOpen) useGridStore.getState().stop();
  }, [albumOpen]);

  return (
    <div className="ow-body">
      {state === "start" && <StartScreen />}
      {state === "opening" && <p className="ow-body__message">{albumText.opening(opening?.name ?? "")}</p>}
      {state === "looking" && <p className="ow-body__message">{albumText.lookingForPhotos}</p>}
      {state === "empty" && <p className="ow-body__message">{albumText.noPhotos}</p>}
      {state === "review" && (
        <section className="ow-review" aria-label={albumText.photosRegion}>
          <BrowseGrid busy={scanRunning} />
        </section>
      )}
      {dropOver && (
        <div className="ow-drop" role="status">
          {albumText.dropHere}
        </div>
      )}
    </div>
  );
}
