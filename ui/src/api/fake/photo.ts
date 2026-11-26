import type { Histogram } from "../bindings/Histogram";
import type { PhotoCommands } from "../bindings/photo";
import type { PhotoDetail } from "../bindings/PhotoDetail";
import type { AreaHandlers } from "./types";

/** Counts per histogram channel, as in Rust. */
export const HISTOGRAM_BINS = 64;

/**
 * A photo's details for page tests: a 4032 × 3024 JPEG named
 * `IMG_<id>.jpg` in `2024/Trip`, taken 12 Jun 2024 at 14:03 near the Eiffel
 * Tower, with full camera details. `changes` replaces any field.
 */
export function fakeDetail(id: number, changes: Partial<PhotoDetail> = {}): PhotoDetail {
  const fileName = `IMG_${String(id).padStart(4, "0")}.jpg`;
  return {
    id,
    rel_path: `2024/Trip/${fileName}`,
    rel_dir: "2024/Trip",
    file_name: fileName,
    abs_path: `D:\\Photos\\2024\\Trip\\${fileName}`,
    kind: "image",
    format: "jpeg",
    width: 4032,
    height: 3024,
    size: 3_250_585,
    duration_ms: null,
    thumb_key: `tk${id}`,
    date: { effective: "2024-06-12T14:03:00", camera: "2024-06-12T14:03:00", typed: null },
    place: {
      lat: 48.85837,
      lon: 2.29448,
      gps: { lat: 48.85837, lon: 2.29448 },
      typed: false,
      name: null,
      country: "France",
    },
    camera: {
      camera: "Owlbum Fixture One",
      lens: "Fixture 24-70mm F2.8",
      focal_mm: 35,
      focal35_mm: 52,
      aperture: 5.6,
      shutter_s: 0.004,
      iso: 100,
      bias_ev: -0.7,
      flash: false,
    },
    clipping: { shadows: 0.004, highlights: 0.023 },
    star: false,
    hidden: false,
    people: [],
    description: null,
    notes: null,
    revision: 1,
    edit_plan: null,
    tags: null,
    ...changes,
  };
}

/** A histogram for page tests: a gentle hill in every channel. */
export function fakeHistogram(): Histogram {
  const hill = Array.from({ length: HISTOGRAM_BINS }, (_, i) => 10 + Math.round(90 * Math.sin((Math.PI * i) / 63)));
  return { l: hill, r: [...hill], g: [...hill], b: [...hill] };
}

/** The fake backend's `photo_` commands. */
export const photoHandlers: AreaHandlers<PhotoCommands> = {
  photo_detail: ({ id }) => fakeDetail(id),
  photo_histogram: () => fakeHistogram(),
  photo_set_info: ({ ids }) => ({ changed: ids, revision: 2 }),
};
