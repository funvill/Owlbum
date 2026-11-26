import type { AppCommands } from "../bindings/app";
import type { AreaHandlers } from "./types";

/** The fake backend's `app_` commands. */
export const appHandlers: AreaHandlers<AppCommands> = {
  app_version: () => ({ version: "0.0.0-fake" }),
};
