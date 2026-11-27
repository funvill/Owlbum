// Which addresses the zoom component loads for a photo (docs/04-browsing.md,
// "Which picture is shown"; docs/03-media-and-scanning.md, "Animated
// pictures").

import { fullUrl, originalUrl, thumbUrl } from "../../api";
import type { Row } from "../../api/bindings/Row";
import type { StagePictures } from "./ZoomStage";

/**
 * An animated GIF or WebP: a still (`kind` image) with a loop length. An
 * animated PNG has none, so it stays a still.
 */
export function isAnimated(row: Pick<Row, "kind" | "duration_ms">): boolean {
  return row.kind === "image" && row.duration_ms !== null;
}

/**
 * The pictures of `row` for the preview and the viewer. An animated
 * picture plays its original file in place of the medium and full-size
 * pictures; its small thumbnail (frame 1) shows until the file is decoded.
 */
export function stagePictures(
  row: Pick<Row, "id" | "thumb_key" | "kind" | "duration_ms">,
): StagePictures {
  return {
    small: thumbUrl(row.thumb_key, "s"),
    medium: thumbUrl(row.thumb_key, "m"),
    full: fullUrl(row.id, row.thumb_key),
    animated: isAnimated(row) ? originalUrl(row.id, row.thumb_key) : null,
  };
}
