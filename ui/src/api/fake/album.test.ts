import { describe, expect, it } from "vitest";
import { createFakeApi } from ".";
import { emptyScanReport, pageOf } from "./album";
import type { UnsupportedFile } from "../bindings/UnsupportedFile";

const heic = (n: number): UnsupportedFile => ({
  rel_path: `${n}.heic`,
  label: "HEIC",
  reason: "HEIC photos are not supported yet.",
});

describe("the fake scan report commands", () => {
  it("answer an empty report and empty lists by default", async () => {
    const fake = createFakeApi();
    await expect(fake.invoke("album_scan_report")).resolves.toEqual(emptyScanReport);
    await expect(fake.invoke("album_unsupported", { offset: 0, limit: 10 })).resolves.toEqual({
      total: 0,
      items: [],
    });
    await expect(fake.invoke("album_scan_errors", { offset: 0, limit: 10 })).resolves.toEqual({
      total: 0,
      items: [],
    });
  });

  it("answer a fresh report each call", async () => {
    const fake = createFakeApi();
    const report = await fake.invoke("album_scan_report");
    report.nested_albums.push({ rel_path: "x", message: "x is its own album and is not shown here." });
    const again = await fake.invoke("album_scan_report");
    expect(again.nested_albums).toEqual([]);
  });

  it("page a list like Rust: offset, limit, at most 500", () => {
    const list = Array.from({ length: 600 }, (_, i) => heic(i));
    expect(pageOf(list, 0, 2)).toEqual({ total: 600, items: [heic(0), heic(1)] });
    expect(pageOf(list, 599, 10)).toEqual({ total: 600, items: [heic(599)] });
    expect(pageOf(list, 700, 10)).toEqual({ total: 600, items: [] });
    expect(pageOf(list, 0, 10_000).items).toHaveLength(500);
    expect(pageOf(list, 0, 0).items).toEqual([]);
  });

  it("let a test serve its own files", async () => {
    const fake = createFakeApi();
    const list = [heic(1), heic(2), heic(3)];
    fake.handle("album_unsupported", ({ offset, limit }) => pageOf(list, offset, limit));
    await expect(fake.invoke("album_unsupported", { offset: 1, limit: 1 })).resolves.toEqual({
      total: 3,
      items: [heic(2)],
    });
  });
});
