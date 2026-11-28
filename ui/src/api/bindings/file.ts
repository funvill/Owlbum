// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { CopiedFiles } from "./CopiedFiles";
import type { CopiedImage } from "./CopiedImage";
import type { CopiedPaths } from "./CopiedPaths";
import type { Dragged } from "./Dragged";
import type { TrashQuestion } from "./TrashQuestion";
import type { Trashed } from "./Trashed";

export type FileCommands = {
  file_copy_files: { args: { photo_ids: Array<number> }; result: CopiedFiles };
  file_copy_image: { args: { photo_id: number }; result: CopiedImage };
  file_copy_paths: { args: { photo_ids: Array<number> }; result: CopiedPaths };
  file_copy_text: { args: { content: string }; result: null };
  file_prepare_drag: { args: { photo_ids: Array<number> }; result: null };
  file_reveal: { args: { photo_id: number }; result: null };
  file_start_drag: { args: { photo_ids: Array<number> }; result: Dragged };
  file_trash: { args: { ticket: number }; result: Trashed };
  file_trash_ask: { args: { photo_ids: Array<number> }; result: TrashQuestion | null };
};

export type FileEvents = {
};
