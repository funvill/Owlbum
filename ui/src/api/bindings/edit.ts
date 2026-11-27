// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { EditInput } from "./EditInput";
import type { EditPlan } from "./EditPlan";
import type { EditSet } from "./EditSet";
import type { PhotoEdit } from "./PhotoEdit";
import type { Reverted } from "./Reverted";
import type { TurnOp } from "./TurnOp";
import type { Turned } from "./Turned";

export type EditCommands = {
  edit_plan: { args: { photo_id: number; edit: EditInput; whole: boolean }; result: EditPlan };
  edit_revert: { args: { photo_ids: Array<number> }; result: Reverted };
  edit_set: { args: { photo_id: number; edit: EditInput | null }; result: EditSet };
  edit_transform: { args: { edit: EditInput; op: TurnOp }; result: PhotoEdit };
  edit_turn: { args: { photo_ids: Array<number>; op: TurnOp }; result: Turned };
};

export type EditEvents = {
};
