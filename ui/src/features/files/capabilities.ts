// What this platform offers for file actions (`app_capabilities`): the
// page asks Rust once, never the user agent.

import { create } from "zustand";
import { api } from "../../api";
import type { Capabilities } from "../../api/bindings/Capabilities";

interface CapabilityState {
  /** Null until Rust has answered. */
  capabilities: Capabilities | null;
}

export const useCapabilities = create<CapabilityState>(() => ({ capabilities: null }));

let asked: Promise<Capabilities> | null = null;

/** The platform's capabilities, asked once. */
export function loadCapabilities(): Promise<Capabilities> {
  asked ??= api()
    .invoke("app_capabilities")
    .then((capabilities) => {
      useCapabilities.setState({ capabilities });
      return capabilities;
    })
    .catch((): Capabilities => {
      // Nothing is offered that might not work.
      asked = null;
      return { drag_out: false, copy_files: "paths" };
    });
  return asked;
}

/** For tests: ask again. */
export function resetCapabilities(): void {
  asked = null;
  useCapabilities.setState({ capabilities: null });
}
