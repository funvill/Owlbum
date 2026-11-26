// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { Keep } from "./Keep";
import type { Located } from "./Located";
import type { Query } from "./QueryShape";
import type { QueryResult } from "./QueryResult";
import type { Row } from "./Row";

export type QueryCommands = {
  query_locate: { args: { token: string; id: number }; result: Located | null };
  query_run: { args: { query: Query; keep: Keep | null }; result: QueryResult };
  query_window: { args: { token: string; offset: number; limit: number }; result: Array<Row> };
};

export type QueryEvents = {
};
