// Addresses of pictures served by the `owlbum://` protocol
// (docs/01-architecture.md, "The `owlbum://` protocol"). Nothing else in the
// page builds one: a component asks here, so no component cares which form
// the platform needs.

/**
 * How the webview reaches a custom scheme: WebView2 on Windows addresses it
 * as `http://owlbum.localhost/…`; WebKitGTK takes `owlbum://…` as it is.
 */
export type MediaPlatform = "windows" | "scheme";

/** The platform form for this webview, read from its user agent. */
export function mediaPlatform(userAgent: string = globalThis.navigator?.userAgent ?? ""): MediaPlatform {
  return /\bWindows\b/.test(userAgent) ? "windows" : "scheme";
}

function base(platform: MediaPlatform): string {
  return platform === "windows" ? "http://owlbum.localhost/" : "owlbum://";
}

/** A thumbnail: `s` is 512 px, `m` is 1600 px on the longest side. Immutable. */
export function thumbUrl(thumbKey: string, size: "s" | "m", platform: MediaPlatform = mediaPlatform()): string {
  return `${base(platform)}thumb/${thumbKey}/${size}`;
}

/**
 * The full-size upright picture for zooming. `thumbKey` only makes the
 * webview drop its copy when the file changes.
 */
export function fullUrl(photoId: number, thumbKey: string, platform: MediaPlatform = mediaPlatform()): string {
  return `${base(platform)}full/${photoId}?v=${thumbKey}`;
}

/** The file's own bytes, for an animated GIF or WebP only. */
export function originalUrl(photoId: number, thumbKey: string, platform: MediaPlatform = mediaPlatform()): string {
  return `${base(platform)}original/${photoId}?v=${thumbKey}`;
}

/** A video, for a `<video>` element. Playback arrives with roadmap #50. */
export function videoUrl(photoId: number, platform: MediaPlatform = mediaPlatform()): string {
  return `${base(platform)}video/${photoId}`;
}
