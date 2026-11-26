// The details panel (docs/04-browsing.md, "The details panel"): the
// current photo's facts, camera, histogram and place, in foldable
// sections, with the typed-info parts (#24) slotted into their sections.
//
// The panel holds no current photo of its own. The page's one current
// photo (the grid's, settled by every `query_run`) is passed in, so the
// preview and the details never disagree.

import { Fragment, useId, type ReactNode } from "react";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import { IconButton, Pill, Tooltip } from "../../controls";
import { Icon } from "../../icons";
import { detailsText as t, type SectionId } from "../../text/details";
import { useAlbumStore, viewOnlyReason } from "../album";
import { cameraRows, factRows, formatCoordinates, type FactRow } from "./facts";
import { HistogramSection } from "./HistogramChart";
import { SECTION_IDS, sectionShown, useSections } from "./sections";
import { DateField, LocationField, NotesFields, PeopleField } from "./typed";
import { useDetail } from "./useDetail";
import "./DetailsPanel.css";

/** The current photo, as the panel is given it. */
export interface CurrentPhoto {
  id: number;
  /** The file name from the photo's row, shown in the head while the details load. */
  file_name?: string | null;
  /** The path relative to the album root from the row, the head's tooltip meanwhile. */
  rel_path?: string | null;
}

export interface DetailsPanelProps {
  /** The page's current photo, or null when nothing is selected. */
  current: CurrentPhoto | null;
  /** The close button: clears the selection. */
  onClose(): void;
  /** `photo_detail` answered `photo_not_found`: re-run the query, which settles the current photo again. */
  onNotFound(): void;
}

function Rows({ rows, children }: { rows: FactRow[]; children?: (row: FactRow) => ReactNode }) {
  return (
    <dl className="ow-details__rows">
      {rows.map((row) => (
        <Fragment key={row.label}>
          <dt className="ow-details__label">{row.label}</dt>
          <dd className="ow-details__value">{children?.(row) ?? row.value}</dd>
        </Fragment>
      ))}
    </dl>
  );
}

function Section({
  id,
  open,
  onToggle,
  children,
}: {
  id: SectionId;
  open: boolean;
  onToggle(): void;
  children: ReactNode;
}) {
  const bodyId = useId();
  return (
    <section className="ow-details__section">
      <h3 className="ow-details__heading">
        <button
          type="button"
          className="ow-details__toggle"
          aria-expanded={open}
          aria-controls={open ? bodyId : undefined}
          onClick={onToggle}
        >
          <Icon name={open ? "chevron-down" : "chevron-right"} size="sm" />
          {t.sections[id]}
        </button>
      </h3>
      {open && (
        <div id={bodyId} className="ow-details__section-body">
          {children}
        </div>
      )}
    </section>
  );
}

/** The Photo section: the fact rows, with Taken's "changed" badge. */
function PhotoFacts({ detail }: { detail: PhotoDetail }) {
  return (
    <Rows rows={factRows(detail)}>
      {(row) =>
        row.label === t.taken && detail.date.typed !== null ? (
          <>
            {row.value}{" "}
            <Tooltip text={t.changedTooltip}>
              <span className="ow-details__badge" tabIndex={0}>
                <Pill tone="accent">{t.changed}</Pill>
              </span>
            </Tooltip>
          </>
        ) : null
      }
    </Rows>
  );
}

function CameraFacts({ detail }: { detail: PhotoDetail }) {
  const rows = cameraRows(detail.camera);
  if (rows.length === 0) return <p className="ow-details__muted">{t.noCamera}</p>;
  return <Rows rows={rows} />;
}

function PlaceFacts({ detail, disabledReason }: { detail: PhotoDetail; disabledReason: string | null }) {
  const { lat, lon, country } = detail.place;
  const rows: FactRow[] = [];
  if (lat !== null && lon !== null) {
    if (country !== null) rows.push({ label: t.country, value: country });
    rows.push({ label: t.coordinates, value: formatCoordinates(lat, lon) });
  }
  return (
    <>
      {rows.length > 0 && <Rows rows={rows} />}
      <LocationField detail={detail} disabledReason={disabledReason} />
    </>
  );
}

function SectionBody({
  id,
  detail,
  disabledReason,
}: {
  id: SectionId;
  detail: PhotoDetail;
  disabledReason: string | null;
}) {
  const typed = { detail, disabledReason };
  switch (id) {
    case "photo":
      return <PhotoFacts detail={detail} />;
    case "whatsinit":
      // The section's content comes with #55.
      return null;
    case "camera":
      return <CameraFacts detail={detail} />;
    case "histogram":
      return <HistogramSection id={detail.id} thumbKey={detail.thumb_key} clipping={detail.clipping} />;
    case "place":
      return <PlaceFacts {...typed} />;
    case "people":
      return <PeopleField {...typed} />;
    case "date":
      return <DateField {...typed} />;
    case "notes":
      return <NotesFields {...typed} />;
  }
}

/** The panel's body for one photo. Keyed by the photo, so its local state starts again for each. */
function Body({ detail, sections }: { detail: PhotoDetail; sections: ReturnType<typeof useSections> }) {
  const disabledReason = useAlbumStore((s) => (s.status === null ? null : viewOnlyReason(s.status)));
  return (
    <div className="ow-details__body">
      {/* The score card (#28) goes first. */}
      {disabledReason !== null && <p className="ow-details__note">{t.viewOnlyNote}</p>}
      {SECTION_IDS.filter((id) => sectionShown(id, detail)).map((id) => (
        <Section key={id} id={id} open={sections.isOpen(id)} onToggle={() => sections.toggle(id)}>
          <SectionBody id={id} detail={detail} disabledReason={disabledReason} />
        </Section>
      ))}
    </div>
  );
}

export function DetailsPanel({ current, onClose, onNotFound }: DetailsPanelProps) {
  const { detail, error } = useDetail(current?.id ?? null, onNotFound);
  const sections = useSections();

  if (current === null) {
    return (
      <aside className="ow-details" aria-label={t.region}>
        <p className="ow-details__empty">{t.noPhoto}</p>
      </aside>
    );
  }

  const name = detail?.file_name ?? current.file_name ?? "";
  const path = detail?.rel_path ?? current.rel_path ?? name;
  return (
    <aside className="ow-details" aria-label={t.region}>
      <header className="ow-details__head">
        <Tooltip text={path}>
          <h2 className="ow-details__name" tabIndex={0}>
            {name}
          </h2>
        </Tooltip>
        <IconButton icon="x" label={t.close} onClick={onClose} />
      </header>
      {detail !== null && <Body key={detail.id} detail={detail} sections={sections} />}
      {detail === null && error !== null && <p className="ow-details__empty">{t.couldNotRead}</p>}
    </aside>
  );
}
