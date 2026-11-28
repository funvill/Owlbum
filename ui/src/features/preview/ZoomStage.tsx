// The zoom component the preview and the viewer share (docs/04-browsing.md,
// "Which picture is shown", "Zoom and pan", "Full-size swap", "Mini map").
//
// The parent gives it a `key` made of the photo's id and thumbnail key, so a
// new photo (or a new edit) is a new stage: it starts fitted and can never
// show the previous photo's picture, not even for one frame.

import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type Ref,
} from "react";
import type { EditPlan } from "../../api/bindings/EditPlan";
import type { KeyId } from "../../keys";
import { previewText } from "../../text/preview";
import { editedStyles } from "../edit/EditedImage";
import { useColourFilter } from "../edit/filters";
import { MiniMap } from "./MiniMap";
import {
  WHEEL_STEP,
  centreOn,
  clampPan,
  fitView,
  fittedSize,
  maxZoom,
  needsFull,
  zoomAt,
  type Box,
  type View,
} from "./zoom";
import "./ZoomStage.css";

/** What the viewer (#22) drives from its keys. */
export interface ZoomHandle {
  /** Zooms by a factor about the centre of the stage. */
  zoomBy(factor: number): void;
  /** Back to fit. */
  fit(): void;
  /** Whether the picture is zoomed in (Esc goes back to fit first in the viewer). */
  zoomed(): boolean;
}

/** The pictures of one photo, from smallest to largest. */
export interface StagePictures {
  /** The small (512 px) thumbnail: shown at once, and in the mini map. */
  small: string;
  /** The medium (1600 px) thumbnail: swapped in when it is decoded. */
  medium: string;
  /** The full-size picture, asked for only past the medium picture's pixels. */
  full: string;
  /**
   * An animated picture's original file: the webview plays it. It takes
   * the place of the medium and full-size pictures and zooms as it is.
   */
  animated?: string | null;
}

export interface ZoomStageProps {
  pictures: StagePictures;
  /** The picture's shape (upright width / height); the layout never follows a loaded picture's own size. */
  aspect: number;
  /**
   * The plan of the photo's stored edit (docs/06-editing-and-file-actions.md,
   * "How the page draws a plan"): every picture is drawn through it, and the
   * stage takes the edited shape. Null or absent for no edit.
   */
  plan?: EditPlan | null;
  /**
   * Before/after (docs/06-editing-and-file-actions.md, "Before and after"):
   * the plain upright picture, with no plan at all, captioned "Original".
   */
  showOriginal?: boolean;
  /** The original's upright size in pixels, once known; until then the largest zoom is 8 and no full picture is asked for. */
  original: Box | null;
  /** False while the photo has no thumbnail yet: the stage stays empty. */
  ready?: boolean;
  /** False for a video: no zoom, pan, mini map or full-size swap. */
  zoomable: boolean;
  /**
   * The medium picture is already decoded in the image cache: it shows at
   * once, and when it was so from the start the small one is not loaded.
   */
  mediumCached?: boolean;
  /** Double-click at fit: the preview opens the viewer. */
  onOpenAtFit?: () => void;
  /**
   * Double-click at fit zooms to actual size (one picture pixel to one
   * screen pixel) at the pointer instead: the viewer. Needs `original`.
   */
  actualSizeAtFit?: boolean;
  /** Right-click: the photo menu (#38). */
  onMenu?: (event: ReactMouseEvent) => void;
  /**
   * A primary-button press at fit, where there is nothing to pan: the
   * preview starts a drag out from it (docs/06, "Dragging photos out").
   */
  onPressAtFit?: (event: ReactPointerEvent) => void;
  handle?: Ref<ZoomHandle>;
}

/** The rows of the key table the zoom component serves: the viewer's (#22), which wires its own listener. */
export const ZOOM_KEY_IDS = ["viewer.zoom_in", "viewer.zoom_out", "viewer.fit"] as const satisfies readonly KeyId[];

export type ZoomKeyId = (typeof ZOOM_KEY_IDS)[number];

