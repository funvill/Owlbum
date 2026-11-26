import type { Commands } from "../types";

/** A fake command: takes the command's arguments, returns its reply. */
export type FakeHandler<C, K extends keyof C> = C[K] extends { args: infer A; result: infer R }
  ? (args: A) => R | Promise<R>
  : never;

/** Fake handlers for every command of an area's `<Area>Commands` map. */
export type AreaHandlers<C> = { [K in keyof C]: FakeHandler<C, K> };

/** Fake handlers for every command. */
export type FakeHandlers = AreaHandlers<Commands>;
