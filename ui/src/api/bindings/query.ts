// Generated from the Rust command registry (src-tauri/src/bindings.rs). Do not edit;
// run scripts/bindings.sh after changing a command, an event or a type.

import type { Facets } from "./Facets";
import type { Filter } from "./Filter";
import type { Keep } from "./Keep";
import type { Located } from "./Located";
import type { ParsedBox } from "./ParsedBox";
import type { Query } from "./QueryShape";
import type { QueryResult } from "./QueryResult";
import type { Row } from "./Row";

export type QueryCommands = {
  query_facets: { args: { token: string }; result: Facets };
  query_ids: { args: { token: string; offset: number; limit: number }; result: Array<number> };
  query_locate: { args: { token: string; id: number }; result: Located | null };
  query_parse: { args: { text: string; enter: boolean; filter: Filter | null }; result: ParsedBox };
  query_run: { args: { query: Query; keep: Keep | null }; result: QueryResult };
  query_window: { args: { token: string; offset: number; limit: number }; result: Array<Row> };
};

export type QueryEvents = {
};
