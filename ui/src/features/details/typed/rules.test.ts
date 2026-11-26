import { describe, expect, it } from "vitest";
import { fakeDetail } from "../../../api/fake/photo";
import {
  addNames,
  cleanName,
  cleanText,
  dateOutcome,
  locationOutcome,
  parseDate,
  splitAtCommas,
  suggestionsFor,
} from "./rules";

const known = [
  { name: "Ann Lee", count: 9 },
  { name: "Émile Zola", count: 5 },
  { name: "Bob", count: 3 },
  { name: "Annette", count: 2 },
];

describe("text", () => {
  it("trims and sends null for empty text", () => {
    expect(cleanText("  hi  ")).toBe("hi");
    expect(cleanText("   ")).toBeNull();
  });
});

describe("people", () => {
  it("cleans a name", () => {
    expect(cleanName("  Ann    Lee ")).toBe("Ann Lee");
    expect(cleanName("   ")).toBeNull();
    expect(cleanName("x".repeat(150))).toHaveLength(100);
  });

  it("adds names once, ignoring case, in the album's spelling", () => {
    expect(addNames(["Bob"], ["bob", "ann lee", " Cy ", "", "cy"], known)).toEqual(["Bob", "Ann Lee", "Cy"]);
  });

  it("splits at commas, keeping the text after the last one", () => {
    expect(splitAtCommas("Ann, Bob,Cy")).toEqual({
      names: ["Ann", " Bob"],
      rest: "Cy",
    });
    expect(splitAtCommas("Ann,")).toEqual({ names: ["Ann"], rest: "" });
  });

  it("suggests names not on the photo whose words start with the text, ignoring case and accents", () => {
    expect(suggestionsFor(known, ["bob"], "")).toEqual(["Ann Lee", "Émile Zola", "Annette"]);
    expect(suggestionsFor(known, [], "ann")).toEqual(["Ann Lee", "Annette"]);
    expect(suggestionsFor(known, [], "lee")).toEqual(["Ann Lee"]);
    expect(suggestionsFor(known, [], "emile")).toEqual(["Émile Zola"]);
    expect(suggestionsFor(known, [], "nn")).toEqual([]);
  });

  it("shows at most 8", () => {
    const many = Array.from({ length: 12 }, (_, i) => ({
      name: `P${i}`,
      count: 1,
    }));
    expect(suggestionsFor(many, [], "")).toHaveLength(8);
  });
});

describe("date", () => {
  it("accepts the three forms with a space or a T", () => {
    expect(parseDate("2024-06-12 14:03:05")).toBe("2024-06-12T14:03:05");
    expect(parseDate("2024-06-12T14:03")).toBe("2024-06-12T14:03:00");
    expect(parseDate("2024-06-12")).toBe("2024-06-12T12:00:00");
  });

  it("refuses dates that do not exist or are out of range", () => {
    for (const bad of ["2024-02-30", "2023-02-29", "1700-01-01", "2201-01-01", "2024-06-12 24:00", "12/06/2024", "x"]) {
      expect(parseDate(bad), bad).toBeNull();
    }
  });

  const camera = fakeDetail(1);
  const typed = fakeDetail(1, {
    date: {
      effective: "2020-01-01T10:00:00",
      camera: "2024-06-12T14:03:00",
      typed: "2020-01-01T10:00:00",
    },
  });
  const undated = fakeDetail(1, {
    date: { effective: null, camera: null, typed: null },
  });

  it("follows the date table", () => {
    expect(dateOutcome("2024-06-12 14:03:00", camera)).toEqual({
      kind: "none",
    });
    expect(dateOutcome("2021-05-05 08:00", camera)).toEqual({
      kind: "send",
      value: "2021-05-05T08:00:00",
    });
    expect(dateOutcome("2024-06-12 14:03", typed)).toEqual({
      kind: "send",
      value: null,
    });
    expect(dateOutcome("", typed)).toEqual({ kind: "send", value: null });
    expect(dateOutcome("", camera)).toEqual({ kind: "restore" });
    expect(dateOutcome("", undated)).toEqual({ kind: "none" });
    expect(dateOutcome("nope", camera)).toEqual({
      kind: "error",
      message: "That is not a valid date and time. Use YYYY-MM-DD HH:MM.",
    });
  });
});

describe("location", () => {
  const gps = fakeDetail(1);
  const typed = fakeDetail(1, {
    place: {
      lat: 51.5,
      lon: -0.12,
      gps: { lat: 48.85837, lon: 2.29448 },
      typed: true,
      name: "London",
      country: null,
    },
  });
  const b = (lat: string, lon: string, name = "") => ({ lat, lon, name });

  it("gives each message of the table", () => {
    expect(locationOutcome(b("x", "1"), gps)).toEqual({
      kind: "error",
      message: "Latitude must be a number.",
    });
    expect(locationOutcome(b("1", "y"), gps)).toEqual({
      kind: "error",
      message: "Longitude must be a number.",
    });
    expect(locationOutcome(b("91", "1"), gps)).toEqual({
      kind: "error",
      message: "Latitude must be between -90 and 90.",
    });
    expect(locationOutcome(b("1", "-181"), gps)).toEqual({
      kind: "error",
      message: "Longitude must be between -180 and 180.",
    });
    expect(locationOutcome(b("1", ""), gps)).toEqual({
      kind: "error",
      message: "Enter both latitude and longitude.",
    });
    expect(locationOutcome(b("", "", "Home"), gps)).toEqual({
      kind: "error",
      message: "Add a latitude and longitude for this place, or clear the name.",
    });
  });

  it("clears, restores or sends", () => {
    expect(locationOutcome(b("", ""), typed)).toEqual({
      kind: "send",
      location: null,
    });
    expect(locationOutcome(b("", ""), gps)).toEqual({
      kind: "restore",
      boxes: b("48.85837", "2.29448"),
    });
    expect(locationOutcome(b("48.858371", "2.294479"), gps)).toEqual({
      kind: "none",
    });
    expect(locationOutcome(b("48.85837", "2.29448"), typed)).toEqual({
      kind: "send",
      location: null,
    });
    expect(locationOutcome(b("10,5", " 20 ", " Spot "), gps)).toEqual({
      kind: "send",
      location: { lat: 10.5, lon: 20, name: "Spot" },
    });
    expect(locationOutcome(b("51.5", "-0.12", "London"), typed)).toEqual({
      kind: "none",
    });
  });
});
