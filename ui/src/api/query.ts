// The page's side of the query engine (docs/04-browsing.md, "Running a
// query"): runs the query, keeps only the newest answer, follows
// `album_changed`, and repeats a call once when its token has expired.
// Coalescing re-runs in time is left to the store that drives this client.

import type { AlbumChanged } from "./bindings/AlbumChanged";
import type { ChangeReason } from "./bindings/ChangeReason";
import type { Keep } from "./bindings/Keep";
import type { Located } from "./bindings/Located";
import type { Query } from "./bindings/QueryShape";
import type { QueryResult } from "./bindings/QueryResult";
import type { Row } from "./bindings/Row";
import { isAppError, toAppError } from "./errors";
import type { Api, AppError, Unlisten } from "./types";

/** The fewest letters or digits search text needs to be a filter, as in Rust. */
const SEARCH_MIN = 2;

/** The query of a freshly opened album: every shown photo in the grid, by date. */
export function defaultQuery(): Query {
  return {
    view: "grid",
    timeline_unit: "auto",
    sort: "date",
    stack: true,
    open_stacks: [],
    filter: {},
    search: null,
    want_facets: false,
  };
}

/** What `album_changed` calls for: run the query again, fetch the rows on screen again, or nothing. */
export type Rerun = "run" | "rows" | "none";

/** The architecture's table of reasons (docs/01-architecture.md, "Events"). */
export function rerunFor(reason: ChangeReason, query: Query): Rerun {
  switch (reason) {
    case "scan":
    case "watch":
    case "info":
    case "saved":
    case "trashed":
    case "facts":
      return "run";
    case "edit":
      return "rows";
    case "tidy":
    case "status":
      return "none";
    case "tags": {
      const searching = (query.search?.match(/[\p{L}\p{N}]/gu)?.length ?? 0) >= SEARCH_MIN;
      return searching || (query.filter.tags?.length ?? 0) > 0 ? "run" : "none";
    }
  }
}

/** What the client asks of the page that drives it. */
export interface QueryClientOptions {
  /** The current photo, anchor and selection to follow into the next result. */
  keep?: () => Omit<Keep, "token">;
  /** A new result is in. */
  onResult?: (result: QueryResult) => void;
  /** The rows on screen should be fetched again (an `edit` change). */
  onRows?: () => void;
  /** A re-run started by `album_changed` failed. */
  onError?: (error: AppError) => void;
}

/**
 * Runs the page's query against one backend. `result` is the newest
 * answer; an answer to any older call is thrown away.
 */
export class QueryClient {
  /** The newest result, once one arrived. */
  result: QueryResult | null = null;
  /** The query of the newest call. */
  query: Query | null = null;

  private calls = 0;
  private pending = new Set<Promise<unknown>>();

  constructor(
    private readonly api: Api,
    private readonly options: QueryClientOptions = {},
  ) {}

  /**
   * Runs `query`, passing `keep` from the result held. Resolves with the
   * result, or null when a newer call was made meanwhile.
   */
  run(query: Query): Promise<QueryResult | null> {
    return this.track(this.runNow(query));
  }

  /** Rows of the newest result. Re-runs once and repeats the call if its token expired. */
  window(offset: number, limit: number): Promise<Row[]> {
    return this.track(this.repeat((token) => this.api.invoke("query_window", { token, offset, limit })));
  }

  /** Where photo `id` is in the newest result. Re-runs once and repeats the call if its token expired. */
  locate(id: number): Promise<Located | null> {
    return this.track(this.repeat((token) => this.api.invoke("query_locate", { token, id })));
  }

  /** Follows `album_changed` until the returned function is called. */
  async follow(): Promise<Unlisten> {
    return this.api.listen("album_changed", (change) => this.changed(change));
  }

  /** Resolves when everything the client started has finished. */
  async settled(): Promise<void> {
    while (this.pending.size > 0) await Promise.allSettled([...this.pending]);
  }

  private async runNow(query: Query): Promise<QueryResult | null> {
    const call = ++this.calls;
    this.query = query;
    const held = this.result;
    const keep: Keep | null =
      held !== null && this.options.keep !== undefined ? { token: held.token, ...this.options.keep() } : null;
    const result = await this.api.invoke("query_run", { query, keep });
    if (call !== this.calls) return null;
    this.result = result;
    this.options.onResult?.(result);
    return result;
  }

  private async repeat<T>(command: (token: string) => Promise<T>): Promise<T> {
    const held = this.result;
    if (held === null || this.query === null) throw new Error("No query has been run yet.");
    try {
      return await command(held.token);
    } catch (e) {
      if (!isAppError(e) || e.code !== "query_expired") throw e;
      const again = await this.runNow(this.query);
      const token = again?.token ?? this.result?.token ?? held.token;
      return command(token);
    }
  }

  private changed(change: AlbumChanged): void {
    const held = this.result;
    const query = this.query;
    if (held === null || query === null || change.revision <= held.revision) return;
    switch (rerunFor(change.reason, query)) {
      case "run":
        this.run(query).catch((e: unknown) => this.options.onError?.(toAppError(e)));
        break;
      case "rows":
        this.options.onRows?.();
        break;
      case "none":
        break;
    }
  }

  private track<T>(work: Promise<T>): Promise<T> {
    this.pending.add(work);
    const done = () => this.pending.delete(work);
    work.then(done, done);
    return work;
  }
}
