import { useEffect, useRef, useState } from "react";
import { IconButton, Slider, Tooltip } from "../../controls";
import { useNarrowWindow } from "../../controls/narrow";
import { Icon } from "../../icons";
import { gridText } from "../../text/grid";
import { THUMB_MAX, THUMB_MIN, THUMB_STEP } from "./layout";
import { useGridStore } from "./store";
import "./Grid.css";

function SizeSlider() {
  const size = useGridStore((s) => s.thumbSize);
  return (
    <Slider
      label={gridText.thumbSize}
      hideLabel
      value={size}
      min={THUMB_MIN}
      max={THUMB_MAX}
      step={THUMB_STEP}
      onChange={(v) => useGridStore.getState().setThumbSize(v)}
    />
  );
}

/**
 * Below 1100 px (docs/09-visual-design.md, "The bars"): the grid icon is a
 * button that drops the slider in a popover. Esc or a press outside
 * closes it.
 */
function ThumbSizePopover() {
  const [open, setOpen] = useState(false);
  const area = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const press = (e: PointerEvent) => {
      if (area.current !== null && !area.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", press);
    return () => document.removeEventListener("pointerdown", press);
  }, [open]);
  return (
    <div
      ref={area}
      className="ow-thumb-size ow-thumb-size--narrow"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.preventDefault();
          e.stopPropagation();
          setOpen(false);
          area.current?.querySelector<HTMLElement>("button")?.focus();
        }
      }}
    >
      <IconButton icon="grid" label={gridText.thumbSize} aria-expanded={open} onClick={() => setOpen(!open)} />
      {open && (
        <div className="ow-thumb-size__popover">
          <SizeSlider />
        </div>
      )}
    </div>
  );
}

/**
 * The thumbnail size control of the top bar (docs/04-browsing.md, "Top
 * bar" and "Thumbnail size"): a grid icon and a slider, or below 1100 px
 * the grid icon alone with the slider in a popover. The grid follows at
 * once; the setting is saved 300 ms after the slider last moved.
 */
export function ThumbSize() {
  const narrow = useNarrowWindow();
  if (narrow) return <ThumbSizePopover />;
  return (
    <Tooltip text={gridText.thumbSize} describe={false}>
      <div className="ow-thumb-size">
        <Icon name="grid" />
        <SizeSlider />
      </div>
    </Tooltip>
  );
}
