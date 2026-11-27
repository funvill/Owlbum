import { Fragment } from "react";
import { Button, Dialog } from "../../controls";
import { parseKey, type KeyRow, type Place } from "../../keys";
import { keyCapText, keyLabels, shortcutsText as t } from "../../text/keys";
import { openSettings } from "./settingsOpen";
import { closeShortcuts, sections, useShortcutsStore, type Where } from "./shortcuts";
import "./ShortcutOverlay.css";

interface ShortcutOverlayProps {
  /** Where the user is: its list comes first. */
  here: Where;
  /** What the grid shows, which names the first heading in the timeline or map. */
  view?: "grid" | "timeline" | "map";
}

/** The heading of a place's list. */
function heading(place: Place, view: ShortcutOverlayProps["view"]): string {
  if (place === "browse" && (view === "timeline" || view === "map")) return t.places[view];
  return t.places[place];
}

/** One key, as key caps: `Ctrl+O` is two caps. */
function KeyCaps({ text }: { text: string }) {
  const spec = parseKey(text);
  const parts = [...(spec.ctrl ? ["Ctrl"] : []), ...(spec.shift ? ["Shift"] : []), ...(spec.alt ? ["Alt"] : [])];
  parts.push(keyCapText[spec.key] ?? spec.key);
  return (
    <span className="ow-shortcuts__combo">
      {parts.map((p, i) => (
        <kbd key={i} className="ow-shortcuts__cap">
          {p}
        </kbd>
      ))}
    </span>
  );
}

function Row({ row }: { row: KeyRow }) {
  return (
    <>
      <dt className="ow-shortcuts__keys">
        {row.hold && <span className="ow-shortcuts__hold">{t.hold}</span>}
        {row.keys.map((k) => (
          <KeyCaps key={k} text={k} />
        ))}
      </dt>
      <dd className="ow-shortcuts__label">{keyLabels[row.id]}</dd>
    </>
  );
}

/**
 * The keyboard shortcut overlay (docs/08-desktop-and-release.md, "The
 * overlay"): every row of the key table, grouped by place, the place the
 * user is in first. It is read-only.
 */
export function ShortcutOverlay({ here, view }: ShortcutOverlayProps) {
  const open = useShortcutsStore((s) => s.open);
  return (
    <Dialog
      open={open}
      title={t.title}
      size="wide"
      onClose={closeShortcuts}
      actions={
        <Button
          icon="settings"
          onClick={() => {
            closeShortcuts();
            openSettings();
          }}
        >
          {t.settings}
        </Button>
      }
    >
      {/* Focus starts on the list, not the close button, so its tooltip
          does not take the first Esc. */}
      <div className="ow-shortcuts" tabIndex={-1} data-autofocus>
        {sections(here).map((s) => (
          <section key={s.place} className="ow-shortcuts__section" aria-label={heading(s.place, view)}>
            <h3 className="ow-shortcuts__heading">
              {heading(s.place, view)}
              {s.here && (
                <Fragment>
                  {" "}
                  <span className="ow-shortcuts__here">{t.whereYouAre}</span>
                </Fragment>
              )}
            </h3>
            <dl className="ow-shortcuts__list">
              {s.rows.map((r) => (
                <Row key={r.id} row={r} />
              ))}
            </dl>
          </section>
        ))}
      </div>
      <p className="ow-shortcuts__note">{t.readOnly}</p>
    </Dialog>
  );
}
