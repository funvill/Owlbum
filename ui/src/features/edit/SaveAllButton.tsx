import { useState } from "react";
import { create } from "zustand";
import { Button, Checkbox, Dialog, Tooltip } from "../../controls";
import { editText } from "../../text/edit";
import { bin } from "../files/actions";
import { useSettingsStore } from "../app/settings";
import { saveAll, useSaveStore } from "./save";

const t = editText;

/** Whether the Save all question is open. */
const useAsk = create<{ open: boolean }>(() => ({ open: false }));

/**
 * Save all, after the question "This overwrites the photo files" unless the
 * user ticked "Don't ask again, always overwrite" (`save_all.ask` false;
 * owner's feedback, issue 209). Used by the top bar and the photo menu.
 */
export function askSaveAll(): void {
  if (useSettingsStore.getState().settings?.["save_all.ask"] === false) void saveAll();
  else useAsk.setState({ open: true });
}

/** Closes the question (tests). */
export function resetSaveAllAsk(): void {
  useAsk.setState({ open: false });
}

/** The question: Yes saves, Cancel does nothing; the box stops it being asked again. */
function SaveAllQuestion({ edited }: { edited: number }) {
  const open = useAsk((s) => s.open);
  const [always, setAlways] = useState(false);
  const close = () => {
    setAlways(false);
    useAsk.setState({ open: false });
  };
  const yes = async () => {
    const keep = always;
    close();
    if (keep) await useSettingsStore.getState().update({ "save_all.ask": false });
    await saveAll();
  };
  return (
    <Dialog
      open={open}
      size="sm"
      title={t.saveAllAskTitle(edited)}
      onClose={close}
      actions={
        <>
          <Button onClick={close}>{t.saveAllAskCancel}</Button>
          <Button kind="primary" data-autofocus onClick={() => void yes()}>
            {t.saveAllAskYes}
          </Button>
        </>
      }
    >
      <p>{t.saveAllAskText(bin())}</p>
      <Checkbox label={t.saveAllAskNever} checked={always} onChange={setAlways} />
    </Dialog>
  );
}

/**
 * Save all (N) in the top bar, in either `edit_target` mode while any photo
 * has an edit (docs/06-editing-and-file-actions.md, "Save all"; owner's
 * feedback, issue 209). It asks first unless told not to. While a save runs
 * it shows the count in its place: "Saving 2 of 5…", and "Finishing
 * saving…" while the window waits to close (docs/06, "Closing and
 * quitting").
 */
export function SaveAllButton({ viewOnly }: { viewOnly?: string }) {
  const edited = useSaveStore((s) => s.status?.edited ?? 0);
  const running = useSaveStore((s) => s.running);
  const progress = useSaveStore((s) => s.progress);
  const finishing = useSaveStore(
    (s) => s.closing && (s.running || (s.status?.saving ?? 0) > 0),
  );
  if (finishing) {
    return (
      <Button icon="check" disabledReason={t.finishingSaving}>
        {t.finishingSaving}
      </Button>
    );
  }
  if (running) {
    return (
      <Button icon="check" disabledReason={t.saving}>
        {progress === null
          ? t.saving
          : t.savingProgress(progress.done, progress.total)}
      </Button>
    );
  }
  if (edited === 0) return null;
  if (viewOnly !== undefined) {
    return (
      <Button icon="check" disabledReason={viewOnly}>
        {t.saveAll(edited)}
      </Button>
    );
  }
  return (
    <>
      <Tooltip text={t.saveAllTooltip}>
        <Button icon="check" onClick={askSaveAll}>
          {t.saveAll(edited)}
        </Button>
      </Tooltip>
      <SaveAllQuestion edited={edited} />
    </>
  );
}
