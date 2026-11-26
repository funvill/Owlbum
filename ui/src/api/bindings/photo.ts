// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { Histogram } from "./Histogram";
import type { InfoChanged } from "./InfoChanged";
import type { InfoPatch } from "./InfoPatch";
import type { PhotoDetail } from "./PhotoDetail";

export type PhotoCommands = {
  photo_detail: { args: { id: number }; result: PhotoDetail };
  photo_histogram: { args: { id: number }; result: Histogram };
  photo_set_info: { args: { ids: Array<number>; patch: InfoPatch }; result: InfoChanged };
};

export type PhotoEvents = {
};
