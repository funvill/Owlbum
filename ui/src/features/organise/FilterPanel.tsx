import { useEffect, useRef, type ReactNode } from "react";
import type { Facets } from "../../api/bindings/Facets";
import type { Filter } from "../../api/bindings/Filter";
import type { KindFilter } from "../../api/bindings/KindFilter";
import { Checkbox, SelectField, TextField, type SelectOption } from "../../controls";
import { organiseText } from "../../text/organise";
import { FilterChip } from "./FilterChip";

const t = organiseText.panel;

interface FilterPanelProps {
  filter: Filter;
  /** The counts beside the options; null until `query_facets` answers. */
  facets: Facets | null;
  onChange: (filter: Filter) => void;
}

/** A select option with its facet; muted, still enabled, when the count is 0. */
function option(value: string, label: string, n?: number): SelectOption {
  return { value, label: t.counted(label, n), muted: n === 0 };
}

/** Every kind of the media type filter, in the order Rust lists them. */
const KINDS: KindFilter[] = ["still", "animated", "raw", "video"];

/**
 * The kinds shown after `kind` is turned on or off: absent when every
 * kind is on; unchanged when it would turn the last one off.
 */
export function toggleKind(kinds: KindFilter[] | undefined, kind: KindFilter, on: boolean): KindFilter[] | undefined {
  const had = kinds ?? KINDS;
  const next = KINDS.filter((k) => (k === kind ? on : had.includes(k)));
  if (next.length === 0) return kinds;
  return next.length === KINDS.length ? undefined : next;
}

/** The formats shown after `format` is turned on or off: absent when none is on. */
export function toggleFormat(formats: string[] | undefined, format: string, on: boolean): string[] | undefined {
  const next = (formats ?? []).filter((f) => f !== format);
  if (on) next.push(format);
  return next.length === 0 ? undefined : next;
}

function Group({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="ow-filter-panel__group">
      <legend className="ow-filter-panel__legend">{legend}</legend>
      {children}
    </fieldset>
  );
}

/**
 * The filter panel (docs/05-organising.md, "The filter panel"): a labelled
 * group of the filters with a panel control, each option with its facet.
 * It is marked `data-keys="dialog"`, so the grid's keys do nothing while
 * focus is inside. Focus moves to its first control when it opens; the bar
 * owns opening and closing.
 */
