// The typed-info parts of the details panel, one component per field
// group. Until typed info is built (#24) each shows the saved value
// read-only; #24 replaces each with its field (docs/04-browsing.md, "How
// typed fields save"). The panel gives each the photo and, in a view-only
// album, the reason every field is disabled.

import type { PhotoDetail } from "../../../api/bindings/PhotoDetail";
import { Chip } from "../../../controls";
import { detailsText as t } from "../../../text/details";

export interface TypedProps {
  detail: PhotoDetail;
  /** Set in a view-only album: the reason every field is disabled. */
  disabledReason: string | null;
}

function Value({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="ow-details__typed">
      <span className="ow-details__label">{label}</span>
      <span className={value === null ? "ow-details__muted" : "ow-details__value"}>{value ?? t.empty}</span>
    </div>
  );
}

/** The Location group, under the Place section's rows. */
export function LocationField({ detail }: TypedProps) {
  if (detail.place.name === null) return null;
  return <Value label={t.placeName} value={detail.place.name} />;
}

/** The People section's body. */
export function PeopleField({ detail }: TypedProps) {
  if (detail.people.length === 0) return <p className="ow-details__muted">{t.noPeople}</p>;
  return (
    <div className="ow-details__chips">
      {detail.people.map((name) => (
        <Chip key={name} label={name} />
      ))}
    </div>
  );
}

/** The Date and time section's body: the effective date as `YYYY-MM-DD HH:MM:SS`. */
export function DateField({ detail }: TypedProps) {
  return <Value label={t.dateTaken} value={detail.date.effective?.replace("T", " ") ?? null} />;
}

/** The Description and notes section's body. */
export function NotesFields({ detail }: TypedProps) {
  return (
    <>
      <Value label={t.description} value={detail.description} />
      <Value label={t.notes} value={detail.notes} />
    </>
  );
}
