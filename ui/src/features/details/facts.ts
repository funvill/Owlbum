// The details panel's text for each fact (docs/04-browsing.md, "Facts",
// "Camera rows", "Place rows" and "Histogram"). Pure functions, so every
// rule of the spec has a test.

import type { Camera } from "../../api/bindings/Camera";
import type { Clipping } from "../../api/bindings/Clipping";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import { detailsText as t } from "../../text/details";

/** One labelled row. */
export interface FactRow {
  label: string;
  value: string;
}

const KB = 1024;
const MB = 1024 * KB;
const GB = 1024 * MB;

/** The minus sign used for negative values. */
const MINUS = "−";

/** A number to at most one decimal, without a trailing ".0". */
function oneDecimal(n: number): string {
  const s = n.toFixed(1);
  return s.endsWith(".0") ? s.slice(0, -2) : s;
}

/**
 * "12 Jun 2024, 14:03" from `YYYY-MM-DDTHH:MM:SS`, read from the written
 * digits and never passed through the computer's time zone. "Unknown" when
 * there is no date.
 */
export function formatTaken(date: string | null): string {
  const m = date === null ? null : /^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})/.exec(date);
  const month = m === null ? undefined : t.months[Number(m[2]) - 1];
  if (m === null || month === undefined) return t.unknown;
  return `${Number(m[3])} ${month} ${m[1]}, ${m[4]}:${m[5]}`;
}

/** "x.x GB" from 1 GB, "x.x MB" from 1 MB, else "N KB" with a minimum of 1. */
export function formatSize(bytes: number): string {
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
  if (bytes >= MB) return `${(bytes / MB).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / KB))} KB`;
}

const RATIOS: [string, number][] = [
  ["1:1", 1],
  ["5:4", 5 / 4],
  ["4:3", 4 / 3],
  ["3:2", 3 / 2],
  ["16:9", 16 / 9],
  ["2:1", 2],
];

/** The long side over the short: the first name within 2%, else "r.rr:1". */
export function ratioName(width: number, height: number): string {
  const r = Math.max(width, height) / Math.min(width, height);
  const named = RATIOS.find(([, v]) => Math.abs(r / v - 1) <= 0.02);
  return named?.[0] ?? `${r.toFixed(2)}:1`;
}

/** "Square" when the sides differ by less than 1%, else "Landscape" or "Portrait". */
export function shapeName(width: number, height: number): string {
  if (Math.abs(width - height) < 0.01 * Math.max(width, height)) return t.square;
  return width > height ? t.landscape : t.portrait;
}

/** `m:ss` under an hour, `h:mm:ss` from an hour, rounded down to whole seconds. */
export function formatLength(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** The Photo section's rows, in order. Taken's "changed" badge is drawn by the panel. */
export function factRows(d: PhotoDetail): FactRow[] {
  const known = d.width > 0 && d.height > 0;
  const rows: FactRow[] = [
    { label: t.taken, value: formatTaken(d.date.effective) },
    {
      label: t.size,
      value: known ? `${d.width} × ${d.height} px · ${formatSize(d.size)}` : formatSize(d.size),
    },
  ];
  if (d.kind === "video" && d.duration_ms !== null) {
    rows.push({ label: t.length, value: formatLength(d.duration_ms) });
  }
  if (known) {
    const mp = ((d.width * d.height) / 1_000_000).toFixed(1);
    rows.push({ label: t.resolution, value: `${mp} MP · ${ratioName(d.width, d.height)}` });
    rows.push({ label: t.shape, value: shapeName(d.width, d.height) });
  }
  rows.push({ label: t.format, value: d.format.toUpperCase() });
  if (d.rel_dir !== "") rows.push({ label: t.folder, value: d.rel_dir });
  return rows;
}

/** "1/N s" below one second, else "N s". */
export function formatShutter(seconds: number): string {
  if (seconds < 1) return `1/${Math.round(1 / seconds)} s`;
  return `${oneDecimal(seconds)} s`;
}

/** "+0.7 EV", "−1 EV" or "0 EV". */
function formatExposure(ev: number): string {
  const rounded = Math.round(ev * 10) / 10;
  if (rounded === 0) return "0 EV";
  return `${rounded > 0 ? "+" : MINUS}${oneDecimal(Math.abs(rounded))} EV`;
}

/** The Camera section's rows: only those whose value is known. */
export function cameraRows(c: Camera): FactRow[] {
  const rows: FactRow[] = [];
  const add = (label: string, value: string | null) => {
    if (value !== null) rows.push({ label, value });
  };
  add(t.camera, c.camera);
  add(t.lens, c.lens);
  if (c.focal_mm !== null) {
    const mm = oneDecimal(c.focal_mm);
    const in35 =
      c.focal35_mm !== null && Math.abs(c.focal35_mm - c.focal_mm) >= 1 ? t.in35mm(oneDecimal(c.focal35_mm)) : "";
    add(t.focalLength, `${mm} mm${in35}`);
  }
  add(t.aperture, c.aperture === null ? null : `f/${oneDecimal(c.aperture)}`);
  add(t.shutter, c.shutter_s === null ? null : formatShutter(c.shutter_s));
  add(t.iso, c.iso === null ? null : String(c.iso));
  add(t.exposure, c.bias_ev === null ? null : formatExposure(c.bias_ev));
  add(t.flash, c.flash === null ? null : c.flash ? t.flashFired : t.flashNotFired);
  return rows;
}

/** Latitude then longitude, each to 5 decimals. */
export function formatCoordinates(lat: number, lon: number): string {
  return `${lat.toFixed(5)}, ${lon.toFixed(5)}`;
}

/** The clipping line's two parts; `high` marks 2.0 % or more. */
export function clippingParts(c: Clipping): { text: string; high: boolean }[] {
  const part = (fraction: number, text: (pct: string) => string) => {
    const pct = (fraction * 100).toFixed(1);
    return { text: text(pct), high: Number(pct) >= 2 };
  };
  return [part(c.shadows, t.shadowsClipped), part(c.highlights, t.highlightsClipped)];
}
