// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { Colour } from "./Colour";
import type { EditInput } from "./EditInput";
import type { EditPlan } from "./EditPlan";
import type { EditSet } from "./EditSet";
import type { EditStatus } from "./EditStatus";
import type { PhotoEdit } from "./PhotoEdit";
import type { Reverted } from "./Reverted";
import type { SaveProgress } from "./SaveProgress";
import type { SavedAs } from "./SavedAs";
import type { SavedEdits } from "./SavedEdits";
import type { TurnOp } from "./TurnOp";
import type { Turned } from "./Turned";

export type EditCommands = {
  edit_auto: { args: { photo_id: number }; result: Colour };
  edit_plan: { args: { photo_id: number; edit: EditInput; whole: boolean }; result: EditPlan };
  edit_revert: { args: { photo_ids: Array<number> }; result: Reverted };
  edit_save_all: { args: Record<string, never>; result: SavedEdits };
  edit_save_as: { args: { photo_id: number }; result: SavedAs };
  edit_save_to_file: { args: { photo_ids: Array<number> }; result: SavedEdits };
  edit_set: { args: { photo_id: number; edit: EditInput | null }; result: EditSet };
  edit_status: { args: Record<string, never>; result: EditStatus };
  edit_transform: { args: { edit: EditInput; op: TurnOp }; result: PhotoEdit };
  edit_turn: { args: { photo_ids: Array<number>; op: TurnOp }; result: Turned };
};

export type EditEvents = {
  edit_save_progress: SaveProgress;
};
