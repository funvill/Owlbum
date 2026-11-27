// The full-screen viewer (docs/04-browsing.md, "Full-screen viewer").
//
// It shows the grid store's current photo on the zoom component the
// preview uses, with the same picture rules and the same image cache, and
// wants the same preload window. Stepping moves the current photo, so a
// photo that leaves the result is replaced by the one now in its place by
// the grid's own settling, and an empty result closes the viewer.

import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { fullUrl, thumbUrl } from "../../api";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import { IconButton, Tooltip, layerRoot } from "../../controls";
import { viewerBarMs } from "../../controls/timing";
import { Icon } from "../../icons";
import { matchKey, type Focus, type KeyId } from "../../keys";
import { viewerText as t } from "../../text/viewer";
import { useSettingsStore } from "../app";
import { formatLength, formatSize, formatTaken } from "../details";
import { step, stepToEnd, useGridStore } from "../grid";
import { fileKeyHandlers } from "../files/keys";
import { FlagMark, ScoreToast, flagKeyHandlers, isFlagKey } from "../organise";
import { CachedZoomStage, usePreload, useShownRow, zoomKeyHandlers, type ZoomHandle } from "../preview";
import { closeViewer, toggleDetails, useViewerStore } from "./store";
import "./Viewer.css";

/** The rows of the key table marked 04 that the viewer handles. */
export const VIEWER_KEY_IDS = [
  "viewer.previous",
  "viewer.next",
  "viewer.first",
  "viewer.last",
  "viewer.close",
  "viewer.escape",
  "viewer.play",
  "viewer.details",
  "viewer.zoom_in",
  "viewer.zoom_out",
  "viewer.fit",
] as const satisfies readonly KeyId[];

export type ViewerKeyId = (typeof VIEWER_KEY_IDS)[number];

/** What each 04 row does in the viewer. */
export function viewerKeyHandlers(zoom: ZoomHandle | null): Record<ViewerKeyId, () => void> {
  const noZoom: ZoomHandle = { zoomBy: () => {}, fit: () => {}, zoomed: () => false };
  const z = zoom ?? noZoom;
  return {
    "viewer.previous": () => step(-1),
    "viewer.next": () => step(1),
    "viewer.first": () => stepToEnd("first"),
    "viewer.last": () => stepToEnd("last"),
    "viewer.close": () => closeViewer(),
    "viewer.escape": () => (z.zoomed() ? z.fit() : closeViewer()),
    // Plays or pauses a video; does nothing for a still photo. The player is the video issue's.
    "viewer.play": () => {},
    "viewer.details": () => toggleDetails(),
    ...zoomKeyHandlers(z),
  };
}

const isViewerKey = (id: KeyId): id is ViewerKeyId => (VIEWER_KEY_IDS as readonly KeyId[]).includes(id);

/** Keys with a default the viewer blocks although no row of the table has them. */
const BLOCKED = new Set(["Tab", "PageUp", "PageDown"]);

export interface ViewerProps {
  /** The current photo's details, read once by the review screen; used only when they are this photo's. */
  detail: PhotoDetail | null;
}

export function Viewer({ detail }: ViewerProps) {
  const open = useViewerStore((s) => s.open);
  if (!open) return null;
  return createPortal(<OpenViewer detail={detail} />, layerRoot());
}

