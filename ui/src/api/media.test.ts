import { describe, expect, it } from "vitest";
import { fullUrl, mediaPlatform, originalUrl, thumbUrl, videoUrl } from "./media";
import { createFakeApi } from "./fake";

const key = "0123456789abcdef0123456789abcdef01234567";

describe("picture URLs", () => {
  it("use owlbum:// where the webview takes a custom scheme as it is", () => {
    expect(thumbUrl(key, "s", "scheme")).toBe(`owlbum://thumb/${key}/s`);
    expect(thumbUrl(key, "m", "scheme")).toBe(`owlbum://thumb/${key}/m`);
    expect(fullUrl(42, key, "scheme")).toBe(`owlbum://full/42?v=${key}`);
    expect(originalUrl(7, key, "scheme")).toBe(`owlbum://original/7?v=${key}`);
    expect(videoUrl(3, "scheme")).toBe("owlbum://video/3");
  });

  it("use http://owlbum.localhost on Windows", () => {
    expect(thumbUrl(key, "s", "windows")).toBe(`http://owlbum.localhost/thumb/${key}/s`);
    expect(fullUrl(42, key, "windows")).toBe(`http://owlbum.localhost/full/42?v=${key}`);
    expect(originalUrl(7, key, "windows")).toBe(`http://owlbum.localhost/original/7?v=${key}`);
    expect(videoUrl(3, "windows")).toBe("http://owlbum.localhost/video/3");
  });

  it("tell the platform from the webview's user agent", () => {
    const webview2 =
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
    const webkitgtk = "Mozilla/5.0 (X11; Ubuntu; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";
    expect(mediaPlatform(webview2)).toBe("windows");
    expect(mediaPlatform(webkitgtk)).toBe("scheme");
  });

  it("are not commands: the fake backend needs no handler for them", () => {
    const fake = createFakeApi();
    expect(thumbUrl(key, "s", "scheme")).toContain(key);
    expect(fake.calls).toEqual([]);
  });
});
