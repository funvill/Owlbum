// The details panel (docs/04-browsing.md, "The details panel"): the
// current photo's facts, camera, histogram and place, in foldable
// sections, with the typed-info parts (#24) slotted into their sections,
// and the file's EXIF tags. Its head holds Copy, which puts all of it on
// the clipboard as plain text; the file name heading and the Close
// details button were taken out on the owner's feedback (issue 167).
//
// The panel holds no current photo of its own. The page's one current
// photo (the grid's, settled by every `query_run`) is passed in, so the
// preview and the details never disagree.

import { Fragment, useEffect, useId, useState, type ReactNode } from "react";
import { api } from "../../api";
import type { ExifField } from "../../api/bindings/ExifField";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import { Button, IconButton, Pill, Tooltip } from "../../controls";
import { Icon } from "../../icons";
import { detailsText as t, type SectionId } from "../../text/details";
import { useAlbumStore, viewOnlyReason } from "../album";
import { copyPaths } from "../files/actions";
import { PickPlaceButton } from "../map/PlacePicker";
import { ScoreCard } from "../organise/ScoreCard";
import { copyDetails } from "./copy";
import { cameraRows, factRows, formatCoordinates, type FactRow } from "./facts";
import { HistogramSection } from "./HistogramChart";
import { SECTION_IDS, sectionShown, useSections } from "./sections";
import { DateField, LocationField, NotesFields, PeopleField, useInfoSaver } from "./typed";
import type { InfoSaver } from "./typed/save";
import { useDetail, type DetailState } from "./useDetail";
import { WhatsInIt } from "./WhatsInIt";
import "./DetailsPanel.css";

/** The current photo, as the panel is given it. */
export interface CurrentPhoto {
  id: number;
}

export interface DetailsPanelProps {
  /** The page's current photo, or null when nothing is selected. */
  current: CurrentPhoto | null;
  /** `photo_detail` answered `photo_not_found`: re-run the query, which settles the current photo again. */
  onNotFound(): void;
  /**
   * The current photo's details when the page reads them once for both
   * the panel and the preview (`useDetail`); without it the panel reads
   * them itself.
   */
  shared?: DetailState;
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

/**
 * The Photo section: the fact rows, with Taken's "changed" badge, and the
 * Path row: the file's absolute path with a button that copies it.
 */
function PhotoFacts({ detail }: { detail: PhotoDetail }) {
  return (
    <Rows rows={[...factRows(detail), { label: t.path, value: detail.abs_path }]}>
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
        ) : row.label === t.path ? (
          <span className="ow-details__path">
            <span className="ow-details__path-text ow-selectable">{row.value}</span>
            <IconButton icon="copy" label={t.copyPath} onClick={() => void copyPaths([detail.id])} />
          </span>
        ) : null
      }
    </Rows>
  );
}

/**
 * The EXIF section: every tag the file carries, in file order, read with
 * `photo_exif` while the section is open (a folded section reads nothing).
 */
function ExifFacts({ id, revision }: { id: number; revision: number }) {
  const [state, setState] = useState<{ id: number; fields: ExifField[] | null } | null>(null);
  useEffect(() => {
    let live = true;
    api()
      .invoke("photo_exif", { id })
      .then((fields) => live && setState({ id, fields }))
      .catch(() => live && setState({ id, fields: null }));
    return () => {
      live = false;
    };
  }, [id, revision]);
  if (state === null || state.id !== id) return null;
  if (state.fields === null) return <p className="ow-details__muted">{t.exifFailed}</p>;
  if (state.fields.length === 0) return <p className="ow-details__muted">{t.noExif}</p>;
  return (
    <dl className="ow-details__rows ow-details__exif">
      {state.fields.map((f, i) => (
        <Fragment key={`${i}:${f.group}:${f.name}`}>
          <dt className="ow-details__label" title={f.group}>
            {f.name}
          </dt>
          <dd className="ow-details__value ow-selectable">{f.value}</dd>
        </Fragment>
      ))}
    </dl>
  );
}

function CameraFacts({ detail }: { detail: PhotoDetail }) {
  const rows = cameraRows(detail.camera);
  if (rows.length === 0) return <p className="ow-details__muted">{t.noCamera}</p>;
  return <Rows rows={rows} />;
}

function PlaceFacts({
  detail,
  disabledReason,
  saver,
}: {
  detail: PhotoDetail;
  disabledReason: string | null;
  saver: InfoSaver;
}) {
  const { lat, lon, country } = detail.place;
  const rows: FactRow[] = [];
  if (lat !== null && lon !== null) {
    if (country !== null) rows.push({ label: t.country, value: country });
    rows.push({ label: t.coordinates, value: formatCoordinates(lat, lon) });
  }
  return (
    <>
      {rows.length > 0 && <Rows rows={rows} />}
      <PickPlaceButton photoId={detail.id} disabledReason={disabledReason} />
      <LocationField detail={detail} disabledReason={disabledReason} saver={saver} />
    </>
  );
}

function SectionBody({
  id,
  detail,
  disabledReason,
  saver,
}: {
  id: SectionId;
  detail: PhotoDetail;
  disabledReason: string | null;
  saver: InfoSaver;
}) {
  const typed = { detail, disabledReason, saver };
  switch (id) {
    case "photo":
      return <PhotoFacts detail={detail} />;
    case "whatsinit":
      return <WhatsInIt detail={detail} disabledReason={disabledReason} />;
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
    case "exif":
      return <ExifFacts id={detail.id} revision={detail.revision} />;
  }
}

/** The panel's body for one photo. Keyed by the photo, so its local state starts again for each. */
function Body({ detail, sections }: { detail: PhotoDetail; sections: ReturnType<typeof useSections> }) {
  const disabledReason = useAlbumStore((s) => (s.status === null ? null : viewOnlyReason(s.status)));
  const saver = useInfoSaver(detail.id);
  return (
    <div className="ow-details__body">
      <ScoreCard id={detail.id} revision={detail.revision} />
      {disabledReason !== null && <p className="ow-details__note">{t.viewOnlyNote}</p>}
      {saver.error !== null && <p className="ow-details__note ow-details__note--error">{saver.error}</p>}
      {SECTION_IDS.filter((id) => sectionShown(id, detail)).map((id) => (
        <Section key={id} id={id} open={sections.isOpen(id)} onToggle={() => sections.toggle(id)}>
          <SectionBody id={id} detail={detail} disabledReason={disabledReason} saver={saver} />
        </Section>
      ))}
    </div>
  );
}

export function DetailsPanel({ current, onNotFound, shared }: DetailsPanelProps) {
  const own = useDetail(shared === undefined ? (current?.id ?? null) : null, onNotFound);
  const { detail, error } = shared ?? own;
  const sections = useSections();

  if (current === null) {
    return (
      <aside className="ow-details" aria-label={t.region}>
        <p className="ow-details__empty">{t.noPhoto}</p>
      </aside>
    );
  }

  // Copy acts on this photo's details only, never on an answer for the one before.
  const shown = detail !== null && detail.id === current.id ? detail : null;
  return (
    <aside className="ow-details" aria-label={t.region}>
      {shown !== null && (
        <header className="ow-details__head">
          <Tooltip text={t.copyAllTooltip}>
            <Button icon="copy" size="sm" onClick={() => void copyDetails(shown)}>
              {t.copyAll}
            </Button>
          </Tooltip>
        </header>
      )}
      {detail !== null && <Body key={detail.id} detail={detail} sections={sections} />}
      {detail === null && error !== null && <p className="ow-details__empty">{t.couldNotRead}</p>}
    </aside>
  );
}
