// The typed-info fields of the details panel (docs/04-browsing.md, "How
// typed fields save"), one component per field group. The panel gives
// each the photo, its saver and, in a view-only album, the reason every
// field is disabled.
//
// Text is never lost: a draft is kept apart from the saved value and
// follows it only while it holds nothing unsaved, and a field that goes
// away (the panel moves to another photo, or closes) first saves a
// changed, valid value. A value on its way is remembered so it is not
// sent twice.

import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../../../api";
import type { PersonCount } from "../../../api/bindings/PersonCount";
import type { PhotoDetail } from "../../../api/bindings/PhotoDetail";
import { Chip, IconButton } from "../../../controls";
import { Icon } from "../../../icons";
import { detailsText as t } from "../../../text/details";
import {
  addNames,
  boxesFor,
  cleanText,
  dateOutcome,
  parseDate,
  locationOutcome,
  shownDate,
  splitAtCommas,
  suggestionsFor,
  MAX_NAME_CHARS,
} from "./rules";
import type { FieldKey, InfoSaver } from "./save";

export { useInfoSaver, sendInfo, SAVED_MS } from "./save";

export interface TypedProps {
  detail: PhotoDetail;
  /** Set in a view-only album: the reason every field is disabled. */
  disabledReason: string | null;
  saver: InfoSaver;
}

/** Runs the latest `fn` when the component goes away. */
function useOnLeave(fn: () => void) {
  const latest = useRef(fn);
  useEffect(() => {
    latest.current = fn;
  });
  useEffect(() => () => latest.current(), []);
}

/**
 * The value last sent, so the same value is not sent twice (a blur right
 * after Enter, or after leaving). Forgotten when the saved value changes
 * or a save fails.
 */
function useSent<T>(savedKey: string) {
  const sent = useRef<{ value: T } | null>(null);
  useEffect(() => {
    sent.current = null;
  }, [savedKey]);
  return {
    /** False when `value` was already sent; otherwise remembers it as sent. */
    once: (value: T) => {
      if (sent.current !== null && sent.current.value === value) return false;
      sent.current = { value };
      return true;
    },
    forget: () => {
      sent.current = null;
    },
  };
}

/** A field's label, with "Saved" after a save. */
function Label({ htmlFor, text, field, saver }: { htmlFor?: string; text: string; field: FieldKey; saver: InfoSaver }) {
  return (
    <div className="ow-typed__label-row">
      {htmlFor === undefined ? (
        <span className="ow-details__label">{text}</span>
      ) : (
        <label className="ow-details__label" htmlFor={htmlFor}>
          {text}
        </label>
      )}
      <span role="status" className="ow-typed__saved">
        {saver.saved === field ? t.saved : ""}
      </span>
    </div>
  );
}

function FieldError({ id, message }: { id: string; message: string | null }) {
  if (message === null) return null;
  return (
    <p id={id} className="ow-field__error">
      {message}
    </p>
  );
}

/**
 * A text box's draft, kept apart from the saved value. It follows `saved`
 * only while it holds nothing unsaved: once typed in, it stays until a save
 * of what it holds succeeds (`settle`) or it is put back (`reset`).
 */
function useDraft(saved: string) {
  const [s, setS] = useState({ draft: saved, prev: saved, dirty: false });
  let current = s;
  if (saved !== s.prev) {
    current = { draft: s.dirty ? s.draft : saved, prev: saved, dirty: s.dirty };
    setS(current);
  }
  return {
    draft: current.draft,
    setDraft: (draft: string) => setS((x) => ({ ...x, draft, dirty: true })),
    settle: (text: string) => setS((x) => (x.draft === text ? { ...x, dirty: false } : x)),
    reset: (draft: string) => setS((x) => ({ ...x, draft, dirty: false })),
  };
}

/** Notes (a text area); a one-line box when not `multiline`. */
function TextInfo({
  field,
  label,
  value,
  multiline,
  disabledReason,
  saver,
}: {
  field: "notes";
  label: string;
  value: string | null;
  multiline: boolean;
} & Omit<TypedProps, "detail">) {
  const id = useId();
  const { draft, setDraft, settle } = useDraft(value ?? "");
  const sent = useSent<string | null>(value ?? "");

  const commit = (text: string) => {
    const clean = cleanText(text);
    if (clean === value || !sent.once(clean)) return;
    void saver.save(field, { [field]: clean }).then((ok) => {
      // Saved: the draft is in step again, so it follows the new value.
      if (ok) settle(text);
      else sent.forget();
    });
  };
  useOnLeave(() => {
    if (disabledReason === null) commit(draft);
  });

  const common = {
    id,
    className: multiline ? "ow-input ow-textarea ow-typed__notes" : "ow-input",
    value: draft,
    disabled: disabledReason !== null,
    title: disabledReason ?? undefined,
    onChange: (e: { target: { value: string } }) => setDraft(e.target.value),
    onBlur: () => commit(draft),
  };
  return (
    <div className="ow-details__typed">
      <Label htmlFor={id} text={label} field={field} saver={saver} />
      {multiline ? (
        <textarea {...common} rows={4} />
      ) : (
        <input
          {...common}
          type="text"
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
      )}
    </div>
  );
}

