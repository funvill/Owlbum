import { Slider, Tooltip } from "../../controls";
import { Icon } from "../../icons";
import { gridText } from "../../text/grid";
import { THUMB_MAX, THUMB_MIN, THUMB_STEP } from "./layout";
import { useGridStore } from "./store";
import "./Grid.css";

/**
 * The thumbnail size control of the top bar (docs/04-browsing.md, "Top
 * bar" and "Thumbnail size"): a grid icon and a slider. The grid follows
 * at once; the setting is saved 300 ms after the slider last moved.
 */
export function ThumbSize() {
  const size = useGridStore((s) => s.thumbSize);
  return (
    <Tooltip text={gridText.thumbSize} describe={false}>
      <div className="ow-thumb-size">
        <Icon name="grid" />
        <Slider
          label={gridText.thumbSize}
          hideLabel
          value={size}
          min={THUMB_MIN}
          max={THUMB_MAX}
          step={THUMB_STEP}
          onChange={(v) => useGridStore.getState().setThumbSize(v)}
        />
      </div>
    </Tooltip>
  );
}
