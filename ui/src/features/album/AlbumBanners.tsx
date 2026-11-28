import { Banner, Button } from "../../controls";
import { albumText } from "../../text/album";
import { viewOnlyBanner, type BannerAction } from "./banners";
import { useAlbumStore } from "./session";

/** A banner button's label, read when drawn so it is in the language in use. */
function actionLabel(action: BannerAction): string {
  switch (action) {
    case "try_again":
      return albumText.tryAgain;
    case "open_for_changes":
    case "take_lock":
      return albumText.openForChanges;
    case "restore":
      return albumText.restoreBackupEllipsis;
  }
}

/** The album banners under the top bar (docs/04-browsing.md, "Album banners"). */
export function AlbumBanners() {
  const status = useAlbumStore((s) => s.status);
  const conflictHidden = useAlbumStore((s) => s.conflictHidden);
  const open = useAlbumStore((s) => s.open);
  const askRestore = useAlbumStore((s) => s.askRestore);
  const hideConflict = useAlbumStore((s) => s.hideConflict);
  if (status === null) return null;
  const banner = viewOnlyBanner(status);

  const act = (action: BannerAction) => {
    switch (action) {
      case "try_again":
      case "open_for_changes":
        void open(status.folder);
        return;
      case "take_lock":
        void open(status.folder, { take_lock: true });
        return;
      case "restore":
        void askRestore();
        return;
    }
  };

  return (
    <>
      {banner !== null && (
        <Banner
          tone={banner.tone}
          actions={
            banner.action === null ? undefined : (
              <Button size="sm" onClick={() => act(banner.action ?? "try_again")}>
                {actionLabel(banner.action)}
              </Button>
            )
          }
        >
          {banner.text}
        </Banner>
      )}
      {status.conflict_file !== null && !conflictHidden && (
        <Banner tone="warn" onDismiss={hideConflict}>
          {albumText.conflict(status.conflict_file)}
        </Banner>
      )}
    </>
  );
}
