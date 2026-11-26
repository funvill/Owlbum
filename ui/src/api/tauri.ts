// The real backend, over Tauri's IPC. The only file that imports Tauri.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { toAppError } from "./errors";
import type { Api, ArgsParam, CommandName, EventName, Payload, Reply, Unlisten } from "./types";

export function createTauriApi(): Api {
  return {
    async invoke<K extends CommandName>(command: K, ...args: ArgsParam<K>): Promise<Reply<K>> {
      try {
        return await tauriInvoke<Reply<K>>(command, args[0] ?? {});
      } catch (e) {
        throw toAppError(e);
      }
    },
    async listen<K extends EventName>(
      event: K,
      handler: (payload: Payload<K>) => void,
    ): Promise<Unlisten> {
      return tauriListen<Payload<K>>(event, (e) => handler(e.payload));
    },
  };
}
