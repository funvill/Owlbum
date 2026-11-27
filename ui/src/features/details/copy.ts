// The details panel's Copy button (docs/04-browsing.md, "The details
// panel"; owner's feedback, issue 167): everything the panel shows, as
// plain text, one "Label: value" line per row and a blank line between
// sections. The text is built here from the same rows the panel draws, so
// the two never disagree; Rust only puts it on the clipboard.

import { api, toAppError } from "../../api";
import type { ExifField } from "../../api/bindings/ExifField";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import type { ScoreBreakdown } from "../../api/bindings/ScoreBreakdown";
import { showToast } from "../../controls/toastQueue";
import { detailsText as t } from "../../text/details";
import { cameraRows, clippingParts, factRows, formatCoordinates, type FactRow } from "./facts";

/** A total with a real minus sign when it is below zero. */
function plain(n: number): string {
  return n < 0 ? `−${-n}` : String(n);
}

function lines(rows: FactRow[]): string[] {
  return rows.map((r) => `${r.label}: ${r.value}`);
}

/**
 * The panel as plain text: the score, then each section in the panel's
 * order that has something to say, then the EXIF tags. `score` and
 * `exif` are null when they could not be read.
 */
export function detailsAsText(d: PhotoDetail, score: ScoreBreakdown | null, exif: ExifField[] | null): string {
  const sections: [string, string[]][] = [];
  if (score !== null) sections.push([t.scoreLabel, [`${t.scoreLabel}: ${plain(score.total)}`]]);
  sections.push([t.sections.photo, [...lines(factRows(d)), `${t.path}: ${d.abs_path}`]]);
  sections.push([t.sections.camera, lines(cameraRows(d.camera))]);
  if (d.clipping !== null && d.kind !== "video") {
    sections.push([t.sections.histogram, clippingParts(d.clipping).map((p) => p.text)]);
  }
  const place: string[] = [];
  const { lat, lon, country, name } = d.place;
  if (lat !== null && lon !== null) {
    if (country !== null) place.push(`${t.country}: ${country}`);
    place.push(`${t.coordinates}: ${formatCoordinates(lat, lon)}`);
  }
  if (name !== null && name !== "") place.push(`${t.placeName}: ${name}`);
  sections.push([t.sections.place, place]);
  sections.push([t.sections.people, d.people.length > 0 ? [d.people.join(", ")] : []]);
  const notes: string[] = [];
  if (d.description !== null && d.description !== "") notes.push(`${t.description}: ${d.description}`);
  if (d.notes !== null && d.notes !== "") notes.push(`${t.notes}: ${d.notes}`);
  sections.push([t.sections.notes, notes]);
  if (exif !== null && exif.length > 0) {
    sections.push([t.sections.exif, exif.map((f) => `${f.name}: ${f.value}`)]);
  }
  return sections
    .filter(([, body]) => body.length > 0)
    .map(([title, body]) => [title, ...body].join("\n"))
    .join("\n\n");
}

/**
 * Reads the score and the EXIF tags afresh, builds the text and puts it on
 * the clipboard, with a toast either way. A score or EXIF read that fails
 * leaves that part out rather than copying nothing.
 */
export async function copyDetails(d: PhotoDetail): Promise<void> {
  const [score, exif] = await Promise.all([
    api()
      .invoke("photo_score", { id: d.id })
      .catch(() => null),
    api()
      .invoke("photo_exif", { id: d.id })
      .catch(() => null),
  ]);
  try {
    await api().invoke("file_copy_text", { content: detailsAsText(d, score, exif) });
    showToast({ kind: "success", message: t.copiedAll });
  } catch (e) {
    showToast({ kind: "error", message: toAppError(e).message });
  }
}
