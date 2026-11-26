import { describe, expect, it } from "vitest";
import { createFakeApi } from ".";
import { TOKENS_KEPT, WINDOW_MAX, createQueryFake, type FakePhoto } from "./query";
import { defaultQuery } from "../query";
import type { Query } from "../bindings/QueryShape";

const photos: FakePhoto[] = [
  { id: 1, rel_path: "b.jpg", date: "2024-06-02T10:00:00" },
  { id: 2, rel_path: "a.jpg", date: "2024-06-03T10:00:00" },
  { id: 3, rel_path: "Trip/c.jpg", date: null },
  { id: 4, rel_path: "d.jpg", date: "2024-06-01T10:00:00", hidden: true },
  { id: 5, rel_path: "e.jpg", date: "2024-06-01T09:00:00" },
];

function setup(list: FakePhoto[] = photos) {
  const album = createQueryFake(list);
  const fake = createFakeApi(album.handlers);
  return { album, fake };
}

async function ids(fake: ReturnType<typeof createFakeApi>, token: string): Promise<number[]> {
  const rows = await fake.invoke("query_window", { token, offset: 0, limit: WINDOW_MAX });
  return rows.map((r) => r.id);
}

describe("the fake query commands", () => {
  it("list every shown photo by date, undated last, with the counts", async () => {
    const { fake } = setup();
    const r = await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    expect(await ids(fake, r.token)).toEqual([5, 1, 2, 3]);
    expect(r).toMatchObject({ count: 4, photos: 4, total: 4, album_photos: 5, filtered: false, kept: null });
    expect(r.chips).toEqual([]);
  });

  it("sort by name in the name order", async () => {
    const { fake } = setup([
      { id: 1, rel_path: "img_10.jpg" },
      { id: 2, rel_path: "IMG_9.jpg" },
    ]);
    const r = await fake.invoke("query_run", { query: { ...defaultQuery(), sort: "name" }, keep: null });
    expect(await ids(fake, r.token)).toEqual([2, 1]);
  });

  it("sort by score, interest and colour, missing values last, ties by name", async () => {
    const { fake } = setup([
      { id: 1, rel_path: "b.jpg", score: 40, interesting: 0.2, colour: { bucket: 5, lightness: 0.3 } },
      { id: 2, rel_path: "a.jpg", score: 40, interesting: 0.9, colour: null },
      { id: 3, rel_path: "c.jpg", score: null, interesting: null, colour: { bucket: 0, lightness: 0.6 } },
      { id: 4, rel_path: "d.jpg", score: 70, interesting: 0.5, colour: { bucket: 0, lightness: 0.2 } },
    ]);
    const order = async (sort: Query["sort"]) =>
      ids(fake, (await fake.invoke("query_run", { query: { ...defaultQuery(), sort }, keep: null })).token);
    expect(await order("score")).toEqual([4, 2, 1, 3]);
    expect(await order("interesting")).toEqual([2, 4, 1, 3]);
    expect(await order("colour")).toEqual([4, 3, 1, 2]);
  });

  it("answer bad_query for parts not built yet, like Rust", async () => {
    const { fake } = setup();
    const refused: Query[] = [
      { ...defaultQuery(), view: "timeline" },
      { ...defaultQuery(), search: "lake" },
      { ...defaultQuery(), filter: { from: "2024-02-30" } },
    ];
    for (const query of refused) {
      await expect(fake.invoke("query_run", { query, keep: null })).rejects.toMatchObject({ code: "bad_query" });
    }
    const absent: Query = { ...defaultQuery(), search: "a", filter: { has_people: false, tags: [] } };
    await expect(fake.invoke("query_run", { query: absent, keep: null })).resolves.toMatchObject({ count: 4 });
  });

  it("filter like Rust, with chips, X of Y and facets", async () => {
    const { fake } = setup([
      { id: 1, rel_path: "a.jpg", date: "2024-06-01T10:00:00", star: true, width: 4, height: 3 },
      { id: 2, rel_path: "Trip/b.jpg", date: "2024-06-30T23:00:00", width: 3, height: 4, people: ["Ann"] },
      { id: 3, rel_path: "Trip/c.jpg", date: null, hidden: true, star: true, keywords: ["lake"] },
    ]);
    const run = (filter: Query["filter"], want_facets = false) =>
      fake.invoke("query_run", { query: { ...defaultQuery(), filter, want_facets }, keep: null });
    const r = await run({ star: "yes", from: "2024-06-01", to: "2024-06-30", has_people: false }, true);
    expect(await ids(fake, r.token)).toEqual([1]);
    expect(r).toMatchObject({ photos: 1, total: 2, filtered: true, panel_count: 2 });
    expect(r.chips.map((c) => [c.id, c.text])).toEqual([
      ["star", "Starred"],
      ["dates", "2024-06-01 → 2024-06-30"],
    ]);
    expect(r.chips[1]!.remove).toEqual({ star: "yes" });
    expect(r.facets).toMatchObject({ star: { yes: 1, no: 1 }, shape: { portrait: 0, landscape: 1 }, hidden: 0 });
    expect(r.facets!.folders).toEqual([
      { folder: "", label: "Top folder", count: 1 },
      { folder: "Trip", label: "Trip", count: 0 },
    ]);
    expect(await fake.invoke("query_facets", { token: r.token })).toEqual(r.facets);
    expect(await ids(fake, (await run({ hidden: "only", tags: [{ kind: "keyword", value: "Lake" }] })).token)).toEqual([3]);
    expect(await ids(fake, (await run({ people: ["ann"], folder: "Trip" })).token)).toEqual([2]);
  });

  it("keep the last four tokens", async () => {
    const { fake } = setup();
    const tokens: string[] = [];
    for (let i = 0; i <= TOKENS_KEPT; i++) {
      tokens.push((await fake.invoke("query_run", { query: defaultQuery(), keep: null })).token);
    }
    expect(new Set(tokens).size).toBe(tokens.length);
    await expect(fake.invoke("query_window", { token: tokens[0]!, offset: 0, limit: 1 })).rejects.toMatchObject({
      code: "query_expired",
    });
    await expect(fake.invoke("query_locate", { token: tokens[0]!, id: 1 })).rejects.toMatchObject({
      code: "query_expired",
    });
    await expect(fake.invoke("query_locate", { token: tokens[1]!, id: 1 })).resolves.toEqual({ index: 1 });
  });

  it("page rows: at most 500, the tail past the end, with the row fields", async () => {
    const many = Array.from({ length: 520 }, (_, i): FakePhoto => ({ id: i + 1, rel_path: `p${String(i).padStart(4, "0")}.jpg` }));
    const { fake } = setup(many);
    const r = await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    expect(await fake.invoke("query_window", { token: r.token, offset: 0, limit: 10_000 })).toHaveLength(500);
    expect(await fake.invoke("query_window", { token: r.token, offset: 518, limit: 10 })).toHaveLength(2);
    const [row] = await fake.invoke("query_window", { token: r.token, offset: 0, limit: 1 });
    expect(row).toEqual({
      id: 1,
      rel_path: "p0000.jpg",
      file_name: "p0000.jpg",
      thumb_key: "tk-1",
      aspect: 1,
      kind: "image",
      duration_ms: null,
      star: false,
      hidden: false,
      score: null,
      stack: null,
      edit_plan: null,
    });
  });

  it("locate a tile, and nothing for a photo not shown", async () => {
    const { fake } = setup();
    const r = await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    await expect(fake.invoke("query_locate", { token: r.token, id: 3 })).resolves.toEqual({ index: 3 });
    await expect(fake.invoke("query_locate", { token: r.token, id: 4 })).resolves.toBeNull();
  });

  it("answer the ids of a range, in order, with no limit", async () => {
    const { fake } = setup();
    const r = await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    const rows = await fake.invoke("query_window", { token: r.token, offset: 1, limit: 3 });
    await expect(fake.invoke("query_ids", { token: r.token, offset: 1, limit: 3 })).resolves.toEqual(
      rows.map((row) => row.id),
    );
    await expect(fake.invoke("query_ids", { token: r.token, offset: 0, limit: 100_000 })).resolves.toHaveLength(r.count);
    await expect(fake.invoke("query_ids", { token: "q-none", offset: 0, limit: 1 })).rejects.toMatchObject({
      code: "query_expired",
    });
  });

  it("settle keep by the four rules and move the revision on changes", async () => {
    const { album, fake } = setup();
    const first = await fake.invoke("query_run", { query: defaultQuery(), keep: null });
    expect(first.revision).toBe(0);
    album.change((list) => list.map((p) => (p.id === 1 ? { ...p, hidden: true } : p)));
    const r = await fake.invoke("query_run", {
      query: defaultQuery(),
      keep: {
        token: first.token,
        current: { id: 1, index: 1 },
        anchor_id: 1,
        selection: [1, 5],
        view: { id: 5, index: 0 },
      },
    });
    expect(r.revision).toBe(1);
    expect(r.kept).toEqual({
      current: { id: 2, index: 1 },
      anchor: { id: 2, index: 1 },
      selection: [5, 2],
      view: { id: 5, index: 0 },
    });
  });

  it("is fresh for each fake backend", async () => {
    const a = createFakeApi();
    const b = createFakeApi();
    const ta = (await a.invoke("query_run", { query: defaultQuery(), keep: null })).token;
    await expect(b.invoke("query_locate", { token: ta, id: 1 })).rejects.toMatchObject({ code: "query_expired" });
  });
});
