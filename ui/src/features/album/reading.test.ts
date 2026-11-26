import { describe, expect, it } from "vitest";
import type { ScanProgress } from "../../api/bindings/ScanProgress";
import { albumText } from "../../text/album";
import { estimate, readingLabel } from "./reading";
import { viewOnlyReason } from "./banners";
import { fakeStatus } from "../../api/fake/album";

const at = (p: Partial<ScanProgress>): ScanProgress => ({
  phase: "reading",
  found: 100,
  done: 0,
  failed: 0,
  finished: false,
  cancelled: false,
  ...p,
});

describe("reading bar label", () => {
  it("says Looking for photos while walking or before anything is found", () => {
    expect(readingLabel(at({ phase: "walking", found: 50 }), null, 0)).toBe(albumText.scanWalking);
    expect(readingLabel(at({ found: 0 }), null, 0)).toBe(albumText.scanWalking);
  });

  it("counts done of found, with no estimate before 5 files and 2 seconds of reading", () => {
    const start = { done: 40, at: 0 };
    expect(readingLabel(at({ done: 44 }), start, 10_000)).toBe(albumText.scanReading(44, 100));
    expect(readingLabel(at({ done: 60 }), start, 1_999)).toBe(albumText.scanReading(60, 100));
  });

  it("does not count files that needed no reading as work done", () => {
    // 40 unchanged files were done at once; 10 read in 10 s leaves 50 at 1 s each.
    expect(estimate(at({ done: 50 }), { done: 40, at: 0 }, 10_000)).toBe(albumText.lessThanAMinute);
    // 10 read in 100 s, 50 left: 500 s, rounded up to 9 minutes.
    expect(estimate(at({ done: 50 }), { done: 40, at: 0 }, 100_000)).toBe(albumText.minutesLeft(9));
  });

  it("writes hours from 90 minutes and singular units for 1", () => {
    // 5 read in 300 s, 95 left: 5,700 s = 95 minutes.
    expect(estimate(at({ done: 5 }), { done: 0, at: 0 }, 300_000)).toBe(albumText.hoursLeft(1, 35));
    expect(albumText.hoursLeft(1, 1)).toBe("about 1 hour 1 minute left");
    expect(albumText.minutesLeft(1)).toBe("about 1 minute left");
    expect(albumText.minutesLeft(2)).toBe("about 2 minutes left");
  });

  it("adds the estimate after the count", () => {
    expect(readingLabel(at({ done: 50 }), { done: 40, at: 0 }, 10_000)).toBe(
      `${albumText.scanReading(50, 100)} · ${albumText.lessThanAMinute}`,
    );
  });
});

describe("view-only reason", () => {
  it("is the first sentence of the banner in force", () => {
    expect(viewOnlyReason(fakeStatus("D:\\Trip", { view_only: "read_only" }))).toBe(
      "View only: this folder can't be written to.",
    );
    expect(viewOnlyReason(fakeStatus("D:\\Trip"))).toBeNull();
  });
});
