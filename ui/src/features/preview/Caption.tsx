// The preview's caption (docs/04-browsing.md, "Preview pane", "Caption"):
// the selection count, the path, and the badges. The edit actions at its
// right end, Before (while the photo has an edit) and Edit, belong to
// editing (docs/06-editing-and-file-actions.md, "Edit mode", "Before and
// after"); they are left out while editing.

import type { EditTarget } from "../../api/bindings/EditTarget";
import type { Row } from "../../api/bindings/Row";
import { Button, Tooltip } from "../../controls";
import { beforeButtonHandlers } from "../edit/before";
import { previewText as t } from "../../text/preview";

export interface CaptionProps {
  row: Row;
  /** How many photos are selected. */
  selected: number;
  /** The file on this computer, for the tooltip; the relative path until the details arrive. */
  fullPath: string | null;
  editTarget: EditTarget;
  /** The edit actions: left out while editing. */
  actions?: {
    /** Why Edit cannot be used now; undefined when it can. */
    editBlocked: string | undefined;
    onEdit: () => void;
  };
}

export function Caption({ row, selected, fullPath, editTarget, actions }: CaptionProps) {
  return (
    <div className="ow-preview__caption">
      {selected > 1 && <span className="ow-preview__count">{t.selected(selected)}</span>}
      <Tooltip text={fullPath ?? row.rel_path} describe={false}>
        {/* Cut with an ellipsis at the start: the end of the path is the part that tells photos apart. */}
        <span className="ow-preview__path ow-selectable">
          <bdi>{row.rel_path}</bdi>
        </span>
      </Tooltip>
      {row.star && <span className="ow-preview__badge ow-preview__badge--star">{t.starred}</span>}
      {row.hidden && <span className="ow-preview__badge">{t.hidden}</span>}
      {row.stack !== null && <span className="ow-preview__badge">{t.inStack(row.stack.place, row.stack.size)}</span>}
      {row.edit_plan !== null && (
        <span className={`ow-preview__badge ow-preview__badge--edit-${editTarget}`}>
          {editTarget === "file" ? t.editedFile : t.editedAlbum}
        </span>
      )}
      {actions !== undefined && (
        <span className="ow-preview__actions">
          {row.edit_plan !== null && (
            <Tooltip text={t.beforeTooltip}>
              <Button size="sm" {...beforeButtonHandlers}>
                {t.before}
              </Button>
            </Tooltip>
          )}
          {row.kind !== "video" && (
            <Button
              size="sm"
              icon="crop"
              disabledReason={actions.editBlocked}
              aria-keyshortcuts={t.editShortcut}
              onClick={actions.onEdit}
            >
              {t.edit}
            </Button>
          )}
        </span>
      )}
    </div>
  );
}
