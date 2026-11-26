import type { FileCommands } from "../bindings/file";
import type { AreaHandlers } from "./types";

/** The fake backend's `file_` commands. */
export const fileHandlers: AreaHandlers<FileCommands> = {};
