import type { SearchCommands } from "../bindings/search";
import type { AreaHandlers } from "./types";

/**
 * The fake backend's `search_` commands: no keywords model is installed, so
 * search by meaning is off. Tests that need matches answer with
 * `QueryFake.meaning`.
 */
export const searchHandlers: AreaHandlers<SearchCommands> = {
  search_meaning: () => ({ state: "off", meaning_start: null }),
};