/** What each zoom row does: `+` and `-` zoom by 1.2 about the centre, `0` goes back to fit. */
export function zoomKeyHandlers(handle: ZoomHandle): Record<ZoomKeyId, () => void> {
  return {
    "viewer.zoom_in": () => handle.zoomBy(WHEEL_STEP),
    "viewer.zoom_out": () => handle.zoomBy(1 / WHEEL_STEP),
    "viewer.fit": () => handle.fit(),
  };
}

const round = (n: number, places: number) => Number(n.toFixed(places));

function dpr(): number {
  const r = globalThis.devicePixelRatio;
  return typeof r === "number" && r > 0 ? r : 1;
}

export function ZoomStage({
  pictures,
  ready = true,
  aspect: photoAspect,
  plan: storedPlan = null,
  showOriginal = false,
  original: photoOriginal,
  zoomable,
  mediumCached = false,
  onOpenAtFit,
  actualSizeAtFit = false,
  onMenu,
  onPressAtFit,
  handle,
}: ZoomStageProps) {
  const stageRef = useRef<HTMLDivElement>(null);
  const [stage, setStage] = useState<Box>({ width: 0, height: 0 });
  /** Null is the fit view, which follows the stage's size. */
  const [view, setView] = useState<View | null>(null);
  const [mediumLoaded, setMediumLoaded] = useState(false);
  const [cachedFromStart] = useState(mediumCached);
  const mediumReady = mediumLoaded || mediumCached;
  const [fullWanted, setFullWanted] = useState(false);
  const [fullReady, setFullReady] = useState(false);
  const [animatedReady, setAnimatedReady] = useState(false);
  const animated = pictures.animated ?? null;
  const [grabbing, setGrabbing] = useState(false);

  const plan = showOriginal ? null : storedPlan;
  // Showing the original changes the shape: back to fit.
  const [shownOriginal, setShownOriginal] = useState(showOriginal);
  if (shownOriginal !== showOriginal) {
    setShownOriginal(showOriginal);
    setView(null);
  }

  // An edit gives the stage the edited picture's shape and size.
  const aspect = plan === null ? photoAspect : plan.out.w / plan.out.h;
  const original = plan === null || photoOriginal === null ? photoOriginal : { width: plan.out.w, height: plan.out.h };
  const filterValues = plan?.colour?.svg.map((v) => String(v)).join(" ") ?? null;
  useColourFilter(filterValues === null ? null : (plan?.key ?? null), filterValues);

  const fitted = fittedSize(stage, { width: aspect > 0 ? aspect : 1, height: 1 });
  const shown = view ?? fitView(stage, fitted);
  const max = maxZoom(fitted.width, original?.width ?? 0, dpr());

  // The latest values, for the native wheel listener and the handle.
  const live = useRef({ stage, fitted, shown, max, zoomable });
  useLayoutEffect(() => {
    live.current = { stage, fitted, shown, max, zoomable };
  });

  // The stage's size; any change goes back to fit.
  useLayoutEffect(() => {
    const el = stageRef.current;
    if (el === null) return;
    let last = { width: -1, height: -1 };
    const measure = () => {
      if (last.width === el.clientWidth && last.height === el.clientHeight) return;
      last = { width: el.clientWidth, height: el.clientHeight };
      setStage(last);
      setView(null);
    };
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const zoomBy = useCallback((factor: number, at?: { x: number; y: number }) => {
    const v = live.current;
    if (!v.zoomable || v.fitted.width <= 0) return;
    const point = at ?? { x: v.stage.width / 2, y: v.stage.height / 2 };
    const next = zoomAt(v.shown, factor, point, v.stage, v.fitted, v.max);
    setView(next.zoom === 1 ? null : next);
  }, []);

  useImperativeHandle(
    handle,
    () => ({
      zoomBy: (factor: number) => zoomBy(factor),
      fit: () => setView(null),
      zoomed: () => live.current.shown.zoom > 1,
    }),
    [zoomBy],
  );

  // The wheel never scrolls the page, so it needs a listener that is not passive.
  useEffect(() => {
    const el = stageRef.current;
    if (el === null) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.deltaY === 0) return;
      const rect = el.getBoundingClientRect();
      zoomBy(e.deltaY < 0 ? WHEEL_STEP : 1 / WHEEL_STEP, { x: e.clientX - rect.left, y: e.clientY - rect.top });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomBy]);

  // Ask for the full-size picture once it is drawn past the medium picture's pixels; keep it for this photo.
  const wantFull =
    zoomable && animated === null && original !== null && needsFull(fitted.width, shown.zoom, dpr(), original);
  if (wantFull && !fullWanted) setFullWanted(true);

  const drag = useRef<{ id: number; x: number; y: number; from: View } | null>(null);
  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    if (shown.zoom <= 1) {
      onPressAtFit?.(e);
      return;
    }
    drag.current = { id: e.pointerId, x: e.clientX, y: e.clientY, from: shown };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setGrabbing(true);
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (d === null || d.id !== e.pointerId) return;
    setView(clampPan({ zoom: d.from.zoom, x: d.from.x + e.clientX - d.x, y: d.from.y + e.clientY - d.y }, stage, fitted));
  };
  const endDrag = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (drag.current === null || drag.current.id !== e.pointerId) return;
    drag.current = null;
    setGrabbing(false);
  };

  const onDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    if (shown.zoom > 1) {
      setView(null);
      return;
    }
    if (!actualSizeAtFit) {
      onOpenAtFit?.();
      return;
    }
    if (original === null || fitted.width <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    zoomBy(original.width / (fitted.width * dpr()), { x: e.clientX - rect.left, y: e.clientY - rect.top });
  };

  const classes = ["ow-zoom"];
  if (zoomable && shown.zoom > 1) classes.push("ow-zoom--grab");
  if (grabbing) classes.push("ow-zoom--grabbing");
  const known = ready && fitted.width > 0;
  const imgStyle = plan === null || !known ? undefined : editedStyles(plan, fitted.width, fitted.height, "contain").img;

  return (
    <div
      ref={stageRef}
      className={classes.join(" ")}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={endDrag}
      onDoubleClick={onDoubleClick}
      onContextMenu={onMenu}
    >
      <div
        className="ow-zoom__picture"
        hidden={!known}
        style={{
          width: fitted.width,
          height: fitted.height,
          transform: `translate(${round(shown.x, 3)}px, ${round(shown.y, 3)}px) scale(${round(shown.zoom, 4)})`,
          ...(plan === null ? {} : { overflow: "hidden" }),
        }}
        data-edit-key={plan?.key}
      >
        {ready && (!cachedFromStart || animated !== null) && (
          <img
            className="ow-zoom__img ow-zoom__img--ready"
            src={pictures.small}
            alt=""
            draggable={false}
            style={imgStyle}
          />
        )}
        {ready && animated !== null && (
          <img
            className={`ow-zoom__img ow-zoom__img--animated${animatedReady ? " ow-zoom__img--ready" : ""}`}
            src={animated}
            alt=""
            draggable={false}
            style={imgStyle}
            onLoad={() => setAnimatedReady(true)}
          />
        )}
        {ready && animated === null && (
          <img
            className={`ow-zoom__img${mediumReady ? " ow-zoom__img--ready" : ""}`}
            src={pictures.medium}
            alt=""
            draggable={false}
            style={imgStyle}
            onLoad={() => setMediumLoaded(true)}
          />
        )}
        {ready && fullWanted && (
          <img
            className={`ow-zoom__img${fullReady ? " ow-zoom__img--ready" : ""}`}
            src={pictures.full}
            alt=""
            draggable={false}
            style={imgStyle}
            onLoad={() => setFullReady(true)}
          />
        )}
      </div>
      {zoomable && known && shown.zoom > 1 && (
        <MiniMap
          src={pictures.small}
          plan={plan}
          fitted={fitted}
          stage={stage}
          view={shown}
          onCentre={(point) => setView(centreOn(live.current.shown, point, live.current.stage, live.current.fitted))}
        />
      )}
      {showOriginal && storedPlan !== null && <span className="ow-zoom__original">{previewText.original}</span>}
    </div>
  );
}
