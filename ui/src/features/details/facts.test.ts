import { describe, expect, it } from "vitest";
import { fakeDetail } from "../../api/fake/photo";
import {
  cameraRows,
  clippingParts,
  factRows,
  formatCoordinates,
  formatLength,
  formatShutter,
  formatSize,
  formatTaken,
  ratioName,
  shapeName,
} from "./facts";

describe("Taken", () => {
  it("is day, short month, year and 24-hour time, from the written digits", () => {
    expect(formatTaken("2024-06-12T14:03:00")).toBe("12 Jun 2024, 14:03");
    expect(formatTaken("2026-01-01T00:05:59")).toBe("1 Jan 2026, 00:05");
    expect(formatTaken("1999-12-31T23:59:00")).toBe("31 Dec 1999, 23:59");
  });

  it("is Unknown with no date, or one it cannot read", () => {
    expect(formatTaken(null)).toBe("Unknown");
    expect(formatTaken("sometime")).toBe("Unknown");
  });
});

describe("Size", () => {
  it("is GB from 1 GB, MB from 1 MB, else whole KB with a minimum of 1", () => {
    expect(formatSize(3_250_585)).toBe("3.1 MB");
    expect(formatSize(1_048_576)).toBe("1.0 MB");
    expect(formatSize(1_048_575)).toBe("1024 KB");
    expect(formatSize(1_073_741_824 * 2.25)).toBe("2.3 GB");
    expect(formatSize(5_000)).toBe("5 KB");
    expect(formatSize(10)).toBe("1 KB");
    expect(formatSize(0)).toBe("1 KB");
  });
});

describe("Ratio and shape", () => {
  it("takes the first name within 2%, else r.rr:1", () => {
    expect(ratioName(4032, 3024)).toBe("4:3");
    expect(ratioName(3024, 4032)).toBe("4:3");
    expect(ratioName(6000, 4000)).toBe("3:2");
    expect(ratioName(1920, 1080)).toBe("16:9");
    expect(ratioName(1000, 1000)).toBe("1:1");
    expect(ratioName(1015, 1000)).toBe("1:1");
    expect(ratioName(1250, 1000)).toBe("5:4");
    expect(ratioName(2000, 1000)).toBe("2:1");
    expect(ratioName(3000, 1000)).toBe("3.00:1");
    expect(ratioName(2370, 1000)).toBe("2.37:1");
  });

  it("is Square when the sides differ by less than 1%", () => {
    expect(shapeName(1000, 1009)).toBe("Square");
    expect(shapeName(1000, 1011)).toBe("Portrait");
    expect(shapeName(4032, 3024)).toBe("Landscape");
    expect(shapeName(3024, 4032)).toBe("Portrait");
  });
});

describe("Length", () => {
  it("is m:ss under an hour and h:mm:ss from an hour, rounded down", () => {
    expect(formatLength(83_500)).toBe("1:23");
    expect(formatLength(5_999)).toBe("0:05");
    expect(formatLength(3_600_000)).toBe("1:00:00");
    expect(formatLength(3_725_000)).toBe("1:02:05");
  });
});

describe("Fact rows", () => {
  it("lists Taken, Size, Resolution, Shape, Format and Folder for a photo", () => {
    expect(factRows(fakeDetail(42))).toEqual([
      { label: "Taken", value: "12 Jun 2024, 14:03" },
      { label: "Size", value: "4032 × 3024 px · 3.1 MB" },
      { label: "Resolution", value: "12.2 MP · 4:3" },
      { label: "Shape", value: "Landscape" },
      { label: "Format", value: "JPEG" },
      { label: "Folder", value: "2024/Trip" },
    ]);
  });

  it("adds Length for a video and leaves out Folder in the album root", () => {
    const rows = factRows(
      fakeDetail(7, { kind: "video", format: "mp4", width: 1920, height: 1080, duration_ms: 83_500, rel_dir: "" }),
    );
    expect(rows.map((r) => r.label)).toEqual(["Taken", "Size", "Length", "Resolution", "Shape", "Format"]);
    expect(rows.find((r) => r.label === "Length")?.value).toBe("1:23");
    expect(rows.find((r) => r.label === "Format")?.value).toBe("MP4");
  });

  it("leaves out the pixel facts while the dimensions are not known", () => {
    const rows = factRows(fakeDetail(7, { width: 0, height: 0 }));
    expect(rows.map((r) => r.label)).toEqual(["Taken", "Size", "Format", "Folder"]);
    expect(rows[1]?.value).toBe("3.1 MB");
  });
});

describe("Camera rows", () => {
  it("writes each known value", () => {
    expect(cameraRows(fakeDetail(1).camera)).toEqual([
      { label: "Camera", value: "Owlbum Fixture One" },
      { label: "Lens", value: "Fixture 24-70mm F2.8" },
      { label: "Focal length", value: "35 mm (52 mm in 35 mm terms)" },
      { label: "Aperture", value: "f/5.6" },
      { label: "Shutter", value: "1/250 s" },
      { label: "ISO", value: "100" },
      { label: "Exposure", value: "−0.7 EV" },
      { label: "Flash", value: "Did not fire" },
    ]);
  });

  it("gives the 35 mm value only when it differs by 1 mm or more", () => {
    const base = fakeDetail(1).camera;
    expect(cameraRows({ ...base, focal_mm: 50, focal35_mm: 50.5 })[2]?.value).toBe("50 mm");
    expect(cameraRows({ ...base, focal_mm: 4.25, focal35_mm: null })[2]?.value).toBe("4.3 mm");
  });

  it("writes exposure with a sign, a real minus, or 0", () => {
    const base = fakeDetail(1).camera;
    const exposure = (bias_ev: number) => cameraRows({ ...base, bias_ev }).find((r) => r.label === "Exposure")?.value;
    expect(exposure(0.7)).toBe("+0.7 EV");
    expect(exposure(-1)).toBe("−1 EV");
    expect(exposure(0)).toBe("0 EV");
    expect(exposure(-0.01)).toBe("0 EV");
  });

  it("writes the shutter as a fraction below one second", () => {
    expect(formatShutter(0.004)).toBe("1/250 s");
    expect(formatShutter(1 / 3)).toBe("1/3 s");
    expect(formatShutter(1)).toBe("1 s");
    expect(formatShutter(2.5)).toBe("2.5 s");
  });

  it("is empty with no camera details", () => {
    const none = {
      camera: null,
      lens: null,
      focal_mm: null,
      focal35_mm: null,
      aperture: null,
      shutter_s: null,
      iso: null,
      bias_ev: null,
      flash: null,
    };
    expect(cameraRows(none)).toEqual([]);
    expect(cameraRows({ ...none, flash: true })).toEqual([{ label: "Flash", value: "Fired" }]);
  });
});

describe("Place and clipping", () => {
  it("writes coordinates to 5 decimals, latitude first", () => {
    expect(formatCoordinates(48.858372, 2.294481)).toBe("48.85837, 2.29448");
    expect(formatCoordinates(-33.9, 151)).toBe("-33.90000, 151.00000");
  });

  it("writes clipping as percentages, marking 2.0 % or more", () => {
    expect(clippingParts({ shadows: 0.004, highlights: 0.023 })).toEqual([
      { text: "Shadows clipped 0.4 %", high: false },
      { text: "Highlights clipped 2.3 %", high: true },
    ]);
    expect(clippingParts({ shadows: 0.0199, highlights: 0.02 })).toEqual([
      { text: "Shadows clipped 2.0 %", high: true },
      { text: "Highlights clipped 2.0 %", high: true },
    ]);
  });
});
