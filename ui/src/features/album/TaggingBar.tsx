import { useEffect } from "react";
import { api } from "../../api";
import { Button, IconButton, ProgressBar } from "../../controls";
import { taggingBarText as t } from "../../text/album";
import { openSettings } from "../app/settingsOpen";
import { useAlbumStore } from "./session";
import { countsKey, hideTaggingLine, startTagging, taggingAction, useTaggingStore } from "./tagging";

const restart = () => void taggingAction(() => api().invoke("tagging_restart"));
const retryFailed = () => void taggingAction(() => api().invoke("tagging_retry_failed"));
const pause = (paused: boolean) => () => void taggingAction(() => api().invoke("tagging_pause", { paused }));

/**
 * One line under the top bar while an album is open, driven by
 * `tagging_progress` alone (docs/07-tagging.md, "The tagging bar"). The
 * first row of the spec's table that matches wins.
 */
export function TaggingBar() {
  const open = useAlbumStore((s) => s.status !== null);
  const p = useTaggingStore((s) => s.progress);
  const hiddenAt = useTaggingStore((s) => s.hiddenAt);
  useEffect(() => {
    void startTagging();
  }, []);
  if (!open || p === null || p.total === 0) return null;

  const seen = p.done + p.failed;
  if (p.problem !== null && !p.running) {
    return (
      <div className="ow-tagging-bar ow-tagging-bar--error" role="status">
        <span className="ow-tagging-bar__text">{t.stopped(p.problem)}</span>
        <Button size="sm" onClick={restart}>
          {t.tryAgain}
        </Button>
      </div>
    );
  }
  if (p.not_working.length > 0 && !p.running) {
    return (
      <div className="ow-tagging-bar ow-tagging-bar--error" role="status">
        <span className="ow-tagging-bar__text">{t.notWorking}</span>
        <Button size="sm" onClick={() => openSettings("tagging")}>
          {t.settings}
        </Button>
        <Button size="sm" onClick={restart}>
          {t.tryAgain}
        </Button>
      </div>
    );
  }
  const paused = !p.running && p.paused && seen < p.total;
  if (p.running || paused) {
    const label =
      t.progress(seen, p.total) + (p.failed > 0 ? t.failedPart(p.failed) : "") + (paused ? t.pausedPart : "");
    return (
      <div className="ow-tagging-bar">
        <div className="ow-tagging-bar__progress">
          <ProgressBar label={label} name={t.label} value={seen} max={p.total} />
        </div>
        <Button size="sm" onClick={pause(!paused)}>
          {paused ? t.resume : t.pause}
        </Button>
      </div>
    );
  }
  if (seen === p.total && p.failed > 0 && hiddenAt !== countsKey(p)) {
    return (
      <div className="ow-tagging-bar" role="status">
        <span className="ow-tagging-bar__text">{t.couldNotTag(p.failed)}</span>
        <Button size="sm" onClick={retryFailed}>
          {t.retryFailed}
        </Button>
        <IconButton icon="x" label={t.hide} onClick={hideTaggingLine} />
      </div>
    );
  }
  return null;
}
