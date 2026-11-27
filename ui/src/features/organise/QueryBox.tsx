import { useEffect, useRef, useState } from "react";
import { api } from "../../api";
import type { Filter } from "../../api/bindings/Filter";
import type { ParsedBox } from "../../api/bindings/ParsedBox";
import { SearchBox, Tooltip, useNarrowWindow } from "../../controls";
import { organiseText as t } from "../../text/organise";
import { useGridStore } from "../grid";

/** The query re-runs this long after the last change to the search text. */
export const SEARCH_DELAY_MS = 200;

/** The open boxes' ways to empty themselves, for Clear filter. */
const clears = new Set<() => void>();

/** Empties the search box and its error (Clear filter). */
export function clearQueryBox(): void {
  for (const clear of clears) clear();
}

/** The filter with a parse's filters merged in. */
function merged(filter: Filter, parsed: ParsedBox): Filter {
  const out: Filter = { ...filter, ...parsed.set };
  if (parsed.add_tags.length > 0) out.tags = [...(filter.tags ?? []), ...parsed.add_tags];
  if (parsed.add_people.length > 0) out.people = [...(filter.people ?? []), ...parsed.add_people];
  return out;
}

/** True when the box's last word is finished: it ends with a space outside quotes. */
function endsWord(text: string): boolean {
  return text.endsWith(" ") && (text.match(/"/g)?.length ?? 0) % 2 === 0;
}

/**
 * The search box and its hint line (docs/05-organising.md, "Query words in
 * the search box"). Every change asks Rust to read the box
 * (`query_parse(text, false)`); Enter asks with `enter` true, so the last
 * word is read too. A `key:value` word Rust takes leaves the box and joins
 * the filter at once; the rest is the search text, which re-runs the query
 * 200 ms after it last changed. The page holds no table of keys.
 *
 * In a window under 1100 px the hint row is hidden (an error still shows
 * there) and the examples become the box's tooltip
 * (docs/09-visual-design.md, "The bars").
 */
export function QueryBox() {
  const search = useGridStore((s) => s.query.search);
  const [text, setText] = useState(search ?? "");
  const [error, setError] = useState<string | null>(null);
  /** The box's text as last sent, so a reply to older text is dropped. */
  const sent = useRef(text);
  /** The search text this box last gave the query. */
  const gave = useRef<string | null>(search);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // In a narrow window the hint row is hidden and its examples are the box's tooltip.
  const narrow = useNarrowWindow();

  useEffect(() => {
    const clear = () => {
      clearTimeout(timer.current);
      sent.current = "";
      gave.current = null;
      setText("");
      setError(null);
    };
    clears.add(clear);
    return () => {
      clears.delete(clear);
      clearTimeout(timer.current);
    };
  }, []);

  // The search text changed from elsewhere (Clear filter, a restored session).
  useEffect(() => {
    if (search === gave.current) return;
    gave.current = search;
    clearTimeout(timer.current);
    sent.current = search ?? "";
    setText(search ?? "");
    setError(null);
  }, [search]);

  const applySearch = (next: string | null, filter: Filter | null) => {
    clearTimeout(timer.current);
    if (filter !== null) {
      // A filter changed: run at once, with the search text as it now is.
      gave.current = next;
      const grid = useGridStore.getState();
      grid.setQuery({ ...grid.query, filter, search: next });
      return;
    }
    if (next === gave.current) return;
    timer.current = setTimeout(() => {
      gave.current = next;
      const grid = useGridStore.getState();
      grid.setQuery({ ...grid.query, search: next });
    }, SEARCH_DELAY_MS);
  };

  const parse = async (value: string, enter: boolean) => {
    sent.current = value;
    const filter = useGridStore.getState().query.filter;
    let parsed: ParsedBox;
    try {
      parsed = await api().invoke("query_parse", { text: value, enter, filter });
    } catch {
      // No album, or it closed: nothing to read the box against.
      return;
    }
    if (sent.current !== value) return;
    // Until a word is finished, the error a change cleared stays cleared.
    if (enter || endsWord(value)) setError(parsed.error);
    if (parsed.taken) {
      sent.current = parsed.rest;
      setText(parsed.rest);
      applySearch(parsed.search, merged(useGridStore.getState().query.filter, parsed));
    } else {
      applySearch(parsed.search, null);
    }
  };

  const box = (
    <SearchBox
      label={t.search.label}
      placeholder={t.search.placeholder}
      className="ow-query-box__input"
      value={text}
      onChange={(value) => {
        setText(value);
        setError(null);
        void parse(value, false);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          void parse(e.currentTarget.value, true);
        }
      }}
    />
  );

  return (
    <div className="ow-query-box">
      {narrow ? <Tooltip text={t.search.tooltip}>{box}</Tooltip> : box}
      {/* Kept in place while hidden, so an error shown in a narrow window is still announced. */}
      <span
        className={`ow-query-box__hint${error === null ? "" : " ow-query-box__hint--error"}${narrow && error === null ? " ow-query-box__hint--hidden" : ""}`}
        aria-live="polite"
      >
        {error ??
          (narrow ? null : (
            <>
              {t.search.try}
              {t.search.examples.map((word) => (
                <span key={word} className="ow-query-box__word">
                  {word}
                </span>
              ))}
            </>
          ))}
      </span>
    </div>
  );
}
