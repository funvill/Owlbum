import { useEffect } from "react";
import { albumText } from "../../text/album";
import { useGridStore } from "../grid";
import { imageCache } from "../preview";
import { Review } from "./Review";
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
 * The body: the start screen, a message, or the review screen (the grid
 * and the details panel). While a drag is over the window it shows the
 * drop outline.
 */
export function Body() {
  const state = useAlbumStore(bodyState);
  const opening = useAlbumStore((s) => s.opening);
  const dropOver = useAlbumStore((s) => s.dropOver);
  const albumOpen = useAlbumStore((s) => s.status !== null);

  // The grid's result and the decoded pictures belong to the open album: they
  // go when the album closes (opening another album closes this one first).
  useEffect(() => {
    if (albumOpen) return;
    useGridStore.getState().stop();
    imageCache().clear();
  }, [albumOpen]);

  return (
    <div className="ow-body">
      {state === "start" && <StartScreen />}
      {state === "opening" && <p className="ow-body__message">{albumText.opening(opening?.name ?? "")}</p>}
      {state === "looking" && <p className="ow-body__message">{albumText.lookingForPhotos}</p>}
      {state === "empty" && <p className="ow-body__message">{albumText.noPhotos}</p>}
      {state === "review" && <Review busy={scanRunning} />}
      {dropOver && (
        <div className="ow-drop" role="status">
          {albumText.dropHere}
        </div>
      )}
    </div>
  );
}
