// The page's errors go to Owlbum's log through `app_log` (docs/08-desktop-and-release.md,
// "Logs"): an uncaught error or a rejected promise nobody handled. Rust
// limits the length and the rate; nothing leaves the computer.

import { api } from "../../api";

/** The words of an error, with its stack when it has one. */
export function describe(e: unknown): string {
  if (e instanceof Error) return e.stack ?? `${e.name}: ${e.message}`;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/** Sends one line to the log; a failure to log is ignored. */
export function logToShell(level: "error" | "warn" | "info", message: string): void {
  void api()
    .invoke("app_log", { level, message })
    .catch(() => {});
}

/** Listens for the page's uncaught errors. Answers the way to stop. */
export function followPageErrors(target: Window = window): () => void {
  const onError = (e: ErrorEvent) => logToShell("error", describe(e.error ?? e.message));
  const onRejection = (e: PromiseRejectionEvent) => logToShell("error", `unhandled rejection: ${describe(e.reason)}`);
  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onRejection);
  return () => {
    target.removeEventListener("error", onError);
    target.removeEventListener("unhandledrejection", onRejection);
  };
}
