// The page's view of the Rust backend. Every type here comes from the
// generated bindings, so the Rust registry is the one source of truth.

import type { Commands, Events } from "./bindings";
import type { AppError } from "./bindings/AppError";

export type { Commands, Events, AppError };

/** A command's name, for example `"app_version"`. */
export type CommandName = keyof Commands;
/** A command's arguments object. */
export type Args<K extends CommandName> = Commands[K]["args"];
/** A command's reply. */
export type Reply<K extends CommandName> = Commands[K]["result"];
/** An event's name, for example `"album_changed"`. */
export type EventName = keyof Events;
/** An event's payload. */
export type Payload<K extends EventName> = Events[K];

/** The arguments of `invoke`: none for a command that takes none. */
export type ArgsParam<K extends CommandName> =
  Args<K> extends Record<string, never> ? [] : [args: Args<K>];

/** Stops listening. */
export type Unlisten = () => void;

/**
 * What the page can ask of Rust. The real implementation is in `tauri.ts`;
 * page tests use `fake/`.
 *
 * `invoke` rejects with an `AppError` and nothing else.
 */
export interface Api {
  invoke<K extends CommandName>(command: K, ...args: ArgsParam<K>): Promise<Reply<K>>;
  listen<K extends EventName>(event: K, handler: (payload: Payload<K>) => void): Promise<Unlisten>;
}
