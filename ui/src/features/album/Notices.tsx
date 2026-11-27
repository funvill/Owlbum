// The notice strip and the Files not shown dialog (docs/04-browsing.md,
// "Notices"): how many files the album leaves out, and which.

import { useEffect, useState } from "react";
import { api, toAppError } from "../../api";
import { Banner, Button, Dialog, showToast } from "../../controls";
import { albumText as t } from "../../text/album";
import { noticeKey, useAlbumStore } from "./session";
import "./Notices.css";

/** The most items `album_unsupported` and `album_scan_errors` give in one page. */
const PAGE = 500;

/** One file left out, as the dialog lists it. */
export interface NotShown {
  rel_path: string;
  reason: string;
}

/** Every item of a paged list, a page at a time until `total`. */
async function allPages<T>(
  read: (offset: number) => Promise<{ total: number; items: T[] }>,
): Promise<T[]> {
  const items: T[] = [];
  for (;;) {
    const page = await read(items.length);
    items.push(...page.items);
    if (page.items.length === 0 || items.length >= page.total) return items;
  }
}

/** The unsupported files, then the files that could not be read, each in walker order. */
export async function readNotShown(): Promise<NotShown[]> {
  const unsupported = await allPages((offset) =>
    api().invoke("album_unsupported", { offset, limit: PAGE }),
  );
  const unreadable = await allPages((offset) =>
    api().invoke("album_scan_errors", { offset, limit: PAGE }),
  );
  return [
    ...unsupported.map((u) => ({ rel_path: u.rel_path, reason: u.reason })),
    ...unreadable.map((e) => ({ rel_path: e.rel_path, reason: e.message })),
  ];
}

/** The notice strip: one link per kind of file left out, and an x. */
export function Notices() {
  const status = useAlbumStore((s) => s.status);
  const hidden = useAlbumStore((s) => s.noticesHidden);
  const hideNotices = useAlbumStore((s) => s.hideNotices);
  const [open, setOpen] = useState(false);
  if (status === null) return null;
  const { unsupported, unreadable } = status;
  const shown =
    (unsupported.count > 0 || unreadable > 0) && hidden !== noticeKey(status);

  const links: string[] = [];
  if (unsupported.count > 0)
    links.push(t.unsupportedNotice(unsupported.count, unsupported.labels));
  if (unreadable > 0) links.push(t.unreadableNotice(unreadable));

  return (
    <>
      {shown && (
        <Banner tone="info" onDismiss={hideNotices}>
          {links.map((text, i) => (
            <span key={text}>
              {i > 0 && t.noticeSeparator}
              <button
                type="button"
                className="ow-notices__link"
                onClick={() => setOpen(true)}
              >
                {text}
              </button>
            </span>
          ))}
        </Banner>
      )}
      <FilesNotShown open={open} onClose={() => setOpen(false)} />
    </>
  );
}

/** The wide Files not shown dialog: each file and why, Copy list and Close. */
export function FilesNotShown({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  return open ? <FilesNotShownBody onClose={onClose} /> : null;
}

function FilesNotShownBody({ onClose }: { onClose: () => void }) {
  const revision = useAlbumStore((s) =>
    s.status === null ? null : noticeKey(s.status),
  );
  const [items, setItems] = useState<NotShown[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [copying, setCopying] = useState(false);

  // Read when the dialog opens, and again when the numbers change.
  useEffect(() => {
    let current = true;
    readNotShown().then(
      (list) => {
        if (current) setItems(list);
      },
      (e: unknown) => {
        if (current) setFailed(toAppError(e).message);
      },
    );
    return () => {
      current = false;
    };
  }, [revision]);

  const copy = async () => {
    setCopying(true);
    try {
      await api().invoke("album_copy_not_shown");
      showToast({ kind: "info", message: t.listCopied });
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
    } finally {
      setCopying(false);
    }
  };

  return (
    <Dialog
      open
      size="wide"
      title={t.notShownTitle}
      onClose={onClose}
      actions={
        <>
          <Button
            busy={copying}
            disabledReason={
              items === null || items.length === 0 ? t.notShownEmpty : undefined
            }
            onClick={() => void copy()}
          >
            {t.copyList}
          </Button>
          <Button data-autofocus onClick={onClose}>
            {t.close}
          </Button>
        </>
      }
    >
      {failed !== null && <p>{failed}</p>}
      {failed === null && items === null && <p>{t.notShownLoading}</p>}
      {items !== null && items.length === 0 && <p>{t.notShownEmpty}</p>}
      {items !== null && items.length > 0 && (
        <ul className="ow-notshown__list" aria-label={t.notShownTitle}>
          {items.map((item, i) => (
            <li key={`${i}:${item.rel_path}`} className="ow-notshown__item">
              <span className="ow-notshown__path">{item.rel_path}</span>
              <span className="ow-notshown__reason">{item.reason}</span>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}
