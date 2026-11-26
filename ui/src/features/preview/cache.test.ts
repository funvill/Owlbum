import { describe, expect, it } from "vitest";
import {
  AHEAD,
  BEHIND,
  CAPACITY,
  FAILED_FOR_MS,
  ImageCache,
  MAX_IN_FLIGHT,
  preloadOrder,
  type Load,
  type Wanted,
} from "./cache";

describe("preloadOrder", () => {
  it("goes LEAD ahead, one behind, the rest ahead, then the rest behind", () => {
    expect(preloadOrder(100, 1000, 1)).toEqual([
      101, 102, 103, 99, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 98, 97,
      96, 95,
    ]);
  });

  it("mirrors the order going back", () => {
    expect(preloadOrder(100, 1000, -1)).toEqual(preloadOrder(100, 1000, 1).map((i) => 200 - i));
  });

  it("holds AHEAD + BEHIND places in the middle of a result", () => {
    expect(preloadOrder(500, 1000, 1)).toHaveLength(AHEAD + BEHIND);
  });

  it("stays inside the result at both ends and in both directions", () => {
    expect(preloadOrder(0, 1000, 1)).toEqual(Array.from({ length: 20 }, (_, i) => i + 1));
    expect(preloadOrder(0, 1000, -1)).toEqual([1, 2, 3, 4, 5]);
    expect(preloadOrder(999, 1000, 1)).toEqual([998, 997, 996, 995, 994]);
    expect(preloadOrder(999, 1000, -1)).toEqual(Array.from({ length: 20 }, (_, i) => 998 - i));
    expect(preloadOrder(1, 1000, 1)).toEqual([2, 3, 4, 0, ...Array.from({ length: 17 }, (_, i) => i + 5)]);
  });

  it("handles short results with no duplicates and never the index itself", () => {
    expect(preloadOrder(0, 1, 1)).toEqual([]);
    expect(preloadOrder(1, 3, 1)).toEqual([2, 0]);
    expect(preloadOrder(1, 3, -1)).toEqual([0, 2]);
    for (const dir of [1, -1] as const) {
      for (let i = 0; i < 7; i++) {
        const order = preloadOrder(i, 7, dir);
        expect(new Set(order).size).toBe(order.length);
        expect(order).not.toContain(i);
        expect(order.every((n) => n >= 0 && n < 7)).toBe(true);
        expect(order.length).toBe(dir === 1 ? Math.min(6, 6 - i + Math.min(i, BEHIND)) : Math.min(6, i + Math.min(6 - i, BEHIND)));
      }
    }
  });
});

/** A loader the test finishes by hand. */
function fakeLoader() {
  const loads = new Map<string, { resolve: () => void; reject: () => void; cancelled: boolean; priority: string }>();
  const started: string[] = [];
  const loader = (url: string, priority: "high" | "low"): Load => {
    started.push(url);
    let resolve = () => {};
    let reject = () => {};
    const done = new Promise<void>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    const rec = { resolve, reject, cancelled: false, priority };
    loads.set(url, rec);
    return { done, cancel: () => (rec.cancelled = true) };
  };
  return { loader, loads, started };
}

const w = (n: number): Wanted => ({ id: n, url: `m/${n}` });
const range = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => w(from + i));
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("ImageCache", () => {
  it("starts the first at once with high priority and the rest within the in-flight cap", () => {
    const f = fakeLoader();
    const cache = new ImageCache(f.loader);
    cache.want(range(1, 10));
    expect(f.started).toEqual(["m/1", "m/2", "m/3", "m/4", "m/5"]);
    expect(f.loads.get("m/1")!.priority).toBe("high");
    expect(f.loads.get("m/2")!.priority).toBe("low");
    // A new picture on screen starts even with the cap reached.
    cache.want([w(20), ...range(2, 10)]);
    expect(f.started.at(-1)).toBe("m/20");
    expect(f.loads.get("m/20")!.priority).toBe("high");
  });

  it("cancels loads no longer wanted and starts the next in order", async () => {
    const f = fakeLoader();
    const cache = new ImageCache(f.loader);
    cache.want(range(1, 10));
    cache.want(range(3, 12));
    expect(f.loads.get("m/1")!.cancelled).toBe(true);
    expect(f.loads.get("m/2")!.cancelled).toBe(true);
    expect(f.loads.get("m/3")!.cancelled).toBe(false);
    // m/3 is now on screen but already running: background is 4, 5 plus 6, 7.
    expect(f.started.slice(5)).toEqual(["m/6", "m/7"]);
    f.loads.get("m/4")!.resolve();
    await flush();
    expect(cache.ready("m/4")).toBe(true);
    expect(f.started.at(-1)).toBe("m/8");
  });

  it("asks for nothing new when the wanted pictures are ready", async () => {
    const f = fakeLoader();
    const cache = new ImageCache(f.loader);
    cache.want(range(1, 3));
    for (const l of f.loads.values()) l.resolve();
    await flush();
    const before = f.started.length;
    cache.want([w(2), w(3), w(1)]);
    expect(f.started.length).toBe(before);
    expect(cache.ready("m/2")).toBe(true);
  });

  it(`keeps at most ${CAPACITY}, dropping the least recently wanted and never a wanted one`, async () => {
    const f = fakeLoader();
    const cache = new ImageCache(f.loader);
    for (let n = 1; n <= CAPACITY + 5; n++) {
      cache.want([w(n)]);
      f.loads.get(`m/${n}`)!.resolve();
      await flush();
    }
    expect(cache.held()).toHaveLength(CAPACITY);
    expect(cache.ready("m/1")).toBe(false);
    expect(cache.ready("m/5")).toBe(false);
    expect(cache.ready("m/6")).toBe(true);
    // Wanting more than CAPACITY at once never drops a wanted one.
    const many = range(100, 100 + CAPACITY + 4);
    cache.want(many);
    for (let round = 0; round < 20; round++) {
      for (const l of f.loads.values()) l.resolve();
      await flush();
    }
    expect(many.every((p) => cache.ready(p.url))).toBe(true);
  });

  it("does not ask again for a failed picture until the wait is over", async () => {
    let now = 1000;
    const f = fakeLoader();
    const cache = new ImageCache(f.loader, () => now);
    cache.want([w(1)]);
    f.loads.get("m/1")!.reject();
    await flush();
    cache.want([w(2)]);
    cache.want([w(1)]);
    expect(f.started.filter((u) => u === "m/1")).toHaveLength(1);
    now += FAILED_FOR_MS;
    cache.want([w(1)]);
    expect(f.started.filter((u) => u === "m/1")).toHaveLength(2);
  });

  it("drops the pictures of rekeyed photos and empties on clear", async () => {
    const f = fakeLoader();
    const cache = new ImageCache(f.loader);
    cache.want(range(1, 3));
    for (const l of f.loads.values()) l.resolve();
    await flush();
    cache.dropPhotos([2]);
    expect(cache.held().sort()).toEqual(["m/1", "m/3"]);
    cache.clear();
    expect(cache.held()).toEqual([]);
  });

  it(`runs at most ${MAX_IN_FLIGHT} background loads`, () => {
    const f = fakeLoader();
    const cache = new ImageCache(f.loader);
    cache.want(range(1, 30));
    expect(f.started).toHaveLength(MAX_IN_FLIGHT + 1);
    expect(cache.inFlight).toBe(MAX_IN_FLIGHT + 1);
  });
});
