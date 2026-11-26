import { useEffect, useId, useRef } from "react";
import { Icon } from "../icons";
import "./Checkbox.css";

interface CheckboxProps {
  label: string;
  /** "mixed" when some, not all, of what it stands for is on. */
  checked: boolean | "mixed";
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  description?: string;
}

/**
 * A 16 px checkbox whose label is part of the click target.
 * docs/09-visual-design.md, "Checkboxes and radio buttons".
 */
export function Checkbox({ label, checked, onChange, disabled, description }: CheckboxProps) {
  const ref = useRef<HTMLInputElement>(null);
  const descriptionId = useId();
  const mixed = checked === "mixed";

  useEffect(() => {
    if (ref.current !== null) ref.current.indeterminate = mixed;
  }, [mixed]);

  return (
    <label className="ow-check">
      <span className="ow-check__box">
        <input
          ref={ref}
          type="checkbox"
          className="ow-check__input"
          checked={checked === true}
          disabled={disabled}
          aria-labelledby={`${descriptionId}-label`}
          aria-describedby={description !== undefined ? descriptionId : undefined}
          onChange={(e) => onChange(e.target.checked)}
        />
        <Icon name={mixed ? "minus" : "check"} size="sm" className="ow-check__mark" />
      </span>
      <span className="ow-check__text">
        <span id={`${descriptionId}-label`} className="ow-check__label">
          {label}
        </span>
        {description !== undefined && (
          <span id={descriptionId} className="ow-check__description">
            {description}
          </span>
        )}
      </span>
    </label>
  );
}

export interface RadioOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}

interface RadioGroupProps {
  label: string;
  value: string;
  options: RadioOption[];
  onChange: (value: string) => void;
}

/**
 * A group of radio buttons where the whole row is the click target, with a
 * description under a label where there is one. Arrow keys move between the
 * choices, as the browser does for radio buttons.
 */
export function RadioGroup({ label, value, options, onChange }: RadioGroupProps) {
  const name = useId();
  const labelId = `${name}-label`;
  return (
    <div role="radiogroup" aria-labelledby={labelId} className="ow-radio-group">
      <span id={labelId} className="ow-radio-group__label">
        {label}
      </span>
      {options.map((option) => {
        const descriptionId = `${name}-${option.value}`;
        return (
          <label key={option.value} className="ow-check ow-check--radio">
            <span className="ow-check__box">
              <input
                type="radio"
                className="ow-check__input"
                name={name}
                value={option.value}
                checked={option.value === value}
                disabled={option.disabled}
                aria-labelledby={`${descriptionId}-label`}
                aria-describedby={option.description !== undefined ? descriptionId : undefined}
                onChange={() => onChange(option.value)}
              />
              <span className="ow-check__dot" />
            </span>
            <span className="ow-check__text">
              <span id={`${descriptionId}-label`} className="ow-check__label">
                {option.label}
              </span>
              {option.description !== undefined && (
                <span id={descriptionId} className="ow-check__description">
                  {option.description}
                </span>
              )}
            </span>
          </label>
        );
      })}
    </div>
  );
}
