// The rows the page holds for one result, fetched in blocks
// (docs/04-browsing.md, "Blocks of rows"). The page never holds the whole
// album: at most BLOCKS_KEPT blocks around what is on screen.

import type { Row } from "../../api/bindings/Row";

/** Rows per block. Block `b` covers indexes `200b` to `200b + 199`. */
export const BLOCK = 200;
/** Blocks held per result. */
export const BLOCKS_KEPT = 30;
/** Block requests in flight at once. */
export const FETCHES = 3;
/** Blocks are fetched this many viewports past the viewport in the scroll direction… */
export const AHEAD = 2;
/** …and this many the other way. */
export const BEHIND = 1;
/** The preload window around the current photo. */
const PRELOAD_BEFORE = 5;
const PRELOAD_AFTER = 20;

/** Fetches `limit` rows at `offset` of the result named by `token`. */
export type FetchRows = (token: string, offset: number, limit: number) => Promise<Row[]>;

/** What the grid has on screen, for working out which blocks are wanted. */
export interface Want {
  /** The first index rendered. */
  first: number;
  /** The last index rendered. */
  last: number;
  /** Indexes in one viewport. */
  span: number;
  /** +1 after scrolling down, −1 after scrolling up. */
  direction: 1 | -1;
  /** The current photo's index; its block is never dropped and its neighbours are preloaded. */
  current: number | null;
  /** The anchor's index; its block is never dropped. */
  anchor: number | null;
}

const blockOf = (index: number) => Math.floor(index / BLOCK);

export class BlockStore {
  /** The result the blocks belong to. */
  token = "";
  /** Places in that result. */
  count = 0;
  /** Moves on whenever what `row` answers may have changed. */
  version = 0;

  private blocks = new Map<number, Row[]>();
  /** The previous result's rows (or the rows before a refresh), drawn until their replacements arrive. */
  private stale = new Map<number, Row[]>();
  private staleCount = 0;
  /** Blocks of this result being asked for. */
  private asking = new Set<number>();
  /** Requests in flight, for any result: an old result's request still takes a place until it answers. */
  private outstanding = 0;
  private queue: number[] = [];
  private last: Want | null = null;
  private listeners = new Set<() => void>();

  constructor(
    private readonly fetchRows: FetchRows,
    private readonly onError?: (error: unknown) => void,
  ) {}

  /** Requests in flight. */
  get inFlight(): number {
    return this.outstanding;
  }

  /** The blocks held, in order. */
  held(): number[] {
    return [...this.blocks.keys()].sort((a, b) => a - b);
  }

  /** A new result. Its rows are not known yet; the old ones are drawn until they are. */
  reset(token: string, count: number): void {
    this.stale = this.blocks.size > 0 ? this.blocks : this.stale;
    this.staleCount = this.blocks.size > 0 ? this.count : this.staleCount;
    this.token = token;
    this.count = count;
    this.blocks = new Map();
    this.asking = new Set();
    this.queue = [];
    this.changed();
    if (this.last !== null) this.want(this.last);
  }

  /** Forgets everything, old rows included (the album closed). */
  clear(): void {
    this.token = "";
    this.count = 0;
    this.blocks = new Map();
    this.stale = new Map();
    this.staleCount = 0;
    this.asking = new Set();
    this.queue = [];
    this.last = null;
    this.changed();
  }

  /** The same result, but its rows changed (an edit): ask again, drawing the old ones meanwhile. */
  refresh(): void {
    for (const [b, rows] of this.blocks) this.stale.set(b, rows);
    this.staleCount = this.count;
    this.blocks = new Map();
    this.asking = new Set();
    this.queue = [];
    this.changed();
    if (this.last !== null) this.want(this.last);
  }

  /** The row at `index`, from this result or, until it arrives, the one drawn there before. */
  row(index: number): Row | undefined {
    if (index < 0 || index >= this.count) return undefined;
    const b = blockOf(index);
    const fresh = this.blocks.get(b);
    if (fresh !== undefined) return fresh[index - b * BLOCK];
    if (index >= this.staleCount) return undefined;
    return this.stale.get(b)?.[index - b * BLOCK];
  }

