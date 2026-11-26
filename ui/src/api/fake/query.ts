import type { QueryCommands } from "../bindings/query";
import type { AreaHandlers } from "./types";

/** The fake backend's `query_` commands. */
export const queryHandlers: AreaHandlers<QueryCommands> = {};
