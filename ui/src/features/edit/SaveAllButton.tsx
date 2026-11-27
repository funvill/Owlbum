import { Button, Tooltip } from "../../controls";
import { editText } from "../../text/edit";
import { useSettingsStore } from "../app/settings";
import { saveAll, useSaveStore } from "./save";

const t = editText;

/**
 * Save all (N) in the top bar, in `file` mode while any photo has an edit
 * (docs/06-editing-and-file-actions.md, "Save all"). While a save runs it
 * shows the count in its place: "Saving 2 of 5…".
 */
export function SaveAllButton({ viewOnly }: { viewOnly?: string }) {
  const fileMode = useSettingsStore((s) => s.settings?.edit_target === "file");
  const edited = useSaveStore((s) => s.status?.edited ?? 0);
  const running = useSaveStore((s) => s.running);
  const progress = useSaveStore((s) => s.progress);
  if (running) {
    return (
      <Button icon="check" disabledReason={t.saving}>
        {progress === null ? t.saving : t.savingProgress(progress.done, progress.total)}
      </Button>
    );
  }
  if (!fileMode || edited === 0) return null;
  if (viewOnly !== undefined) {
    return (
      <Button icon="check" disabledReason={viewOnly}>
        {t.saveAll(edited)}
      </Button>
    );
  }
  return (
    <Tooltip text={t.saveAllTooltip}>
      <Button icon="check" onClick={() => void saveAll()}>
        {t.saveAll(edited)}
      </Button>
    </Tooltip>
  );
}
