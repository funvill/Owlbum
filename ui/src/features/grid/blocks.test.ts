import { describe, expect, it } from "vitest";
import type { Row } from "../../api/bindings/Row";
import { BLOCK, BLOCKS_KEPT, BlockStore, FETCHES } from "./blocks";

function rowOf(id: number, token = "t"): Row {
  return {
    id,
    rel_path: `${token}/${id}.jpg`,
    file_name: `${id}.jpg`,
    thumb_key: `k${id}`,
    aspect: 1,
    kind: "image",
    duration_ms: null,
    star: false,
    hidden: false,
    score: null,
    stack: null,
    edit_plan: null,
  };
}

interface Asked {
  token: string;
  offset: number;
  limit: number;
  resolve: () => void;
  reject: (e: unknown) => void;
}

/** A block store whose fetches wait until the test answers them. */
function setup(count: number, token = "t1") {
  const asked: Asked[] = [];
  const store = new BlockStore(
    (t, offset, limit) =>
      new Promise<Row[]>((resolve, reject) => {
        const rows = Array.from({ length: Math.max(0, Math.min(limit, count - offset)) }, (_, i) =>
          rowOf(offset + i, t),
        );
        asked.push({ token: t, offset, limit, resolve: () => resolve(rows), reject });
      }),
  );
  store.reset(token, count);
  const blocksAsked = () => asked.map((a) => a.offset / BLOCK);
  const answer = async (block: number) => {
    const a = asked.find((x) => x.offset === block * BLOCK && x.token === store.token);
    if (a === undefined) throw new Error(`block ${block} was not asked for`);
    a.resolve();
    await Promise.resolve();
    await Promise.resolve();
  };
  const answerAll = async () => {
    for (let i = 0; i < 200 && store.inFlight > 0; i++) {
      for (const a of [...asked]) a.resolve();
      await new Promise((r) => setTimeout(r, 0));
    }
  };
  return { store, asked, blocksAsked, answer, answerAll };
}

describe("block store", () => {
  it("asks for whole blocks of 200 rows", () => {
    const { store, asked } = setup(10_000);
    store.want({ first: 0, last: 39, span: 40, direction: 1, current: null, anchor: null });
    expect(asked.map((a) => [a.offset, a.limit])).toEqual([[0, BLOCK]]);
  });

  it("asks for the viewport first, then ahead, then behind, three at a time", async () => {
    const { store, blocksAsked, answer } = setup(10_000);
    // Rendered 380 to 420 (blocks 1 and 2); ahead to 820 (3, 4); behind to 180 (0).
    store.want({ first: 380, last: 420, span: 200, direction: 1, current: null, anchor: null });
    expect(store.inFlight).toBe(FETCHES);
    expect(blocksAsked()).toEqual([1, 2, 3]);
    await answer(1);
    expect(blocksAsked()).toEqual([1, 2, 3, 4]);
    await answer(2);
    expect(blocksAsked()).toEqual([1, 2, 3, 4, 0]);
  });

  it("mirrors the order when scrolling up", () => {
    const { store, blocksAsked } = setup(10_000);
    store.want({ first: 2000, last: 2100, span: 200, direction: -1, current: null, anchor: null });
    // Viewport block 10, then behind in the scroll direction: 9, 8.
    expect(blocksAsked()).toEqual([10, 9, 8]);
  });

  it("drops queued blocks that are no longer wanted", async () => {
    const { store, blocksAsked, answer } = setup(100_000);
    store.want({ first: 380, last: 420, span: 200, direction: 1, current: null, anchor: null });
    // A drag of the scrollbar to far away before anything answered.
    store.want({ first: 90_100, last: 90_140, span: 20, direction: 1, current: null, anchor: null });
    await answer(1);
    await answer(2);
    await answer(3);
    expect(blocksAsked()).toEqual([1, 2, 3, 450]);
  });

  it("holds the rows that arrived", async () => {
    const { store, answer } = setup(1000);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null });
    expect(store.row(5)).toBeUndefined();
    await answer(0);
    expect(store.row(5)?.id).toBe(5);
    expect(store.fresh(5)).toBe(true);
    expect(store.row(1000)).toBeUndefined();
  });

  it("never asks for a block past the end", () => {
    const { store, blocksAsked } = setup(250);
    store.want({ first: 0, last: 249, span: 250, direction: 1, current: null, anchor: null });
    expect(blocksAsked()).toEqual([0, 1]);
  });

  it("preloads around the current photo", () => {
    const { store, blocksAsked } = setup(10_000);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: 4005, anchor: null });
    // current − 5 to current + 20: 4000 to 4025, block 20.
    expect(blocksAsked()).toEqual([0, 20]);
  });

  it("keeps at most 30 blocks, never the current photo's or the anchor's", async () => {
    const { store, answerAll } = setup(100_000);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: 5, anchor: 250 });
    await answerAll();
    for (let b = 1; b < 1 + BLOCKS_KEPT + 5; b++) {
      store.want({ first: b * BLOCK, last: b * BLOCK + 10, span: 10, direction: 1, current: 5, anchor: 250 });
      await answerAll();
    }
    const held = store.held();
    expect(held.length).toBeLessThanOrEqual(BLOCKS_KEPT);
    expect(held).toContain(0);
    expect(held).toContain(1);
    expect(held).toContain(BLOCKS_KEPT + 5);
    // The furthest from the viewport went first.
    expect(held).not.toContain(2);
  });

  it("keeps the old result's rows on screen until their replacements arrive", async () => {
    const { store, answer } = setup(1000);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null });
    await answer(0);
    store.reset("t2", 1000);
    expect(store.row(3)?.rel_path).toBe("t1/3.jpg");
    expect(store.fresh(3)).toBe(false);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null });
    await answer(0);
    expect(store.row(3)?.rel_path).toBe("t2/3.jpg");
  });

  it("does not draw old rows past the new count", () => {
    const { store } = setup(1000);
    store.reset("t2", 2);
    expect(store.row(5)).toBeUndefined();
  });

  it("throws away a reply for an older token", async () => {
    const { store, asked } = setup(1000);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null });
    store.reset("t2", 1000);
    asked[0]!.resolve();
    await new Promise((r) => setTimeout(r, 0));
    expect(store.row(3)).toBeUndefined();
    // Only the new result's own request is left.
    expect(store.inFlight).toBe(1);
  });

  it("asks again for the rows on screen after a refresh, showing the old ones meanwhile", async () => {
    const { store, blocksAsked, answer } = setup(1000);
    const want = { first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null } as const;
    store.want(want);
    await answer(0);
    store.refresh();
    expect(store.row(3)).toBeDefined();
    expect(store.fresh(3)).toBe(false);
    store.want(want);
    expect(blocksAsked()).toEqual([0, 0]);
  });

  it("tells listeners when rows arrive", async () => {
    const { store, answer } = setup(1000);
    let told = 0;
    const stop = store.subscribe(() => told++);
    store.want({ first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null });
    await answer(0);
    expect(told).toBeGreaterThan(0);
    const before = store.version;
    stop();
    store.reset("t2", 10);
    expect(store.version).toBeGreaterThan(before);
  });

  it("asks again for a block whose fetch failed, on the next want", async () => {
    const { store, asked, blocksAsked } = setup(1000);
    const want = { first: 0, last: 10, span: 10, direction: 1, current: null, anchor: null } as const;
    store.want(want);
    asked[0]!.reject(new Error("no"));
    await new Promise((r) => setTimeout(r, 0));
    expect(store.inFlight).toBe(0);
    store.want(want);
    expect(blocksAsked()).toEqual([0, 0]);
  });
});
