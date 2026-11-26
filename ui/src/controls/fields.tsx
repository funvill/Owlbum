import { useId, useLayoutEffect, useRef, type InputHTMLAttributes, type ReactNode } from "react";
import { Icon } from "../icons";
import { controlsText } from "../text/controls";
import "./fields.css";

interface FieldShellProps {
  id: string;
  label: string;
  hideLabel?: boolean;
  error?: string | undefined;
  errorId: string;
  children: ReactNode;
}

/** A label above a control and, when the value is invalid, a sentence below it. */
function FieldShell({ id, label, hideLabel = false, error, errorId, children }: FieldShellProps) {
  return (
    <div className="ow-field">
      <label htmlFor={id} className={hideLabel ? "ow-visually-hidden" : "ow-field__label"}>
        {label}
      </label>
      {children}
      {error !== undefined && (
        <p id={errorId} className="ow-field__error">
          <Icon name="alert" size="sm" />
          <span>{error}</span>
        </p>
      )}
    </div>
  );
}

type InputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "id" | "type">;

interface TextFieldProps extends InputProps {
  label: string;
  hideLabel?: boolean;
  value: string;
  onChange: (value: string) => void;
  /** Why the value is invalid; shown under the field in the error colour. */
  error?: string;
  type?: "text" | "number" | "date";
}

/** A one-line text input. docs/09-visual-design.md, "Inputs". */
export function TextField({ label, hideLabel, value, onChange, error, type = "text", className, ...rest }: TextFieldProps) {
  const id = useId();
  const errorId = `${id}-error`;
  return (
    <FieldShell id={id} label={label} hideLabel={hideLabel} error={error} errorId={errorId}>
      <input
        {...rest}
        id={id}
        type={type}
        className={["ow-input", className].filter(Boolean).join(" ")}
        value={value}
        aria-invalid={error !== undefined || undefined}
        aria-describedby={error !== undefined ? errorId : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
    </FieldShell>
  );
}

export interface SelectOption {
  value: string;
  label: string;
}

interface SelectFieldProps {
  label: string;
  hideLabel?: boolean;
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  error?: string;
}

/** A drop-down list, drawn like an input. */
export function SelectField({ label, hideLabel, value, options, onChange, disabled, error }: SelectFieldProps) {
  const id = useId();
  const errorId = `${id}-error`;
  return (
    <FieldShell id={id} label={label} hideLabel={hideLabel} error={error} errorId={errorId}>
      <span className="ow-select">
        <select
          id={id}
          className="ow-input ow-select__control"
          value={value}
          disabled={disabled}
          aria-invalid={error !== undefined || undefined}
          aria-describedby={error !== undefined ? errorId : undefined}
          onChange={(e) => onChange(e.target.value)}
        >
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        <Icon name="chevron-down" size="sm" className="ow-select__chevron" />
      </span>
    </FieldShell>
  );
}

interface SearchBoxProps extends Omit<InputProps, "placeholder"> {
  /** The name; the box shows no visible label. */
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  clearLabel?: string;
}

/** A search box with a search icon at its left and, once it holds text, an x that clears it. */
export function SearchBox({
  label,
  value,
  onChange,
  clearLabel = controlsText.clearSearch,
  className,
  ...rest
}: SearchBoxProps) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <span className={["ow-search", className].filter(Boolean).join(" ")}>
      <Icon name="search" className="ow-search__icon" />
      <input
        {...rest}
        ref={input}
        type="search"
        className="ow-input ow-search__input"
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      {value !== "" && (
        <button
          type="button"
          className="ow-search__clear"
          aria-label={clearLabel}
          onClick={() => {
            onChange("");
            input.current?.focus();
          }}
        >
          <Icon name="x" size="sm" />
        </button>
      )}
    </span>
  );
}

interface TextAreaProps {
  label: string;
  hideLabel?: boolean;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  error?: string;
}

const minRows = 3;
const maxRows = 12;

/**
 * The number of lines a text area shows: its lines, counting those that
 * wrap (`wrapped`, when it could be measured), from 3 to 12.
 */
export function rowsFor(value: string, wrapped = 0): number {
  const lines = Math.max(value.split("\n").length, wrapped);
  return Math.min(maxRows, Math.max(minRows, lines));
}

/** How many lines the text in a text area takes, wrapping included; 0 when it cannot be measured. */
function measureLines(el: HTMLTextAreaElement): number {
  const style = getComputedStyle(el);
  const line = parseFloat(style.lineHeight);
  if (!(line > 0)) return 0;
  const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
  const rows = el.rows;
  el.rows = 1;
  const lines = Math.ceil((el.scrollHeight - padding) / line);
  el.rows = rows;
  return lines > 0 ? lines : 0;
}

/** A multi-line field (notes). It grows with its text from 3 to 12 lines, then scrolls. */
export function TextArea({ label, hideLabel, value, onChange, placeholder, disabled, error }: TextAreaProps) {
  const id = useId();
  const errorId = `${id}-error`;
  const ref = useRef<HTMLTextAreaElement>(null);

  // Lines that wrap count too; that needs layout, so it is measured after drawing.
  useLayoutEffect(() => {
    const el = ref.current;
    if (el !== null) el.rows = rowsFor(value, measureLines(el));
  }, [value]);

  return (
    <FieldShell id={id} label={label} hideLabel={hideLabel} error={error} errorId={errorId}>
      <textarea
        ref={ref}
        id={id}
        className="ow-input ow-textarea ow-selectable"
        rows={rowsFor(value)}
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        aria-invalid={error !== undefined || undefined}
        aria-describedby={error !== undefined ? errorId : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
    </FieldShell>
  );
}
