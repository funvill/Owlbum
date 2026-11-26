import { describe, expect, it, vi } from "vitest";
import { appError, createFakeApi } from "./fake";
import { createQueryFake, type FakePhoto } from "./fake/query";
import { QueryClient, defaultQuery, rerunFor } from "./query";
import type { ChangeReason } from "./bindings/ChangeReason";
import type { QueryResult } from "./bindings/QueryResult";

const photos: FakePhoto[] = [
  { id: 1, rel_path: "a.jpg", date: "2024-01-01T00:00:00" },
  { id: 2, rel_path: "b.jpg", date: "2024-01-02T00:00:00" },
  { id: 3, rel_path: "c.jpg", date: "2024-01-03T00:00:00" },
];

function setup() {
  const album = createQueryFake(photos);
  const fake = createFakeApi(album.handlers);
  return { album, fake };
}

const runs = (fake: ReturnType<typeof createFakeApi>) => fake.calls.filter((c) => c.command === "query_run");

describe("rerunFor", () => {
  const plain = defaultQuery();
  const searching = { ...defaultQuery(), search: "lake" };
  const tagged = { ...defaultQuery(), filter: { tags: [{ kind: "object" as const, value: "boat" }] } };

  it("follows the architecture's table of reasons", () => {
    const expected: [ChangeReason, string][] = [
      ["scan", "run"],
      ["watch", "run"],
      ["info", "run"],
      ["saved", "run"],
      ["trashed", "run"],
      ["facts", "run"],
      ["edit", "rows"],
      ["tidy", "none"],
      ["status", "none"],
      ["tags", "none"],
    ];
    for (const [reason, what] of expected) expect(rerunFor(reason, plain), reason).toBe(what);
  });

  it("re-runs for tags only when the query has search text or a keyword or object filter", () => {
    expect(rerunFor("tags", searching)).toBe("run");
    expect(rerunFor("tags", tagged)).toBe("run");
    expect(rerunFor("tags", { ...defaultQuery(), search: "a" })).toBe("none");
  });
});

describe("the query client", () => {
  it("passes keep on every run after the first", async () => {
    const { fake } = setup();
    const client = new QueryClient(fake, { keep: () => ({ current: { id: 2, index: 1 }, anchor_id: null, selection: [2], view: null }) });
    const first = await client.run(defaultQuery());
    expect(runs(fake)[0]?.args).toEqual({ query: defaultQuery(), keep: null });
    const second = await client.run(defaultQuery());
    expect(runs(fake)[1]?.args).toMatchObject({ keep: { token: first?.token, current: { id: 2, index: 1 } } });
    expect(second?.kept?.current).toEqual({ id: 2, index: 1 });
    expect(client.result).toEqual(second);
  });

  it("throws away an answer to anything but the newest call", async () => {
    const { fake } = setup();
    let release: (() => void) | undefined;
    const slow = new Promise<void>((r) => (release = r));
    const original = createQueryFake(photos).handlers.query_run;
    let n = 0;
    fake.handle("query_run", async (args) => {
      n += 1;
      if (n === 1) await slow;
      return original(args);
    });
    const client = new QueryClient(fake);
    const older = client.run(defaultQuery());
    const newer = await client.run({ ...defaultQuery(), sort: "name" });
    release?.();
    await expect(older).resolves.toBeNull();
    expect(client.result).toEqual(newer);
    expect(client.query?.sort).toBe("name");
  });

  it("re-runs once and repeats the call when a token has expired", async () => {
    const { fake } = setup();
    const client = new QueryClient(fake);
    await client.run(defaultQuery());
    for (let i = 0; i < 4; i++) await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    const rows = await client.window(0, 10);
    expect(rows.map((r) => r.id)).toEqual([1, 2, 3]);
    expect(runs(fake)).toHaveLength(6);
    for (let i = 0; i < 4; i++) await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    await expect(client.locate(3)).resolves.toEqual({ index: 2 });
  });

  it("does not loop when the repeat expires too", async () => {
    const { fake } = setup();
    const client = new QueryClient(fake);
    await client.run(defaultQuery());
    fake.handle("query_window", () => {
      throw appError("query_expired", "gone");
    });
    await expect(client.window(0, 1)).rejects.toMatchObject({ code: "query_expired" });
    expect(runs(fake)).toHaveLength(2);
  });

  it("follows album_changed: re-runs for a newer revision when the reason calls for it", async () => {
    const { album, fake } = setup();
    const onResult = vi.fn<(r: QueryResult) => void>();
    const onRows = vi.fn();
    const client = new QueryClient(fake, { onResult, onRows });
    const stop = await client.follow();
    await client.run(defaultQuery());
    expect(onResult).toHaveBeenCalledTimes(1);

    fake.emit("album_changed", { revision: 0, reason: "scan" });
    await client.settled();
    expect(runs(fake)).toHaveLength(1);

    const revision = album.change((list) => list.slice(1));
    fake.emit("album_changed", { revision, reason: "info" });
    await client.settled();
    expect(runs(fake)).toHaveLength(2);
    expect(client.result?.count).toBe(2);
    expect(onResult).toHaveBeenCalledTimes(2);

    const next = album.change((list) => list);
    fake.emit("album_changed", { revision: next, reason: "tidy" });
    fake.emit("album_changed", { revision: next, reason: "edit" });
    await client.settled();
    expect(runs(fake)).toHaveLength(2);
    expect(onRows).toHaveBeenCalledTimes(1);

    stop();
    fake.emit("album_changed", { revision: album.change((l) => l), reason: "scan" });
    await client.settled();
    expect(runs(fake)).toHaveLength(2);
  });

  it("ignores album_changed before the first run", async () => {
    const { fake } = setup();
    const client = new QueryClient(fake);
    await client.follow();
    fake.emit("album_changed", { revision: 5, reason: "scan" });
    await client.settled();
    expect(runs(fake)).toHaveLength(0);
  });
});