  /** Whether the row at `index` belongs to this result. */
  fresh(index: number): boolean {
    return this.blocks.get(blockOf(index))?.[index - blockOf(index) * BLOCK] !== undefined;
  }

  /** Says what is on screen; asks for the blocks wanted and forgets queued ones no longer wanted. */
  want(w: Want): void {
    this.last = w;
    this.queue = this.wanted(w).filter((b) => !this.blocks.has(b) && !this.asking.has(b));
    this.pump();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** The blocks wanted, most wanted first. */
  private wanted(w: Want): number[] {
    if (this.count === 0) return [];
    const lastBlock = blockOf(this.count - 1);
    const clampIndex = (i: number) => Math.min(this.count - 1, Math.max(0, i));
    const order: number[] = [];
    const add = (b: number) => {
      if (b >= 0 && b <= lastBlock && !order.includes(b)) order.push(b);
    };
    const span = Math.max(1, w.span);
    const first = clampIndex(w.first);
    const last = clampIndex(Math.max(w.first, w.last));
    const [lo, hi] = [blockOf(first), blockOf(last)];
    const forward = w.direction > 0;
    // The viewport, in the scroll direction.
    for (let i = 0; i <= hi - lo; i++) add(forward ? lo + i : hi - i);
    // The preload window around the current photo.
    if (w.current !== null) {
      const from = blockOf(clampIndex(w.current - PRELOAD_BEFORE));
      const to = blockOf(clampIndex(w.current + PRELOAD_AFTER));
      for (let b = from; b <= to; b++) add(b);
    }
    // Ahead in the scroll direction, outward; then behind.
    const aheadEnd = forward ? clampIndex(last + AHEAD * span) : clampIndex(first - AHEAD * span);
    const behindEnd = forward ? clampIndex(first - BEHIND * span) : clampIndex(last + BEHIND * span);
    if (forward) {
      for (let b = hi + 1; b <= blockOf(aheadEnd); b++) add(b);
      for (let b = lo - 1; b >= blockOf(behindEnd); b--) add(b);
    } else {
      for (let b = lo - 1; b >= blockOf(aheadEnd); b--) add(b);
      for (let b = hi + 1; b <= blockOf(behindEnd); b++) add(b);
    }
    return order;
  }

  private pump(): void {
    while (this.outstanding < FETCHES && this.queue.length > 0) {
      const b = this.queue.shift()!;
      this.fetch(b);
    }
  }

  private fetch(b: number): void {
    const token = this.token;
    const asking = this.asking;
    asking.add(b);
    this.outstanding++;
    const current = () => token === this.token && asking === this.asking;
    this.fetchRows(token, b * BLOCK, BLOCK).then(
      (rows) => {
        this.outstanding--;
        asking.delete(b);
        if (current()) {
          this.blocks.set(b, rows);
          this.stale.delete(b);
          this.evict();
          this.changed();
        }
        this.pump();
      },
      (error: unknown) => {
        this.outstanding--;
        asking.delete(b);
        if (current()) this.onError?.(error);
        this.pump();
      },
    );
  }

  /** Drops the blocks furthest from the viewport beyond BLOCKS_KEPT, never the current photo's or the anchor's. */
  private evict(): void {
    if (this.blocks.size <= BLOCKS_KEPT) return;
    const w = this.last;
    const centre = w === null ? 0 : blockOf((w.first + w.last) / 2);
    const keep = new Set<number>();
    if (w?.current != null) keep.add(blockOf(w.current));
    if (w?.anchor != null) keep.add(blockOf(w.anchor));
    const droppable = [...this.blocks.keys()]
      .filter((b) => !keep.has(b))
      .sort((a, b) => Math.abs(b - centre) - Math.abs(a - centre));
    while (this.blocks.size > BLOCKS_KEPT && droppable.length > 0) this.blocks.delete(droppable.shift()!);
  }

  private changed(): void {
    this.version++;
    for (const listener of this.listeners) listener();
  }
}
