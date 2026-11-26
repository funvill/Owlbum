import { describe, expect, it } from "vitest";
import { createFakeApi } from ".";

const names = async (text: string) =>
  (await createFakeApi().invoke("app_place_search", { text })).map((m) => m.name);

describe("the fake app_place_search", () => {
  it("lists countries whose name contains the text, ignoring case and accents", async () => {
    expect(await names("FRAN")).toEqual(["France"]);
    expect(await names("  fran ")).toEqual(["France"]);
    expect(await names("aland")).toEqual(["Åland"]);
    expect(await names("")).toEqual([]);
    expect(await names("   ")).toEqual([]);
  });

  it("puts names that start with the text first, then word starts, then the rest", async () => {
    expect(await names("ger")).toEqual(["Germany", "Algeria", "Niger", "Nigeria"]);
  });

  it("answers at most 6, each with a central point", async () => {
    const found = await createFakeApi().invoke("app_place_search", { text: "a" });
    expect(found).toHaveLength(6);
    const [france] = await createFakeApi().invoke("app_place_search", { text: "france" });
    expect(france).toEqual({ name: "France", lat: expect.any(Number), lon: expect.any(Number) });
  });
});
