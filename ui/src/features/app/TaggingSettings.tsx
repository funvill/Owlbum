import { useEffect, useRef, useState } from "react";
import { api, toAppError } from "../../api";
import type { ModelInfo } from "../../api/bindings/ModelInfo";
import type { ModelsState } from "../../api/bindings/ModelsState";
import type { Part } from "../../api/bindings/Part";
import type { TaggingChoice } from "../../api/bindings/TaggingChoice";
import { Button, ProgressBar } from "../../controls";
import { taggingSettingsText as t } from "../../text/app";
import "./TaggingSettings.css";

const PARTS: readonly Part[] = ["describe", "keywords"];

/** A model's size: "1.3 GB" from 1,000,000,000 bytes, "277 MB" from 10 MB, "4.2 MB" below. */
export function modelSize(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  const mb = bytes / 1_000_000;
  return mb >= 10 ? `${Math.round(mb)} MB` : `${mb.toFixed(1)} MB`;
}

/** "less than a second a photo", or "about 6 s a photo". */
export function modelSpeed(seconds: number): string {
  return seconds < 1 ? t.lessThanASecond : t.aboutSeconds(Math.round(seconds));
}

/**
 * The Photo tagging section of Settings: the model choices, downloads and
 * removal (docs/07-tagging.md, "Settings → Photo tagging"). The Settings
 * dialog only gives it a place. It reads `tagging_models` when it opens and
 * again when a download ends; the progress bar follows `download_progress`.
 * Nothing here polls.
 */
