import { create } from "zustand";
import type { TrashQuestion as Question } from "../../api/bindings/TrashQuestion";
import { Button, Dialog } from "../../controls";
import { filesText } from "../../text/files";

interface Asking {
  question: Question | null;
  answer: ((yes: boolean) => void) | null;
}

const useAsking = create<Asking>(() => ({ question: null, answer: null }));

/**
 * Shows Rust's trash question and answers true when the user chose to
 * move. A question still open is answered no first.
 */
export function askTrash(question: Question): Promise<boolean> {
  useAsking.getState().answer?.(false);
  return new Promise((resolve) => useAsking.setState({ question, answer: resolve }));
}

/** Closes the question without moving anything (tests). */
export function resetTrashQuestion(): void {
  useAsking.getState().answer?.(false);
  useAsking.setState({ question: null, answer: null });
}

function answer(yes: boolean): void {
  const { answer: reply } = useAsking.getState();
  useAsking.setState({ question: null, answer: null });
  reply?.(yes);
}

/**
 * The question before moving photos to the Recycle Bin or Trash
 * (docs/06-editing-and-file-actions.md, "Move to the Recycle Bin or
 * Trash"): an alert dialog whose default button is Cancel, so Enter and
 * Esc both keep the photos. Its words come from Rust's `file_trash_ask`.
 */
export function TrashQuestion() {
  const question = useAsking((s) => s.question);
  return (
    <Dialog
      open={question !== null}
      alert
      size="sm"
      title={question?.message ?? ""}
      onClose={() => answer(false)}
      actions={
        <>
          <Button data-autofocus onClick={() => answer(false)}>
            {filesText.trashCancel}
          </Button>
          <Button danger onClick={() => answer(true)}>
            {question?.button}
          </Button>
        </>
      }
    >
      <p>{question?.detail}</p>
    </Dialog>
  );
}
