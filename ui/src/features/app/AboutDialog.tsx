import { useEffect, useState } from "react";
import { api, toAppError } from "../../api";
import type { About } from "../../api/bindings/About";
import ownLogo from "../../assets/owlbum-icon.png";
import { Button, Dialog, showToast } from "../../controls";
import { aboutText as t } from "../../text/app";
import { updateCommand, useLifecycle } from "./lifecycle";
import "./AboutDialog.css";

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
}

/** Opens one of Owlbum's fixed addresses or files; a failure is a toast. */
function openFixed(
  command: "app_open_repository" | "app_open_feedback" | "app_open_licence" | "app_open_notices",
): void {
  api()
    .invoke(command)
    .catch((e: unknown) => showToast({ kind: "error", message: toAppError(e).message }));
}

/**
 * The About box (docs/08-desktop-and-release.md, "About box"), opened by F1
 * or Help > About Owlbum: the logo, the name and version, what Owlbum is,
 * where its data is kept, and a link to the project on GitHub. The page
 * never opens an address itself: the link asks Rust to.
 */
export function AboutDialog({ open, onClose }: AboutDialogProps) {
  const canUpdate = useLifecycle((s) => s.update.state !== "unavailable");
  const [hasNotices, setHasNotices] = useState(false);
  return (
    <Dialog
      open={open}
      title={t.title}
      onClose={onClose}
      actions={
        <>
          {canUpdate && (
            <Button
              onClick={() => {
                onClose();
                updateCommand("app_check_updates");
              }}
            >
              {t.checkForUpdates}
            </Button>
          )}
          <Button onClick={() => openFixed("app_open_licence")}>{t.licence}</Button>
          {hasNotices && <Button onClick={() => openFixed("app_open_notices")}>{t.notices}</Button>}
          <Button onClick={() => openFixed("app_open_feedback")}>{t.feedback}</Button>
          <Button kind="primary" data-autofocus onClick={onClose}>
            {t.ok}
          </Button>
        </>
      }
    >
      <AboutBody onNotices={setHasNotices} />
    </Dialog>
  );
}

function AboutBody({ onNotices }: { onNotices: (has: boolean) => void }) {
  const [about, setAbout] = useState<About | null>(null);
  useEffect(() => {
    let live = true;
    api()
      .invoke("app_about")
      .then(
        (a) => {
          if (!live) return;
          setAbout(a);
          onNotices(a.has_notices);
        },
        (e: unknown) => showToast({ kind: "error", message: toAppError(e).message }),
      );
    return () => {
      live = false;
    };
  }, [onNotices]);

  return (
    <div className="ow-about">
      <div className="ow-about__head">
        <img src={ownLogo} alt="" width={64} height={64} />
        <div>
          <p className="ow-about__name">{about === null ? t.loading : t.name(about.version)}</p>
          {about !== null && (
            <p className="ow-about__version">{t.versionLine(about.version, about.platform, about.arch)}</p>
          )}
        </div>
      </div>
      <p>{t.description}</p>
      <p className="ow-about__muted">{t.network}</p>
      {about !== null && (
        <div>
          <p className="ow-about__muted">{t.dataFolder}</p>
          <p className="ow-about__folder">{about.data_dir}</p>
        </div>
      )}
      <div>
        <p className="ow-about__muted">{t.credits}</p>
        {about !== null && about.copyright !== "" && <p className="ow-about__muted">{about.copyright}</p>}
      </div>
      <button type="button" className="ow-about__link" onClick={() => openFixed("app_open_repository")}>
        {about?.repository ?? t.repository}
      </button>
    </div>
  );
}
