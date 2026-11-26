// The preview pane (docs/04-browsing.md, "Preview pane"): the current
// photo on a stage that zooms and pans, and its caption.
//
// It reads the page's one current photo from the grid store, like the
// details panel, so the two never disagree. Which picture is shown follows
// "Which picture is shown": this photo's small thumbnail at once, the
// medium one swapped in when it is decoded. The decoded-image cache and
// preloading (#21) replace that order with a cache lookup; the stage takes
// its pictures as addresses, so the cache only changes what is passed in.

import { useCallback, useState, useSyncExternalStore, type MouseEvent as ReactMouseEvent, type Ref } from "react";
import { fullUrl, thumbUrl } from "../../api";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import type { Row } from "../../api/bindings/Row";
import { previewText as t } from "../../text/preview";
import { useSettingsStore } from "../app";
import { gridBlocks, useGridStore } from "../grid";
import { Caption } from "./Caption";
import { ZoomStage, type ZoomHandle } from "./ZoomStage";
import "./Preview.css";

export interface PreviewProps {
  /** The current photo's details, from the details panel's read; used only when they are this photo's. */
  detail: PhotoDetail | null;
  /** Double-click at fit: open the viewer on the current photo (#22). */
  onOpen?: (at: PhotoAt) => void;
  /** Right-click on the stage: the photo menu (#38). */
  onMenu?: (at: PhotoAt, event: ReactMouseEvent) => void;
  /** The zoom component's handle, for keys. */
  zoom?: Ref<ZoomHandle>;
}

export function Preview({ detail, onOpen, onMenu, zoom }: PreviewProps) {
  const current = useGridStore((s) => s.current);
  const pending = useGridStore((s) => s.pending);
  const selected = useGridStore((s) => s.selection.length);
  const editTarget = useSettingsStore((s) => s.settings?.edit_target ?? "album");
  const blocks = gridBlocks();
  useSyncExternalStore(
    useCallback((listener: () => void) => gridBlocks().subscribe(listener), []),
    () => gridBlocks().version,
  );
  // The last row seen for the current photo: a re-run that moves it, or a
  // scroll that lets its block go, must not blank the preview.
  const [held, setHeld] = useState<Row | null>(null);

  if (current === null && pending === null) {
    return (
      <section className="ow-preview" aria-label={t.region}>
        <div className="ow-preview__stage">
          <p className="ow-preview__hint">{t.hint}</p>
        </div>
      </section>
    );
  }

  // Only a row that is this photo's: the index may be an old result's. A
  // step waiting for its row shows a placeholder, never the photo it left.
  const candidate = current === null || pending !== null ? undefined : blocks.row(current.index);
  const found = candidate !== undefined && candidate.id === current?.id ? candidate : undefined;
  if (found !== undefined && found !== held) setHeld(found);
  const row = found ?? (pending === null && held !== null && held.id === current?.id ? held : undefined);
  if (current === null || row === undefined) {
    return (
      <section className="ow-preview" aria-label={t.region} aria-busy="true">
        <div className="ow-preview__stage" />
      </section>
    );
  }

  const own = detail !== null && detail.id === row.id ? detail : null;
  const at = current;
  return (
    <section className="ow-preview" aria-label={t.region}>
      <div className="ow-preview__stage">
        <ZoomStage
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
          onOpenAtFit={onOpen === undefined ? undefined : () => onOpen(at)}
          onMenu={onMenu === undefined ? undefined : (e) => onMenu(at, e)}
          handle={zoom}
        />
      </div>
      <Caption row={row} selected={selected} fullPath={own?.abs_path ?? null} editTarget={editTarget} />
    </section>
  );
}
