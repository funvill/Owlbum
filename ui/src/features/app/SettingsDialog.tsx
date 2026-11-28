import { useEffect, useState, type KeyboardEvent, type ReactNode } from "react";
import { api, toAppError } from "../../api";
import type { CacheUsage } from "../../api/bindings/CacheUsage";
import type { SettingsPatch } from "../../api";
import { Button, Checkbox, Dialog, RadioGroup, TextField, showToast } from "../../controls";
import { settingsText as t } from "../../text/app";
import { formatSize } from "../details/facts";
import { refreshEditStatus } from "../edit/save";
import { useSettingsStore } from "./settings";
import { useSettingsOpen } from "./settingsOpen";
import { TaggingSettings } from "./TaggingSettings";
import "./SettingsDialog.css";

/** The cache limit's range, in whole GB (docs/08, "Settings dialog"). */
export const CACHE_LIMIT = { min: 1, max: 1000 } as const;

/** The cache limit typed, as a whole number in range, or null. */
export function parseLimit(text: string): number | null {
  if (!/^\s*\d+\s*$/.test(text)) return null;
  const n = Number(text);
  return n >= CACHE_LIMIT.min && n <= CACHE_LIMIT.max ? n : null;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** "today at 09:14", or "27 Nov 2026 at 09:14", or "never". */
export function lastChecked(ms: number | null, now: Date = new Date()): string {
  if (ms === null) return t.never;
  const at = new Date(ms);
  const time = `${pad(at.getHours())}:${pad(at.getMinutes())}`;
  if (at.toDateString() === now.toDateString()) return t.today(time);
  const day = at.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
  return t.onDay(day, time);
}

/** Saves a change; a refused one shows "Couldn't keep the setting: <reason>". */
async function save(patch: SettingsPatch): Promise<boolean> {
  const store = useSettingsStore.getState();
  await store.update(patch);
  const error = useSettingsStore.getState().error;
  if (error !== null) {
    showToast({ kind: "error", message: t.couldNotKeep(error.message) });
    return false;
  }
  return true;
}

function Section({ title, id, children }: { title: string; id?: string; children: ReactNode }) {
  return (
    <section id={id} className="ow-settings__section" aria-label={title}>
      <h3 className="ow-settings__heading">{title}</h3>
      {children}
    </section>
  );
}

/** The thumbnail cache: its use, its limit and Clear cache. */
function CacheControls({ limitGb }: { limitGb: number }) {
  const [usage, setUsage] = useState<CacheUsage | null>(null);
  const [limit, setLimit] = useState(String(limitGb));
  const [limitError, setLimitError] = useState<string | undefined>(undefined);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  const readUsage = () =>
    api()
      .invoke("app_cache_usage")
      .then(setUsage, (e: unknown) => showToast({ kind: "error", message: toAppError(e).message }));
  // Read when the dialog opens; the parent keys this on the limit, so a new
  // limit starts it afresh.
  useEffect(() => {
    void readUsage();
  }, []);

  const commit = async () => {
    const n = parseLimit(limit);
    if (n === null) {
      setLimit(String(limitGb));
      setLimitError(t.cacheRange);
      return;
    }
    setLimitError(undefined);
    if (n === limitGb) return;
    if (await save({ "cache.limit_gb": n })) void readUsage();
    else setLimit(String(limitGb));
  };

  const clear = async () => {
    setBusy(true);
    try {
      const done = await api().invoke("app_cache_clear");
      showToast({
        kind: "info",
        message: t.cleared(formatSize(done.bytes_freed)),
      });
    } catch (e) {
      showToast({ kind: "error", message: toAppError(e).message });
    } finally {
      setBusy(false);
      setConfirming(false);
      void readUsage();
    }
  };

  const used = usage === null ? t.cacheUnknown : formatSize(usage.bytes);
  return (
    <div className="ow-settings__cache">
      <div className="ow-settings__cache-row">
        <span className="ow-settings__cache-use">{t.cacheUses(used)}</span>
        <TextField
          label={t.cacheLimit}
          hideLabel
          type="number"
          className="ow-settings__limit"
          min={CACHE_LIMIT.min}
          max={CACHE_LIMIT.max}
          value={limit}
          error={limitError}
          onChange={setLimit}
          onBlur={() => void commit()}
          onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
            if (e.key === "Enter") void commit();
          }}
        />
        <span>{t.cacheUnit}</span>
        <Button onClick={() => setConfirming(true)}>{t.clearCache}</Button>
      </div>
      <p className="ow-settings__note">{t.cacheNote}</p>
      {confirming && (
        <div className="ow-settings__confirm" role="group" aria-label={t.clearCache}>
          <span>{t.clearConfirm(used)}</span>
          <Button data-autofocus onClick={() => setConfirming(false)}>
            {t.clearCancel}
          </Button>
          <Button danger busy={busy} onClick={() => void clear()}>
            {t.clear}
          </Button>
        </div>
      )}
    </div>
  );
}

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * The Settings dialog (docs/08-desktop-and-release.md, "Settings dialog").
 * Every change is saved at once through `app_settings_set`; there is no
 * Save button. Nothing here writes to the album, so it all works in a
 * view-only album.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  return (
    <Dialog
      open={open}
      title={t.title}
      size="wide"
      onClose={onClose}
      actions={<Button onClick={onClose}>{t.close}</Button>}
    >
      <SettingsBody />
    </Dialog>
  );
}

function SettingsBody() {
  const settings = useSettingsStore((s) => s.settings);
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    void useSettingsStore.getState().load();
    api()
      .invoke("app_version")
      .then((v) => setVersion(v.version))
      .catch(() => setVersion(null));
  }, []);
  const section = useSettingsOpen((s) => s.section);
  const shown = settings !== null;
  useEffect(() => {
    // Opened at a section: bring it into view.
    if (shown && section !== null) document.getElementById(`ow-settings-${section}`)?.scrollIntoView?.({ block: "start" });
  }, [shown, section]);
  if (settings === null) return null;

  return (
    <div className="ow-settings">
      <Section title={t.general}>
        <Checkbox
          label={t.reopenLast}
          checked={settings.reopen_last}
          onChange={(on) => void save({ reopen_last: on })}
        />
        <h4 className="ow-settings__subheading">{t.cache}</h4>
        <CacheControls key={settings["cache.limit_gb"]} limitGb={settings["cache.limit_gb"]} />
        <RadioGroup
          label={t.editTarget}
          value={settings.edit_target}
          options={[
            {
              value: "album",
              label: t.editAlbum,
              description: t.editAlbumNote,
            },
            { value: "file", label: t.editFile, description: t.editFileNote },
          ]}
          onChange={(v) => {
            if (v !== "album" && v !== "file") return;
            void save({ edit_target: v }).then((ok) => {
              // What waits to be saved depends on the setting.
              if (ok) void refreshEditStatus();
            });
          }}
        />
      </Section>
      <Section title={t.maps}>
        <Checkbox
          label={t.mapsOnline}
          description={t.mapsNote}
          checked={settings.maps_online === true}
          onChange={(on) => void save({ maps_online: on })}
        />
      </Section>
      <Section title={t.tagging} id="ow-settings-tagging">
        <TaggingSettings />
      </Section>
      <Section title={t.updates}>
        <Checkbox
          label={t.updatesAuto}
          checked={settings["updates.auto"]}
          onChange={(on) => void save({ "updates.auto": on })}
        />
        <p className="ow-settings__note">{t.versionLine(version ?? "", lastChecked(settings["updates.last_check"]))}</p>
      </Section>
    </div>
  );
}
