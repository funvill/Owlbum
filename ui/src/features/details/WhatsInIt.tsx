import { api } from "../../api";
import type { Filter } from "../../api/bindings/Filter";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import type { PhotoTags } from "../../api/bindings/PhotoTags";
import type { TagKind } from "../../api/bindings/TagKind";
import { Button, Tooltip } from "../../controls";
import { whatsInItText as t } from "../../text/details";
import { taggingAction, useTaggingStore } from "../album/tagging";
import { openSettings } from "../app/settingsOpen";
import { useGridStore } from "../grid";
import { regenerate, useRegenerateStore } from "./regenerate";
import "./WhatsInIt.css";

/** Whether the current query filters by this label. Both sides are lower case. */
function filtering(filter: Filter, kind: TagKind, label: string): boolean {
  return (filter.tags ?? []).some((f) => f.kind === kind && f.value.toLowerCase() === label.toLowerCase());
}

/** Adds the keyword or object filter, or takes it away. */
function toggleFilter(kind: TagKind, label: string): void {
  const grid = useGridStore.getState();
  const filter = grid.query.filter;
  const tags = filter.tags ?? [];
  const next = filtering(filter, kind, label)
    ? tags.filter((f) => !(f.kind === kind && f.value.toLowerCase() === label.toLowerCase()))
    : [...tags, { kind, value: label }];
  const out: Filter = { ...filter, tags: next };
  if (next.length === 0) delete out.tags;
  grid.setQuery({ ...grid.query, filter: out });
}

/** A chip that toggles a keyword or object filter. */
function TagChip({ kind, label, text }: { kind: TagKind; label: string; text: string }) {
  const on = useGridStore((s) => filtering(s.query.filter, kind, label));
  return (
    <Tooltip text={on ? t.filterOff : t.filterOn}>
      <button
        type="button"
        className={`ow-chip ow-chip--button ${on ? "ow-chip--active" : "ow-chip--plain"}`}
        aria-pressed={on}
        onClick={() => toggleFilter(kind, label)}
      >
        {text}
      </button>
    </Tooltip>
  );
}

interface WhatsInItProps {
  detail: PhotoDetail;
  /** Why changes are off (view-only), or null. */
  disabledReason: string | null;
}

/**
 * The "What's in it" section of the details panel (docs/07-tagging.md,
 * "\"What's in it\" in the details panel"): the description, Regenerate,
 * and the keywords and objects as chips that filter the grid.
 */
export function WhatsInIt({ detail, disabledReason }: WhatsInItProps) {
  const notWorking = useTaggingStore((s) => (s.progress?.not_working.length ?? 0) > 0);
  const busy = useRegenerateStore((s) => s.busy.includes(detail.id));
  const reply = useRegenerateStore((s) => s.replies[detail.id]);
  // A reply replaces the tags until a newer detail arrives.
  const tags: PhotoTags | null = reply !== undefined && reply.revision >= detail.revision ? reply.tags : detail.tags;
  if (tags === null) return null;

  if (tags.state === "off") {
    return (
      <div className="ow-whatsinit">
        <p className="ow-details__muted">
          {t.off}{" "}
          <button type="button" className="ow-whatsinit__link" onClick={() => openSettings("tagging")}>
            {t.settings}
          </button>
        </p>
      </div>
    );
  }

  const nothing = tags.description === null && tags.keywords.length === 0 && tags.objects.length === 0;
  const writer = tags.written_by;
  return (
    <div className="ow-whatsinit">
      {notWorking && <p className="ow-whatsinit__error">{t.notWorking}</p>}
      {tags.state === "failed" && (
        <p className="ow-whatsinit__error">
          <Tooltip text={tags.error ?? t.couldNotLook}>
            <span tabIndex={0}>{t.couldNotLook}</span>
          </Tooltip>{" "}
          <button
            type="button"
            className="ow-whatsinit__link"
            onClick={() => void taggingAction(() => api().invoke("tagging_retry_failed"))}
          >
            {t.tryAgain}
          </button>
        </p>
      )}
      {tags.state === "waiting" && nothing && <p className="ow-details__muted">{t.notLookedAt}</p>}
      {tags.description !== null && (
        <Tooltip text={t.writtenBy(writer ?? "")}>
          <p className="ow-whatsinit__description" tabIndex={0}>
            {tags.description}
          </p>
        </Tooltip>
      )}
      {(tags.can_regenerate || (disabledReason !== null && tags.description !== null)) && (
        <div>
          {disabledReason !== null ? (
            <Button size="sm" aria-label={t.regenerateLabel} disabledReason={disabledReason}>
              {t.regenerate}
            </Button>
          ) : (
            <Tooltip text={t.regenerateTooltip(writer ?? "")}>
              <Button
                size="sm"
                aria-label={t.regenerateLabel}
                busy={busy}
                onClick={() => void regenerate(detail.id, detail.revision)}
              >
                {busy ? t.regenerating : t.regenerate}
              </Button>
            </Tooltip>
          )}
        </div>
      )}
      {tags.keywords.length > 0 && (
        <>
          <p className="ow-whatsinit__label">{t.keywords}</p>
          <div className="ow-whatsinit__chips">
            {tags.keywords.map((k) => (
              <TagChip key={k} kind="keyword" label={k} text={k} />
            ))}
          </div>
        </>
      )}
      {tags.objects.length > 0 && (
        <>
          <p className="ow-whatsinit__label">{t.objects}</p>
          <div className="ow-whatsinit__chips">
            {tags.objects.map((o) => (
              <TagChip key={o.label} kind="object" label={o.label} text={t.count(o.label, o.count)} />
            ))}
          </div>
        </>
      )}
      {tags.state === "waiting" && !nothing && tags.waiting_for !== null && (
        <p className="ow-details__muted">{t.waitingFor(tags.waiting_for)}</p>
      )}
    </div>
  );
}
