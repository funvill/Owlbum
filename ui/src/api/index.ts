// The only way the page reaches Rust. Components and stores call `api()`;
// `main.tsx` installs the Tauri implementation and page tests install the
// fake. Nothing outside src/api/tauri.ts imports @tauri-apps/api.

import type { Api } from "./types";

export type * from "./types";
export { isAppError, toAppError } from "./errors";

let current: Api | undefined;

/** Installs the backend the page talks to. */
export function setApi(api: Api): void {
  current = api;
}

/** The backend the page talks to. */
export function api(): Api {
  if (current === undefined) {
    throw new Error("No backend is installed: call setApi() first.");
  }
  return current;
}
