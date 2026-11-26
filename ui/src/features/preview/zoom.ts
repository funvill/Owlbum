// The zoom rules shared by the preview and the viewer (docs/04-browsing.md,
// "Zoom and pan", "Full-size swap", "Mini map"). Pure functions: the
// component measures and draws; this file only works out where things go.

/** A width and a height, in CSS pixels unless said otherwise. */
export interface Box {
  width: number;
  height: number;
}

/**
 * Where the picture is: laid out at its fitted size and placed with
 * `translate(x, y) scale(zoom)` from its top-left corner.
 */
export interface View {
  zoom: number;
  x: number;
  y: number;
}

/** Fitted. */
export const MIN_ZOOM = 1;
/** The smallest the largest zoom can be. */
export const MAX_ZOOM_FLOOR = 8;
/** Zoom per wheel notch (and per `+` or `-` in the viewer). */
export const WHEEL_STEP = 1.2;
/** The medium thumbnail's longest side (docs/03-media-and-scanning.md). */
export const MEDIUM_SIDE = 1600;
/** The mini map's long side. */
export const MINI_MAP_SIDE = 160;

/** The largest size with the picture's shape that fits wholly in the stage; 0 × 0 if any input is 0 or less. */
export function fittedSize(stage: Box, picture: Box): Box {
  if (stage.width <= 0 || stage.height <= 0 || picture.width <= 0 || picture.height <= 0) {
    return { width: 0, height: 0 };
  }
  const scale = Math.min(stage.width / picture.width, stage.height / picture.height);
  return { width: picture.width * scale, height: picture.height * scale };
}

/** Zoom 1, centred. */
export function fitView(stage: Box, fitted: Box): View {
  return { zoom: MIN_ZOOM, x: (stage.width - fitted.width) / 2, y: (stage.height - fitted.height) / 2 };
}

function clampAxis(at: number, stageSide: number, size: number): number {
  if (size <= stageSide) return (stageSide - size) / 2;
  return Math.min(0, Math.max(stageSide - size, at));
}

/** Centres an axis where the scaled picture fits, else keeps it covering the stage: no gap at an edge. */
export function clampPan(view: View, stage: Box, fitted: Box): View {
  return {
    zoom: view.zoom,
    x: clampAxis(view.x, stage.width, fitted.width * view.zoom),
    y: clampAxis(view.y, stage.height, fitted.height * view.zoom),
  };
}

/**
 * Zooms by `factor` about a stage point, keeping the picture point under it
 * fixed. Back at 1 it is the fit view.
 */
export function zoomAt(
  view: View,
  factor: number,
  at: { x: number; y: number },
  stage: Box,
  fitted: Box,
  max: number,
): View {
  const zoom = Math.min(max, Math.max(MIN_ZOOM, view.zoom * factor));
  if (zoom === MIN_ZOOM) return fitView(stage, fitted);
  const px = (at.x - view.x) / view.zoom;
  const py = (at.y - view.y) / view.zoom;
  return clampPan({ zoom, x: at.x - px * zoom, y: at.y - py * zoom }, stage, fitted);
}

/** The larger of 8 and the zoom at which one pixel of the original is 2 screen pixels. */
export function maxZoom(fittedWidth: number, originalWidth: number, devicePixelRatio: number): number {
  if (fittedWidth <= 0 || originalWidth <= 0 || devicePixelRatio <= 0) return MAX_ZOOM_FLOOR;
  return Math.max(MAX_ZOOM_FLOOR, (2 * originalWidth) / (fittedWidth * devicePixelRatio));
}

/** The medium thumbnail's size for an original: fitted inside 1600 px, never enlarged. */
export function mediumSize(original: Box): Box {
  const long = Math.max(original.width, original.height);
  if (long <= MEDIUM_SIDE) return { width: original.width, height: original.height };
  const scale = MEDIUM_SIDE / long;
  return { width: Math.round(original.width * scale), height: Math.round(original.height * scale) };
}

/**
 * Whether the full-size picture is wanted: the picture is drawn larger than
 * the medium one has pixels. Never for an original no larger than the
 * medium picture, or one whose size is not known.
 */
export function needsFull(fittedWidth: number, zoom: number, devicePixelRatio: number, original: Box): boolean {
  if (Math.max(original.width, original.height) <= MEDIUM_SIDE) return false;
  return fittedWidth * zoom * devicePixelRatio > mediumSize(original).width;
}

/** The mini map's size: a long side of 160 px and the picture's shape. */
export function miniMapSize(fitted: Box): Box {
  if (fitted.width <= 0 || fitted.height <= 0) return { width: 0, height: 0 };
  const scale = MINI_MAP_SIDE / Math.max(fitted.width, fitted.height);
  return { width: Math.round(fitted.width * scale), height: Math.round(fitted.height * scale) };
}

const unit = (n: number) => Math.min(1, Math.max(0, n));

/** The visible part of the picture, as fractions of its width and height. */
export function visibleRect(
  view: View,
  stage: Box,
  fitted: Box,
): { left: number; top: number; width: number; height: number } {
  const w = fitted.width * view.zoom;
  const h = fitted.height * view.zoom;
  if (w <= 0 || h <= 0) return { left: 0, top: 0, width: 1, height: 1 };
  const left = unit(-view.x / w);
  const top = unit(-view.y / h);
  const right = unit((stage.width - view.x) / w);
  const bottom = unit((stage.height - view.y) / h);
  return { left, top, width: right - left, height: bottom - top };
}

/** Centres the view on a point of the picture given as fractions, then clamps. */
export function centreOn(view: View, point: { u: number; v: number }, stage: Box, fitted: Box): View {
  return clampPan(
    {
      zoom: view.zoom,
      x: stage.width / 2 - point.u * fitted.width * view.zoom,
      y: stage.height / 2 - point.v * fitted.height * view.zoom,
    },
    stage,
    fitted,
  );
}
