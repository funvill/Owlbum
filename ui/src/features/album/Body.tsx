import { albumText } from "../../text/album";
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

/**
 * The body: the start screen, a message, or the review screen. The grid
 * that fills the review screen comes with #18; until then it is an empty
 * region. While a drag is over the window it shows the drop outline.
 */
export function Body() {
  const state = useAlbumStore(bodyState);
  const opening = useAlbumStore((s) => s.opening);
  const dropOver = useAlbumStore((s) => s.dropOver);

  return (
    <div className="ow-body">
      {state === "start" && <StartScreen />}
      {state === "opening" && <p className="ow-body__message">{albumText.opening(opening?.name ?? "")}</p>}
      {state === "looking" && <p className="ow-body__message">{albumText.lookingForPhotos}</p>}
      {state === "empty" && <p className="ow-body__message">{albumText.noPhotos}</p>}
      {state === "review" && <section className="ow-review" aria-label={albumText.photosRegion} />}
      {dropOver && (
        <div className="ow-drop" role="status">
          {albumText.dropHere}
        </div>
      )}
    </div>
  );
}