export function FilterPanel({ filter, facets, onChange }: FilterPanelProps) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("select, input, button")?.focus();
  }, []);
  const set = (change: Partial<Filter>) => {
    const next: Filter = { ...filter, ...change };
    for (const key of Object.keys(next) as (keyof Filter)[]) if (next[key] === undefined) delete next[key];
    // A typed date can get past the picker's bounds: From never after To,
    // and only whole days reach Rust.
    const day = (d: string | undefined) => d === undefined || /^\d{4}-\d{2}-\d{2}$/.test(d);
    if (!day(next.from) || !day(next.to)) return;
    if (next.from !== undefined && next.to !== undefined && next.from > next.to) return;
    onChange(next);
  };
  const folders = facets?.folders ?? [];
  // The formats come from Rust's registry through the facets, never from a list here.
  const formats = facets?.formats ?? [];
  const people = filter.people ?? [];
  const tags = filter.tags ?? [];
  return (
    <div ref={ref} className="ow-filter-panel" role="group" aria-label={t.label} data-keys="dialog">
      <Group legend={t.groups.photo}>
        <SelectField
          label={t.stars}
          value={filter.star ?? ""}
          options={[
            option("", t.starOptions.all),
            option("yes", t.starOptions.yes, facets?.star.yes),
            option("no", t.starOptions.no, facets?.star.no),
          ]}
          onChange={(v) => set({ star: v === "" ? undefined : (v as Filter["star"]) })}
        />
        <SelectField
          label={t.shape}
          value={filter.shape ?? ""}
          options={[
            option("", t.shapeOptions.any),
            option("portrait", t.shapeOptions.portrait, facets?.shape.portrait),
            option("landscape", t.shapeOptions.landscape, facets?.shape.landscape),
          ]}
          onChange={(v) => set({ shape: v === "" ? undefined : (v as Filter["shape"]) })}
        />
        <SelectField
          label={t.hidden}
          value={filter.hidden ?? ""}
          options={[
            option("", t.hiddenOptions.leave),
            option("show", t.hiddenOptions.show, facets?.hidden),
            option("only", t.hiddenOptions.only, facets?.hidden),
          ]}
          onChange={(v) => set({ hidden: v === "" ? undefined : (v as Filter["hidden"]) })}
        />
      </Group>
      <Group legend={t.groups.type}>
        <div className="ow-filter-panel__list" role="group" aria-label={t.kindsLabel}>
          {KINDS.map((kind) => (
            <Checkbox
              key={kind}
              label={t.counted(t.kinds[kind], facets?.kinds[kind])}
              checked={filter.kinds === undefined || filter.kinds.includes(kind)}
              onChange={(on) => set({ kinds: toggleKind(filter.kinds, kind, on) })}
            />
          ))}
        </div>
        <div className="ow-filter-panel__list" role="group" aria-label={t.formatsLabel}>
          <span className="ow-filter-panel__sublabel">{t.formatsLabel}</span>
          {formats.length === 0 ? (
            <p className="ow-filter-panel__note">{t.noFormats}</p>
          ) : (
            formats.map((f) => (
              <Checkbox
                key={f.format}
                label={t.counted(f.label, f.count)}
                checked={filter.formats?.includes(f.format) ?? false}
                onChange={(on) => set({ formats: toggleFormat(filter.formats, f.format, on) })}
              />
            ))
          )}
        </div>
      </Group>
      <Group legend={t.groups.when}>
        <TextField
          type="date"
          label={t.from}
          value={filter.from ?? ""}
          max={filter.to}
          onChange={(v) => set({ from: v === "" ? undefined : v })}
        />
        <TextField
          type="date"
          label={t.to}
          value={filter.to ?? ""}
          min={filter.from}
          onChange={(v) => set({ to: v === "" ? undefined : v })}
        />
      </Group>
      <Group legend={t.groups.where}>
        {folders.length >= 2 && (
          <SelectField
            label={t.folder}
            value={filter.folder ?? "\u0000all"}
            options={[option("\u0000all", t.allFolders), ...folders.map((f) => option(f.folder, f.label, f.count))]}
            onChange={(v) => set({ folder: v === "\u0000all" ? undefined : v })}
          />
        )}
        <Checkbox
          label={t.counted(t.hasLocation, facets?.has_location)}
          checked={filter.has_location ?? false}
          onChange={(on) => set({ has_location: on || undefined })}
        />
      </Group>
      <Group legend={t.groups.content}>
        <Checkbox
          label={t.counted(t.hasPeople, facets?.has_people)}
          checked={filter.has_people ?? false}
          onChange={(on) => set({ has_people: on || undefined })}
        />
        {people.length + tags.length === 0 ? (
          <p className="ow-filter-panel__note">{t.noContent}</p>
        ) : (
          <div className="ow-filter-panel__chips">
            {people.map((name, i) => (
              <FilterChip
                key={`person:${name}`}
                text={name}
                onRemove={() => set({ people: people.filter((_, j) => j !== i) })}
              />
            ))}
            {tags.map((tag, i) => (
              <FilterChip
                key={`tag:${tag.kind}:${tag.value}`}
                text={tag.value}
                onRemove={() => set({ tags: tags.filter((_, j) => j !== i) })}
              />
            ))}
          </div>
        )}
      </Group>
      <Group legend={t.groups.quality}>
        <Checkbox
          label={t.counted(t.lowQuality, facets?.low_quality)}
          checked={filter.low_quality ?? false}
          onChange={(on) => set({ low_quality: on || undefined })}
        />
        <Checkbox
          label={t.counted(t.nearDuplicates, facets?.near_duplicates)}
          checked={filter.near_duplicates ?? false}
          onChange={(on) => set({ near_duplicates: on || undefined })}
        />
      </Group>
    </div>
  );
}
