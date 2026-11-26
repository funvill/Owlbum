import type { AppCommands } from "../bindings/app";
import type { FormatInfo } from "../bindings/FormatInfo";
import type { AreaHandlers } from "./types";

/**
 * A few rows of the media format registry (`owlbum_core::media::FORMATS`),
 * enough for page tests: one editable still, one animated still, one RAW
 * and one video. The real list comes from Rust; the page never hard-codes it.
 */
export const fakeFormats: FormatInfo[] = [
  {
    id: "jpeg",
    extensions: [".jpg", ".jpeg", ".jpe", ".jfif"],
    kind: "image",
    label: "JPEG",
    edit_in_place: true,
    alpha: false,
    save_as: "jpeg",
    animated: false,
  },
  {
    id: "gif",
    extensions: [".gif"],
    kind: "image",
    label: "GIF",
    edit_in_place: false,
    alpha: true,
    save_as: "png",
    animated: true,
  },
  {
    id: "cr3",
    extensions: [".cr3"],
    kind: "raw",
    label: "Canon RAW",
    edit_in_place: false,
    alpha: false,
    save_as: "jpeg",
    animated: false,
  },
  {
    id: "mp4",
    extensions: [".mp4", ".m4v"],
    kind: "video",
    label: "MP4 video",
    edit_in_place: false,
    alpha: false,
    save_as: "jpeg",
    animated: false,
  },
];

/** The fake backend's `app_` commands. */
export const appHandlers: AreaHandlers<AppCommands> = {
  app_formats: () => fakeFormats.map((f) => ({ ...f, extensions: [...f.extensions] })),
  app_version: () => ({ version: "0.0.0-fake" }),
};