function OpenViewer({ detail }: ViewerProps) {
  const { current, pending, row } = useShownRow();
  const count = useGridStore((s) => s.result?.count ?? 0);
  const details = useViewerStore((s) => s.details);
  const editTarget = useSettingsStore((s) => s.settings?.edit_target ?? "album");
  const root = useRef<HTMLDivElement>(null);
  const zoom = useRef<ZoomHandle>(null);
  const index = pending?.index ?? current?.index ?? null;
  usePreload(index);

  // The photo left the result and nothing took its place: the result is empty.
  const gone = (current === null && pending === null) || count === 0;
  useEffect(() => {
    if (gone) closeViewer();
  }, [gone]);

  // The top bar fades after 3 seconds without pointer movement or a key.
  const [bar, setBar] = useState(true);
  const barTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const showBar = useCallback(() => {
    setBar(true);
    clearTimeout(barTimer.current);
    barTimer.current = setTimeout(() => setBar(false), viewerBarMs);
  }, []);
  useEffect(() => {
    // The bar starts shown; only the timer starts here.
    barTimer.current = setTimeout(() => setBar(false), viewerBarMs);
    return () => clearTimeout(barTimer.current);
  }, []);

  useEffect(() => {
    root.current?.focus();
  }, []);

  // While the viewer is open nothing else on the page receives keys. It
  // listens before anything else does; rows for every place (the shortcut
  // overlay, region keys) still reach their owners.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const el = root.current;
      if (el === null) return;
      // A dialog or menu opened over the viewer has the keys.
      const above = [...document.querySelectorAll('[aria-modal="true"]')].some((m) => m !== el);
      if (above) return;
      showBar();
      const target = e.target instanceof Element ? e.target : null;
      const focus: Focus = target !== null && el.contains(target) && target.closest("button") !== null ? "control" : "none";
      const match = matchKey(e, { place: "viewer", focus });
      if (match !== null && match.row.place === "everywhere") return;
      if (match !== null || BLOCKED.has(e.key)) e.preventDefault();
      e.stopPropagation();
      if (match !== null && match.act && isViewerKey(match.id)) viewerKeyHandlers(zoom.current)[match.id]();
      // F, S and H star or hide, Up and Down nudge, the shown photo only
      // (docs/05-organising.md).
      if (match !== null && match.act && isFlagKey(match.id)) flagKeyHandlers()[match.id]();
      // Ctrl+C copies the shown photo (docs/06-editing-and-file-actions.md).
      if (match !== null && match.act && match.id === "viewer.copy") fileKeyHandlers()["viewer.copy"]();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [showBar]);

  const own = row !== undefined && detail !== null && detail.id === row.id ? detail : null;
  const name = row?.file_name ?? t.region;

  return (
    <div
      ref={root}
      className="ow-viewer"
      role="dialog"
      aria-modal="true"
      aria-label={name}
      data-keys="surface"
      tabIndex={-1}
      onPointerMove={showBar}
    >
      <div className="ow-viewer__stage">
        {row !== undefined && current !== null && (
          <CachedZoomStage
            key={`${row.id}:${row.thumb_key}`}
            ready={row.thumb_key !== ""}
            pictures={{
              small: thumbUrl(row.thumb_key, "s"),
              medium: thumbUrl(row.thumb_key, "m"),
              full: fullUrl(row.id, row.thumb_key),
            }}
            aspect={row.aspect}
            original={own !== null && own.width > 0 && own.height > 0 ? { width: own.width, height: own.height } : null}
            zoomable={row.kind !== "video"}
            actualSizeAtFit
            handle={zoom}
          />
        )}
        <FlagMark place="viewer" />
        <ScoreToast place="viewer" />
      </div>
      <div className={`ow-viewer__bar${bar ? "" : " ow-viewer__bar--away"}`}>
        {row !== undefined && (
          <Tooltip text={own?.abs_path ?? row.rel_path} describe={false}>
            <span className="ow-viewer__name">{row.file_name}</span>
          </Tooltip>
        )}
        {row?.star === true && (
          <span className="ow-viewer__mark ow-viewer__mark--star" role="img" aria-label={t.starred}>
            <Icon name="star-filled" size="sm" />
          </span>
        )}
        {row?.hidden === true && (
          <span className="ow-viewer__mark" role="img" aria-label={t.hidden}>
            <Icon name="eye-off" size="sm" />
          </span>
        )}
        {row !== undefined && row.edit_plan !== null && (
          <span className={`ow-viewer__badge ow-viewer__badge--${editTarget}`}>
            {editTarget === "file" ? t.editedFile : t.editedAlbum}
          </span>
        )}
        {index !== null && count > 0 && <span className="ow-viewer__position">{t.position(index, count)}</span>}
        <span className="ow-viewer__hint">{t.keyHint}</span>
        <IconButton className="ow-viewer__close" icon="x" label={t.close} shortcut={t.closeShortcut} onClick={closeViewer} />
      </div>
      {details && own !== null && <DetailsBox detail={own} />}
    </div>
  );
}

/** The read-only details box (I). */
function DetailsBox({ detail }: { detail: PhotoDetail }) {
  const rows: [string, string][] = [[t.taken, formatTaken(detail.date.effective)]];
  if (detail.camera.camera !== null) rows.push([t.camera, detail.camera.camera]);
  rows.push([t.size, t.sizeValue(detail.width, detail.height, formatSize(detail.size))]);
  if (detail.kind === "video" && detail.duration_ms !== null) rows.push([t.length, formatLength(detail.duration_ms)]);
  if (detail.people.length > 0) rows.push([t.people, detail.people.join(", ")]);
  if (detail.description !== null && detail.description !== "") rows.push([t.description, detail.description]);
  return (
    <section className="ow-viewer__details" aria-label={t.details}>
      <h2 className="ow-viewer__details-name">{detail.file_name}</h2>
      <dl>
        {rows.map(([label, value]) => (
          <div key={label} className="ow-viewer__fact">
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
