// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { MeaningResult } from "./MeaningResult";

export type SearchCommands = {
  search_meaning: { args: { token: string }; result: MeaningResult };
};

export type SearchEvents = {
};
