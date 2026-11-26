import { useId, type CSSProperties } from "react";
import "./Slider.css";

interface SliderProps {
  label: string;
  hideLabel?: boolean;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  /**
   * A neutral middle (straighten, light and colour): the fill runs from it
   * outwards, and a double-click returns to it.
   */
  neutral?: number;
  /** How the value is written beside the slider. */
  format?: (value: number) => string;
  disabled?: boolean;
}

/** A slider with its value shown as a number beside it. docs/09-visual-design.md, "Slider". */
export function Slider({
  label,
  hideLabel = false,
  value,
  min,
  max,
  step = 1,
  onChange,
  neutral,
  format = String,
  disabled,
}: SliderProps) {
  const id = useId();
  const percent = (v: number) => `${((v - min) / (max - min)) * 100}%`;
  const start = neutral ?? min;
  const fill = {
    "--_from": percent(Math.min(start, value)),
    "--_to": percent(Math.max(start, value)),
  } as CSSProperties;

  return (
    <div className="ow-slider">
      <label htmlFor={id} className={hideLabel ? "ow-visually-hidden" : "ow-slider__label"}>
        {label}
      </label>
      <input
        id={id}
        type="range"
        className="ow-slider__input"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        style={fill}
        onChange={(e) => onChange(Number(e.target.value))}
        onDoubleClick={neutral === undefined ? undefined : () => onChange(neutral)}
      />
      <output htmlFor={id} className="ow-slider__value ow-num">
        {format(value)}
      </output>
    </div>
  );
}
