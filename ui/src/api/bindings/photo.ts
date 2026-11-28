// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { ExifField } from "./ExifField";
import type { Histogram } from "./Histogram";
import type { InfoChanged } from "./InfoChanged";
import type { InfoPatch } from "./InfoPatch";
import type { Nudged } from "./Nudged";
import type { PhotoDetail } from "./PhotoDetail";
import type { ScoreBreakdown } from "./ScoreBreakdown";

export type PhotoCommands = {
  photo_detail: { args: { id: number }; result: PhotoDetail };
  photo_exif: { args: { id: number }; result: Array<ExifField> };
  photo_find: { args: { path: string }; result: number | null };
  photo_histogram: { args: { id: number }; result: Histogram };
  photo_nudge: { args: { ids: Array<number>; delta: number }; result: Nudged };
  photo_score: { args: { id: number }; result: ScoreBreakdown };
  photo_set_info: { args: { ids: Array<number>; patch: InfoPatch }; result: InfoChanged };
};

export type PhotoEvents = {
};