/**
 * The Notes section's body: one box (owner's feedback, issue 208). The typed
 * Description was folded into Notes by the album's migration 2; the
 * model's description in What's in it is apart and stays.
 */
export function NotesFields(props: TypedProps) {
  return <TextInfo {...props} field="notes" label={t.notes} value={props.detail.notes} multiline />;
}

/** The People section's body: chips, and a box with suggestions. */
export function PeopleField({ detail, disabledReason, saver }: TypedProps) {
  const id = useId();
  const listId = `${id}-list`;
  const savedKey = detail.people.join("\n");
  const { draft: namesKey, setDraft: setNamesKey, settle } = useDraft(savedKey);
  const names = namesKey === "" ? [] : namesKey.split("\n");
  const [text, setText] = useState("");
  const [known, setKnown] = useState<PersonCount[]>([]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const disabled = disabledReason !== null;

  useEffect(() => {
    let live = true;
    api()
      .invoke("album_people")
      .then((people) => {
        if (live) setKnown(people);
      })
      .catch(() => {
        // Suggestions are a help; without them the field still works.
      });
    return () => {
      live = false;
    };
  }, []);

  const send = (next: string[]) => {
    const key = next.join("\n");
    if (key === namesKey) return;
    setNamesKey(key);
    void saver.save("people", { people: next.length === 0 ? null : next }).then((ok) => {
      if (ok) settle(key);
    });
  };
  const add = (typed: string[]) => send(addNames(names, typed, known));

  const suggestions = open && !disabled ? suggestionsFor(known, names, text) : [];
  const choose = (name: string) => {
    setText("");
    setActive(-1);
    add([name]);
  };

  const onChange = (value: string) => {
    if (value.includes(",")) {
      const { names: done, rest } = splitAtCommas(value);
      add(done);
      setText(rest);
    } else {
      setText(value);
    }
    setActive(-1);
    setOpen(true);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" && suggestions.length > 0) {
      e.preventDefault();
      setActive((i) => (i + 1) % suggestions.length);
    } else if (e.key === "ArrowUp" && suggestions.length > 0) {
      e.preventDefault();
      setActive((i) => (i <= 0 ? suggestions.length - 1 : i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (active >= 0 && suggestions[active] !== undefined) choose(suggestions[active]);
      else if (text.trim() !== "") {
        add([text]);
        setText("");
      }
    } else if (e.key === "Escape" && suggestions.length > 0) {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    } else if (e.key === "Backspace" && text === "" && names.length > 0) {
      send(names.slice(0, -1));
    }
  };

  const onBlur = () => {
    setOpen(false);
    if (text.trim() !== "") {
      add([text]);
      setText("");
    }
  };

  return (
    <div className="ow-details__typed">
      <Label htmlFor={id} text={t.people} field="people" saver={saver} />
      {names.length > 0 && (
        <div className="ow-details__chips">
          {names.map((name) => (
            <Chip
              key={name}
              label={name}
              onRemove={disabled ? undefined : () => send(names.filter((n) => n !== name))}
            />
          ))}
        </div>
      )}
      <div className="ow-country-search">
        <input
          id={id}
          className="ow-input ow-country-search__input"
          type="text"
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={suggestions.length > 0}
          aria-controls={listId}
          aria-activedescendant={active >= 0 && suggestions.length > 0 ? `${id}-${active}` : undefined}
          placeholder={t.addName}
          maxLength={MAX_NAME_CHARS * 4}
          value={text}
          disabled={disabled}
          title={disabledReason ?? undefined}
          onChange={(e) => onChange(e.target.value)}
          onFocus={() => setOpen(true)}
          onKeyDown={onKeyDown}
          onBlur={onBlur}
        />
        {suggestions.length > 0 && (
          <ul id={listId} role="listbox" aria-label={t.suggestions} className="ow-country-search__list">
            {suggestions.map((name, i) => (
              <li
                key={name}
                id={`${id}-${i}`}
                role="option"
                aria-selected={i === active}
                className="ow-country-search__option"
                onMouseDown={(e) => {
                  e.preventDefault();
                  choose(name);
                }}
              >
                {name}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

/** The Date and time section's body. */
export function DateField({ detail, disabledReason, saver }: TypedProps) {
  const id = useId();
  const errorId = `${id}-error`;
  const { draft, setDraft, settle, reset } = useDraft(shownDate(detail));
  const [error, setError] = useState<string | null>(null);
  const sent = useSent<string | null>(`${detail.date.effective}|${detail.date.typed}`);
  const disabled = disabledReason !== null;

  const send = (value: string | null, text: string) => {
    if (!sent.once(value)) return;
    void saver.save("date", { date_override: value }).then((ok) => {
      if (ok) settle(text);
      else sent.forget();
    });
  };

  const commit = (leaving: boolean, text = draft) => {
    const outcome = dateOutcome(text, detail);
    switch (outcome.kind) {
      case "none":
        if (!leaving) setError(null);
        return;
      case "restore":
        if (!leaving) {
          reset(shownDate(detail));
          setError(null);
        }
        return;
      case "error":
        if (!leaving) setError(outcome.message);
        return;
      case "send":
        if (!leaving) setError(null);
        send(outcome.value, text);
    }
  };
  // The calendar and clock (owner's feedback, issue 208): a date and time
  // input kept out of sight, opened by the button beside the box.
  const picker = useRef<HTMLInputElement>(null);
  const picked = parseDate(draft) ?? "";
  useOnLeave(() => {
    if (!disabled) commit(true);
  });

  return (
    <div className="ow-details__typed">
      <Label htmlFor={id} text={t.dateTaken} field="date" saver={saver} />
      <div className="ow-typed__date">
      <input
        id={id}
        className="ow-input"
        type="text"
        value={draft}
        placeholder={t.datePlaceholder}
        disabled={disabled}
        title={disabledReason ?? undefined}
        aria-invalid={error !== null || undefined}
        aria-describedby={error !== null ? errorId : undefined}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => commit(false)}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(false);
        }}
      />
      <input
        ref={picker}
        className="ow-typed__picker"
        type="datetime-local"
        step={1}
        tabIndex={-1}
        aria-label={t.pickDate}
        value={picked}
        disabled={disabled}
        onChange={(e) => {
          const text = e.target.value.replace("T", " ");
          if (text === "") return;
          setDraft(text);
          commit(false, text);
        }}
      />
      <IconButton
        icon="calendar"
        label={t.pickDate}
        disabledReason={disabledReason ?? undefined}
        onClick={() => {
          const el = picker.current;
          if (el === null) return;
          if (typeof el.showPicker === "function") el.showPicker();
          else el.focus();
        }}
      />
      </div>
      <FieldError id={errorId} message={error} />
      {detail.date.typed !== null && (
        <button
          type="button"
          className="ow-typed__link"
          disabled={disabled}
          title={disabledReason ?? undefined}
          onClick={() => {
            const camera = detail.date.camera?.replace("T", " ") ?? "";
            setDraft(camera);
            setError(null);
            send(null, camera);
          }}
        >
          <Icon name={detail.date.camera === null ? "x" : "undo"} size="sm" />
          {detail.date.camera === null ? t.clearDate : t.useCameraDate}
        </button>
      )}
    </div>
  );
}

/**
 * The Place name box, under the Place section's Coordinates and Country
 * labels (owner's feedback, issue 208: the Location search and the
 * Latitude and Longitude boxes were taken out; Pick on map sets the
 * coordinates). A name needs a place: the box is off until the photo has
 * coordinates.
 */
export function LocationField({ detail, disabledReason, saver }: TypedProps) {
  const id = useId();
  const errorId = `${id}-error`;
  const saved = boxesFor(detail);
  const { draft, setDraft, settle, reset } = useDraft(saved.name);
  const [error, setError] = useState<string | null>(null);
  const sent = useSent<string>(`${saved.lat}
${saved.lon}
${saved.name}|${detail.place.typed}`);
  const noPlace = saved.lat === "" || saved.lon === "";
  const off = disabledReason ?? (noPlace ? t.nameNeedsCoordinates : null);

  const commit = (name: string, leaving: boolean) => {
    const outcome = locationOutcome({ ...saved, name }, detail);
    switch (outcome.kind) {
      case "none":
        if (!leaving) setError(null);
        return;
      case "restore":
        if (!leaving) {
          reset(outcome.boxes.name);
          setError(null);
        }
        return;
      case "error":
        if (!leaving) setError(outcome.message);
        return;
      case "send": {
        if (!leaving) setError(null);
        const key = JSON.stringify(outcome.location);
        if (!sent.once(key)) return;
        void saver.save("location", { location: outcome.location }).then((ok) => {
          if (ok) settle(name);
          else sent.forget();
        });
      }
    }
  };
  useOnLeave(() => {
    if (off === null) commit(draft, true);
  });

  return (
    <div className="ow-details__typed">
      <Label htmlFor={id} text={t.placeName} field="location" saver={saver} />
      <input
        id={id}
        className="ow-input"
        type="text"
        value={draft}
        disabled={off !== null}
        title={off ?? undefined}
        aria-invalid={error !== null || undefined}
        aria-describedby={error !== null ? errorId : undefined}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => commit(draft, false)}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(draft, false);
        }}
      />
      <FieldError id={errorId} message={error} />
    </div>
  );
}
