import { useEffect, type ReactNode } from "react";
import { api } from "../../api";
import type { TaggingProgress } from "../../api/bindings/TaggingProgress";
import { Button, IconButton, ProgressBar, Tooltip } from "../../controls";
import { taggingBarText as t } from "../../text/album";
import { openSettings } from "../app/settingsOpen";
import { useAlbumStore } from "./session";
import {
  countsKey,
  hideTaggingLine,
  secondsLeft,
  startTagging,
  taggingAction,
  useTaggingStore,
  type TaggingState,
} from "./tagging";

const restart = () => void taggingAction(() => api().invoke("tagging_restart"));
const retryFailed = () => void taggingAction(() => api().invoke("tagging_retry_failed"));
const pause = (paused: boolean) => () => void taggingAction(() => api().invoke("tagging_pause", { paused }));

/**
 * What the bar is and what it is doing now, for its tooltip and accessible
 * description (owner's feedback, issue 198): "Photo tagging: describing
 * photos with Florence-2 base, 120 of 2,000 done, about 15 minutes left",
 * or why it is paused or stopped.
 */
export function taggingAbout(p: TaggingProgress, models: TaggingState["models"], since: TaggingState["since"]): string {
  const a = t.about;
  const seen = p.done + p.failed;
  if (p.problem !== null && !p.running) return a.stopped(p.problem);
  if (p.not_working.length > 0 && !p.running) return a.notWorking;
  if (!p.running && p.paused && seen < p.total) return a.paused(seen, p.total);
  if (!p.running) return a.finished(p.failed);
  const parts = [
    ...(models?.describe != null ? [a.describing(models.describe)] : []),
    ...(models?.keywords != null ? [a.keywords(models.keywords)] : []),
  ];
  const doing = parts.length > 0 ? parts.join(" and ") : a.tagging;
  const left = secondsLeft(p, since);
  return a.running(doing, seen, p.total) + (p.failed > 0 ? a.failed(p.failed) : "") + (left === null ? "" : a.left(left));
}

/** One line of the bar, with its tooltip and the same words as its description. */
function Line({
  about,
  error = false,
  status = true,
  children,
}: {
  about: string;
  error?: boolean;
  /** A line of words is announced and described; the moving bar describes its progress bar instead. */
  status?: boolean;
  children: ReactNode;
}) {
  return (
    <Tooltip text={about} describe={false}>
      <div
        className={`ow-tagging-bar${error ? " ow-tagging-bar--error" : ""}`}
        role={status ? "status" : undefined}
        aria-description={status ? about : undefined}
      >
        {children}
      </div>
    </Tooltip>
  );
}

/**
 * One line under the top bar while an album is open, driven by
 * `tagging_progress` alone (docs/07-tagging.md, "The tagging bar"). The
 * first row of the spec's table that matches wins. Pointing at it shows
 * what it is and what it is doing (issue 198).
 */
export function TaggingBar() {
  const open = useAlbumStore((s) => s.status !== null);
  const p = useTaggingStore((s) => s.progress);
  const hiddenAt = useTaggingStore((s) => s.hiddenAt);
  const models = useTaggingStore((s) => s.models);
  const since = useTaggingStore((s) => s.since);
  useEffect(() => {
    void startTagging();
  }, []);
  if (!open || p === null || p.total === 0) return null;
  const about = taggingAbout(p, models, since);

  const seen = p.done + p.failed;
  if (p.problem !== null && !p.running) {
    const problem = p.problem;
    return (
      <Line about={about} error>
        <>
          <span className="ow-tagging-bar__text">{t.stopped(problem)}</span>
          <Button size="sm" onClick={restart}>
            {t.tryAgain}
          </Button>
        </>
      </Line>
    );
  }
  if (p.not_working.length > 0 && !p.running) {
    return (
      <Line about={about} error>
        <>
          <span className="ow-tagging-bar__text">{t.notWorking}</span>
          <Button size="sm" onClick={() => openSettings("tagging")}>
            {t.settings}
          </Button>
          <Button size="sm" onClick={restart}>
            {t.tryAgain}
          </Button>
        </>
      </Line>
    );
  }
  const paused = !p.running && p.paused && seen < p.total;
  if (p.running || paused) {
    const label =
      t.progress(seen, p.total) + (p.failed > 0 ? t.failedPart(p.failed) : "") + (paused ? t.pausedPart : "");
    return (
      <Line about={about} status={false}>
        <>
          <div className="ow-tagging-bar__progress">
            <ProgressBar label={label} name={t.label} value={seen} max={p.total} description={about} />
          </div>
          <Button size="sm" onClick={pause(!paused)}>
            {paused ? t.resume : t.pause}
          </Button>
        </>
      </Line>
    );
  }
  if (seen === p.total && p.failed > 0 && hiddenAt !== countsKey(p)) {
    return (
      <Line about={about}>
        <>
          <span className="ow-tagging-bar__text">{t.couldNotTag(p.failed)}</span>
          <Button size="sm" onClick={retryFailed}>
            {t.retryFailed}
          </Button>
          <IconButton icon="x" label={t.hide} onClick={hideTaggingLine} />
        </>
      </Line>
    );
  }
  return null;
}
