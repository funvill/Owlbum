// The in-memory backend used by page tests. It implements the same `Api` as
// the Tauri backend, and its handlers are typed from the generated bindings:
// a command added in Rust fails the page typecheck until its area's fake
// handler exists here.

import type {
  Api,
  AppError,
  Args,
  ArgsParam,
  CommandName,
  EventName,
  Payload,
  Reply,
  Unlisten,
} from "../types";
import { toAppError } from "../errors";
import { albumHandlers } from "./album";
import { appHandlers } from "./app";
import { editHandlers } from "./edit";
import { fileHandlers } from "./file";
import { photoHandlers } from "./photo";
import { createQueryFake, queryHandlers } from "./query";
import { searchHandlers } from "./search";
import { taggingHandlers } from "./tagging";
import type { FakeHandlers } from "./types";

export type { AreaHandlers, FakeHandlers } from "./types";

/** Every fake command's default behaviour. Tests override what they need. */
export const defaultHandlers: FakeHandlers = {
  ...albumHandlers,
  ...appHandlers,
  ...editHandlers,
  ...fileHandlers,
  ...photoHandlers,
  ...queryHandlers,
  ...searchHandlers,
  ...taggingHandlers,
};

/** One recorded `invoke`. */
export interface FakeCall {
  command: CommandName;
  args: unknown;
}

/** The fake backend, with what a test needs to drive and inspect it. */
export interface FakeApi extends Api {
  /** Every `invoke`, in order. */
  readonly calls: FakeCall[];
  /** Pushes an event to the page's listeners, as Rust would. */
  emit<K extends EventName>(event: K, payload: Payload<K>): void;
  /** Replaces one command's behaviour. */
  handle<K extends CommandName>(command: K, handler: FakeHandlers[K]): void;
}

/** Builds an `AppError` for a fake handler to throw. */
export function appError(code: AppError["code"], message: string, details?: AppError["details"]): AppError {
  return details === undefined ? { code, message } : { code, message, details };
}

/** Creates a fake backend. `overrides` replaces default handlers. */
export function createFakeApi(overrides: Partial<FakeHandlers> = {}): FakeApi {
  const handlers: FakeHandlers = { ...defaultHandlers, ...createQueryFake().handlers, ...overrides };
  const listeners = new Map<string, Set<(payload: never) => void>>();
  const calls: FakeCall[] = [];

  return {
    calls,
    async invoke<K extends CommandName>(command: K, ...args: ArgsParam<K>): Promise<Reply<K>> {
      const a = (args[0] ?? {}) as Args<K>;
      calls.push({ command, args: a });
      const handler = handlers[command] as unknown as (args: Args<K>) => Reply<K> | Promise<Reply<K>>;
      try {
        return await handler(a);
      } catch (e) {
        throw toAppError(e);
      }
    },
    async listen<K extends EventName>(event: K, handler: (payload: Payload<K>) => void): Promise<Unlisten> {
      const set = listeners.get(event) ?? new Set();
      listeners.set(event, set);
      const h = handler as (payload: never) => void;
      set.add(h);
      return () => {
        set.delete(h);
      };
    },
    emit<K extends EventName>(event: K, payload: Payload<K>): void {
      for (const h of listeners.get(event) ?? []) (h as (payload: Payload<K>) => void)(payload);
    },
    handle<K extends CommandName>(command: K, handler: FakeHandlers[K]): void {
      handlers[command] = handler;
    },
  };
}