export function TaggingSettings() {
  const [state, setState] = useState<ModelsState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<ModelInfo | null>(null);

  const read = () =>
    api()
      .invoke("tagging_models")
      .then((s) => {
        setState(s);
        setLoadError(null);
      })
      .catch((e: unknown) => setLoadError(toAppError(e).message));

  useEffect(() => {
    void read();
    let stop: (() => void) | undefined;
    let gone = false;
    void api()
      .listen("download_progress", (p) => {
        if (p.finished) {
          void read();
          return;
        }
        setState((s) =>
          s === null ? s : { ...s, downloading: { model: p.model, received: p.received, total: p.total }, error: null },
        );
      })
      .then((u) => {
        if (gone) u();
        else stop = u;
      });
    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  if (state === null) {
    return loadError === null ? (
      <p className="ow-settings__note">{t.loading}</p>
    ) : (
      <p className="ow-tagging-settings__error" role="alert">
        {loadError}
      </p>
    );
  }

  /** Runs an action; its error shows under the section. */
  const act = async (run: () => Promise<unknown>, reread = true) => {
    setActionError(null);
    let ok = true;
    try {
      await run();
    } catch (e) {
      ok = false;
      setActionError(toAppError(e).message);
    }
    if (reread) await read();
    return ok;
  };

  const choose = (part: Part, id: string | null) => {
    const before = state.settings;
    const next: TaggingChoice = { ...before, [part]: id };
    // Shown at once; the reply is the truth, so a refusal puts it back.
    setState({ ...state, settings: next });
    setActionError(null);
    api()
      .invoke("tagging_set", next)
      .then(
        (saved) => setState((s) => (s === null ? s : { ...s, settings: saved })),
        (e: unknown) => {
          setState((s) => (s === null ? s : { ...s, settings: before }));
          setActionError(toAppError(e).message);
        },
      );
  };

  const download = (m: ModelInfo) => {
    setConfirming(null);
    void act(() => api().invoke("tagging_download", { model: m.id }), false).then((ok) => {
      if (!ok) return;
      setState((s) =>
        s === null ? s : { ...s, downloading: { model: m.id, received: m.bytes_present, total: m.bytes }, error: null },
      );
    });
  };

  const error = actionError ?? state.error;
  return (
    <div className="ow-tagging-settings">
      <p className="ow-settings__note">{t.intro}</p>
      {PARTS.map((part) => (
        <PartGroup
          key={part}
          part={part}
          state={state}
          onChoose={(id) => choose(part, id)}
          onAsk={setConfirming}
          onCancel={() => void act(() => api().invoke("tagging_download_cancel"))}
          onRemove={(m) => void act(() => api().invoke("tagging_remove", { model: m.id }))}
        />
      ))}
      {confirming !== null && (
        <ConfirmStrip model={confirming} onCancel={() => setConfirming(null)} onDownload={() => download(confirming)} />
      )}
      {error !== null && (
        <p className="ow-tagging-settings__error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

interface PartGroupProps {
  part: Part;
  state: ModelsState;
  onChoose: (id: string | null) => void;
  onAsk: (m: ModelInfo) => void;
  onCancel: () => void;
  onRemove: (m: ModelInfo) => void;
}

/** One part: its title, note, Off and its models, smaller first. */
function PartGroup({ part, state, onChoose, onAsk, onCancel, onRemove }: PartGroupProps) {
  const words = t.parts[part];
  const chosen = state.settings[part];
  const models = state.models
    .filter((m) => m.part === part)
    .sort((a, b) => (a.size === b.size ? 0 : a.size === "smaller" ? -1 : 1));
  const name = `ow-tagging-${part}`;
  const titleId = `${name}-title`;
  return (
    <div role="radiogroup" aria-labelledby={titleId} className="ow-tagging-settings__part">
      <h4 id={titleId} className="ow-settings__subheading">
        {words.title}
      </h4>
      <p className="ow-settings__note">{words.note}</p>
      <div className="ow-tagging-settings__row">
        <Radio name={name} id={`${name}-off`} label={t.off} checked={chosen === null} onChange={() => onChoose(null)} />
      </div>
      {models.map((m) => (
        <div key={m.id} className="ow-tagging-settings__row">
          <Radio
            name={name}
            id={`${name}-${m.id}`}
            label={m.label}
            size={t.sizes[m.size]}
            description={t.details(modelSize(m.bytes), modelSpeed(m.seconds_per_photo), m.licence)}
            checked={chosen === m.id}
            disabled={m.state !== "ok"}
            onChange={() => onChoose(m.id)}
          />
          <div className="ow-tagging-settings__actions">
            <ModelActions
              model={m}
              state={state}
              onAsk={() => onAsk(m)}
              onCancel={onCancel}
              onRemove={() => onRemove(m)}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

interface RadioProps {
  name: string;
  id: string;
  label: string;
  size?: string;
  description?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: () => void;
}

/** One radio of a part, drawn like the shared radio buttons. */
function Radio({ name, id, label, size, description, checked, disabled, onChange }: RadioProps) {
  return (
    <label className="ow-check ow-check--radio">
      <span className="ow-check__box">
        <input
          type="radio"
          className="ow-check__input"
          name={name}
          checked={checked}
          disabled={disabled}
          aria-labelledby={`${id}-label`}
          aria-describedby={description !== undefined ? `${id}-description` : undefined}
          onChange={onChange}
        />
        <span className="ow-check__dot" />
      </span>
      <span className="ow-check__text">
        <span id={`${id}-label`} className="ow-check__label">
          {size === undefined ? (
            label
          ) : (
            <>
              <strong>{label}</strong> ({size})
            </>
          )}
        </span>
        {description !== undefined && (
          <span id={`${id}-description`} className="ow-check__description">
            {description}
          </span>
        )}
      </span>
    </label>
  );
}

interface ModelActionsProps {
  model: ModelInfo;
  state: ModelsState;
  onAsk: () => void;
  onCancel: () => void;
  onRemove: () => void;
}

/** What sits at the right of a model row (the table under "Settings → Photo tagging"). */
function ModelActions({ model, state, onAsk, onCancel, onRemove }: ModelActionsProps) {
  const running = state.downloading;
  if (running !== null && running.model === model.id) {
    const percent = running.total > 0 ? Math.floor((running.received / running.total) * 100) : 0;
    return (
      <>
        <div className="ow-tagging-settings__bar" role="group" aria-label={t.downloading(model.label)}>
          <ProgressBar label={t.percent(percent)} value={running.received} max={running.total} />
        </div>
        <Button size="sm" onClick={onCancel}>
          {t.cancel}
        </Button>
      </>
    );
  }
  const busy = running !== null ? t.busyDownloading : undefined;
  const chosen = state.settings[model.part] === model.id;
  const notWorking = chosen && state.not_working.includes(model.part);
  switch (model.state) {
    case "absent":
      return (
        <Button size="sm" disabledReason={busy} onClick={onAsk}>
          {t.download}
        </Button>
      );
    case "partial":
      return (
        <Button size="sm" disabledReason={busy} onClick={onAsk}>
          {t.continueDownload}
        </Button>
      );
    case "unverified":
    case "checking":
      return <span className="ow-settings__note">{t.checking}</span>;
    case "damaged":
      return (
        <Button size="sm" disabledReason={busy} onClick={onAsk}>
          {t.downloadAgain}
        </Button>
      );
    case "ok":
      return (
        <>
          {notWorking && (
            <Button size="sm" disabledReason={busy} onClick={onAsk}>
              {t.downloadAgain}
            </Button>
          )}
          <Button size="sm" danger onClick={onRemove}>
            {t.remove}
          </Button>
        </>
      );
  }
}

interface ConfirmStripProps {
  model: ModelInfo;
  onCancel: () => void;
  onDownload: () => void;
}

/** "Download {label}, {size}, from Hugging Face?" with Cancel and Download, which has the focus. */
function ConfirmStrip({ model, onCancel, onDownload }: ConfirmStripProps) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.querySelector<HTMLButtonElement>("[data-confirm]")?.focus();
  }, [model.id]);
  return (
    <div ref={box} className="ow-settings__confirm" role="group" aria-label={t.confirm(model.label, modelSize(model.bytes))}>
      <span>{t.confirm(model.label, modelSize(model.bytes))}</span>
      <Button onClick={onCancel}>{t.cancel}</Button>
      <Button kind="primary" data-confirm onClick={onDownload}>
        {t.download}
      </Button>
    </div>
  );
}
