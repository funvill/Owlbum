import { useEffect, useState } from "react";
import { api, toAppError } from "../../api";
import type { TidyFolder } from "../../api/bindings/TidyFolder";
import { Button, Checkbox, Dialog, Tooltip, showToast } from "../../controls";
import { albumText as t } from "../../text/album";
import { useGridStore } from "../grid";
import { useAlbumStore } from "./session";
import "./TidyDialog.css";

type Shown =
  | { kind: "loading" }
  | { kind: "scanning" }
  | { kind: "message"; text: string }
  | { kind: "list"; folders: TidyFolder[] };

/** The files ticked when the list arrives: those that would lose nothing typed, outside folders that are gone. */
export function defaultTicks(folders: readonly TidyFolder[]): Set<number> {
  const ticked = new Set<number>();
  for (const folder of folders) {
    if (folder.gone) continue;
    for (const file of folder.files) if (file.keeps.length === 0) ticked.add(file.id);
  }
  return ticked;
}

/**
 * The Tidy album dialog (docs/05-organising.md, "Tidy"). Rust lists the
 * missing rows and deletes the confirmed ones, after looking for each file
 * once more; this only shows the list and which rows are ticked.
 */
export function TidyDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  return open ? <TidyBody onClose={onClose} /> : null;
}

function TidyBody({ onClose }: { onClose: () => void }) {
  const scanning = useAlbumStore((s) => s.status?.scanning ?? false);
  const [shown, setShown] = useState<Shown>({ kind: "loading" });
  const [ticked, setTicked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);

  // Raised to read the list again after a failed removal.
  const [reads, setReads] = useState(0);

  // The list is read when the dialog opens, when a scan ends, and after a
  // failed removal.
  useEffect(() => {
    if (scanning) return;
    let current = true;
    api()
      .invoke("album_tidy_list")
      .then(
        (folders) => {
          if (!current) return;
          setTicked(defaultTicks(folders));
          setShown(folders.length === 0 ? { kind: "message", text: t.tidyNothing } : { kind: "list", folders });
        },
        (e: unknown) => {
          if (!current) return;
          const err = toAppError(e);
          setShown(err.code === "scan_running" ? { kind: "scanning" } : { kind: "message", text: err.message });
        },
      );
    return () => {
      current = false;
    };
  }, [scanning, reads]);

  const remove = async () => {
    setBusy(true);
    try {
      const done = await api().invoke("album_tidy_remove", { ids: [...ticked] });
      showToast({
        kind: "info",
        message: done.kept > 0 ? t.tidyRemovedKept(done.removed, done.kept) : t.tidyRemoved(done.removed),
      });
      onClose();
      // A photo found again is back in the album: show it.
      if (done.kept > 0) await useGridStore.getState().refresh();
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
      setBusy(false);
      setShown({ kind: "loading" });
      setReads((n) => n + 1);
    }
  };

  const setMany = (ids: readonly number[], on: boolean) =>
    setTicked((old) => {
      const next = new Set(old);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });

  // While a scan runs, "missing" cannot be trusted: no list is shown.
  const view: Shown = scanning ? { kind: "scanning" } : shown;
  const list = view.kind === "list";
  const count = ticked.size;
  return (
    <Dialog
      open
      size="wide"
      title={t.tidyTitle}
      onClose={onClose}
      actions={
        list ? (
          <>
            <Button data-autofocus onClick={onClose}>
              {t.cancel}
            </Button>
            <Button
              kind="primary"
              busy={busy}
              disabledReason={count === 0 ? t.tidyRemove(0) : undefined}
              onClick={() => void remove()}
            >
              {t.tidyRemove(count)}
            </Button>
          </>
        ) : (
          <Button data-autofocus onClick={onClose}>
            {t.close}
          </Button>
        )
      }
    >
      {view.kind === "loading" && <p>{t.tidyLooking}</p>}
      {view.kind === "scanning" && <p>{t.tidyScanning}</p>}
      {view.kind === "message" && <p>{view.text}</p>}
      {view.kind === "list" && (
        <>
          <p>{t.tidyExplain}</p>
          <ul className="ow-tidy__list" aria-label={t.tidyTitle}>
            {view.folders.map((folder) => {
              const ids = folder.files.map((f) => f.id);
              const on = ids.filter((id) => ticked.has(id)).length;
              const name = folder.folder === "" ? t.tidyTopFolder : folder.folder;
              return (
                <li key={folder.folder} className="ow-tidy__folder">
                  <div className="ow-tidy__folder-row">
                    <Checkbox
                      label={name}
                      checked={on === 0 ? false : on === ids.length ? true : "mixed"}
                      onChange={(c) => setMany(ids, c)}
                    />
                    {folder.gone && (
                      <Tooltip text={t.tidyFolderGoneTooltip}>
                        <span className="ow-tidy__gone" tabIndex={0}>
                          {t.tidyFolderGone}
                        </span>
                      </Tooltip>
                    )}
                    <span className="ow-tidy__count">{t.tidySelected(on, ids.length)}</span>
                  </div>
                  <ul className="ow-tidy__files">
                    {folder.files.map((file) => (
                      <li key={file.id}>
                        <Checkbox
                          label={file.file_name}
                          description={file.keeps.length > 0 ? file.keeps.join(" · ") : undefined}
                          checked={ticked.has(file.id)}
                          onChange={(c) => setMany([file.id], c)}
                        />
                      </li>
                    ))}
                  </ul>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </Dialog>
  );
}
