// The preview pane (docs/04-browsing.md, "Preview pane"): the current
// photo on a stage that zooms and pans, and its caption.
//
// It reads the page's one current photo from the grid store, like the
// details panel, so the two never disagree. Which picture is shown follows
// "Which picture is shown": the medium picture at once when the image cache
// has it decoded, else this photo's small thumbnail with the medium one
// swapped in when it is decoded. The preview wants the photo and its
// preload window from the cache on every step ("Image cache and
// preloading").

import type { MouseEvent as ReactMouseEvent, Ref } from "react";
import type { PhotoAt } from "../../api/bindings/PhotoAt";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import { previewText as t } from "../../text/preview";
import { useSettingsStore } from "../app";
import { useGridStore } from "../grid";
import { FlagMark, ScoreToast } from "../organise";
import { Caption } from "./Caption";
import { stagePictures } from "./pictures";
import { useCachedReady, usePreload } from "./preload";
import { useShownRow } from "./shownRow";
import { ZoomStage, type ZoomHandle, type ZoomStageProps } from "./ZoomStage";
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
  const { current, pending, row } = useShownRow();
  const selected = useGridStore((s) => s.selection.length);
  const editTarget = useSettingsStore((s) => s.settings?.edit_target ?? "album");
  usePreload(pending?.index ?? current?.index ?? null);

  if (current === null && pending === null) {
    return (
      <section className="ow-preview" aria-label={t.region}>
        <div className="ow-preview__stage">
          <p className="ow-preview__hint">{t.hint}</p>
        </div>
      </section>
    );
  }

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
        <CachedZoomStage
          key={`${row.id}:${row.thumb_key}:${row.edit_plan?.key ?? ""}`}
          ready={row.thumb_key !== ""}
          pictures={stagePictures(row)}
          aspect={row.aspect}
          plan={row.edit_plan}
          original={own !== null && own.width > 0 && own.height > 0 ? { width: own.width, height: own.height } : null}
          zoomable={row.kind !== "video"}
          onOpenAtFit={onOpen === undefined ? undefined : () => onOpen(at)}
          onMenu={onMenu === undefined ? undefined : (e) => onMenu(at, e)}
          handle={zoom}
        />
        <FlagMark place="preview" />
        <ScoreToast place="preview" />
      </div>
      <Caption row={row} selected={selected} fullPath={own?.abs_path ?? null} editTarget={editTarget} />
    </section>
  );
}

/** The zoom component, showing the medium picture at once when the image cache has it decoded. */
export function CachedZoomStage(props: Omit<ZoomStageProps, "mediumCached">) {
  const cached = useCachedReady(props.pictures.medium);
  return <ZoomStage {...props} mediumCached={cached} />;
}
