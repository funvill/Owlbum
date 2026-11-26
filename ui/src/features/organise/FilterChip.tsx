import { Tooltip } from "../../controls";
import { Icon } from "../../icons";
import { organiseText } from "../../text/organise";

const t = organiseText.filtered;

/**
 * One active filter (docs/05-organising.md, "The filtered row"): its text,
 * from Rust, and a × named "Remove filter {text}" with the tooltip "Remove
 * this filter".
 */
export function FilterChip({ text, onRemove }: { text: string; onRemove: () => void }) {
  return (
    <span className="ow-chip ow-chip--active">
      <span className="ow-chip__label">{text}</span>
      <Tooltip text={t.removeTip}>
        <button type="button" className="ow-chip__remove" aria-label={t.remove(text)} onClick={onRemove}>
          <Icon name="x" size="sm" />
        </button>
      </Tooltip>
    </span>
  );
}
