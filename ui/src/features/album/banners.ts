import type { AlbumStatus } from "../../api/bindings/AlbumStatus";
import { albumText } from "../../text/album";

/** What a view-only banner's button does. */
export type BannerAction = "try_again" | "restore" | "open_for_changes" | "take_lock";

/** One view-only banner (docs/04-browsing.md, "Album banners"). */
export interface ViewOnlyBanner {
  tone: "warn" | "error";
  text: string;
  action: BannerAction | null;
}

/**
 * The view-only banner for `status`, or null when the album can be
 * changed. The **Check for updates** button of `newer` comes with the
 * update check (#62).
 */
export function viewOnlyBanner(status: AlbumStatus): ViewOnlyBanner | null {
  const host = status.lock_host;
  switch (status.view_only) {
    case null:
      return null;
    case "locked":
      return { tone: "warn", text: albumText.viewOnlyLocked(host ?? ""), action: "try_again" };
    case "read_only":
      return {
        tone: "warn",
        text: status.in_memory ? albumText.viewOnlyInMemory : albumText.viewOnlyReadOnly,
        action: null,
      };
    case "newer":
      return { tone: "warn", text: albumText.viewOnlyNewer, action: null };
    case "damaged":
      return { tone: "error", text: albumText.viewOnlyDamaged, action: "restore" };
    case "chosen":
      return host === null
        ? { tone: "warn", text: albumText.viewOnlyChosen, action: "open_for_changes" }
        : { tone: "warn", text: albumText.viewOnlyChosenHost(host), action: "take_lock" };
  }
}

/**
 * The first sentence of the banner in force: the tooltip of the View only
 * badge, and of every control view-only disables.
 */
export function viewOnlyReason(status: AlbumStatus): string | null {
  const banner = viewOnlyBanner(status);
  if (banner === null) return null;
  const end = banner.text.indexOf(". ");
  return end === -1 ? banner.text : banner.text.slice(0, end + 1);
}

/** The top bar's path: the root, and the scope in the system's own separators. */
export function shownScope(status: AlbumStatus): string {
  return status.root.includes("\\") ? status.scope.replaceAll("/", "\\") : status.scope;
}
