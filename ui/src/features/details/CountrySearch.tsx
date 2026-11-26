import { useId, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../../api";
import type { CountryMatch } from "../../api/bindings/CountryMatch";
import { detailsText } from "../../text/details";
import "./CountrySearch.css";

/** The text boxes of the Location group (docs/04-browsing.md, "How typed fields save"). */
export interface LocationBoxes {
  lat: string;
  lon: string;
  name: string;
}

/**
 * The Location group's boxes after a country is chosen: its name goes in
 * Place name and, when both coordinate boxes are empty, its central point
 * fills them. The group then saves.
 */
export function withCountry(boxes: LocationBoxes, country: CountryMatch): LocationBoxes {
  const empty = boxes.lat.trim() === "" && boxes.lon.trim() === "";
  return {
    lat: empty ? String(country.lat) : boxes.lat,
    lon: empty ? String(country.lon) : boxes.lon,
    name: country.name,
  };
}

interface CountrySearchProps {
  /** Called with the country chosen from the list. */
  onChoose: (country: CountryMatch) => void;
  disabled?: boolean;
}

/**
 * "Search for a country…": typing lists up to 6 countries whose name
 * contains the text (`app_place_search`, from the outlines bundled in Rust);
 * choosing one hands it to `onChoose` and empties the box.
 */
export function CountrySearch({ onChoose, disabled = false }: CountrySearchProps) {
  const id = useId();
  const listId = `${id}-list`;
  const [text, setText] = useState("");
  const [matches, setMatches] = useState<CountryMatch[]>([]);
  const [active, setActive] = useState(-1);
  // Only the answer to the latest request is shown.
  const latest = useRef(0);

  const close = () => {
    latest.current++;
    setMatches([]);
    setActive(-1);
  };

  const search = (value: string) => {
    setText(value);
    if (value.trim() === "") {
      close();
      return;
    }
    const request = ++latest.current;
    api()
      .invoke("app_place_search", { text: value })
      .then((found) => {
        if (request !== latest.current) return;
        setMatches(found);
        setActive(-1);
      })
      .catch(() => {
        // The search reads bundled data; a failure just shows no matches.
        if (request === latest.current) setMatches([]);
      });
  };

  const choose = (country: CountryMatch) => {
    setText("");
    close();
    onChoose(country);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (matches.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => (i + 1) % matches.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => (i <= 0 ? matches.length - 1 : i - 1));
    } else if (e.key === "Enter" && active >= 0) {
      e.preventDefault();
      e.stopPropagation();
      choose(matches[active]!);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };

  const open = matches.length > 0;
  return (
    <div className="ow-country-search">
      <input
        className="ow-input ow-country-search__input"
        type="text"
        role="combobox"
        aria-label={detailsText.countrySearch}
        placeholder={detailsText.countrySearch}
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        aria-activedescendant={open && active >= 0 ? `${id}-${active}` : undefined}
        value={text}
        disabled={disabled}
        onChange={(e) => search(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={close}
      />
      {open && (
        <ul id={listId} role="listbox" aria-label={detailsText.countryMatches} className="ow-country-search__list">
          {matches.map((m, i) => (
            <li
              key={m.name}
              id={`${id}-${i}`}
              role="option"
              aria-selected={i === active}
              className="ow-country-search__option"
              // Chosen on mouse down, before the box loses focus and closes the list.
              onMouseDown={(e) => {
                e.preventDefault();
                choose(m);
              }}
            >
              {m.name}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
