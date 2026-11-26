import type { SearchCommands } from "../bindings/search";
import type { AreaHandlers } from "./types";

/** The fake backend's `search_` commands. */
export const searchHandlers: AreaHandlers<SearchCommands> = {};
