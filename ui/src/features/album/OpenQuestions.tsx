import { Button, Dialog } from "../../controls";
import { albumText } from "../../text/album";
import { useAlbumStore } from "./session";

/**
 * The questions `album_open` can ask (docs/04-browsing.md, "Opening a
 * folder"). The sentence is the error's message, from Rust. For the stale
 * lock, Esc and the backdrop choose Open view-only, the safe answer, which
 * has the focus; for the damaged database they cancel.
 */
export function OpenQuestions() {
  const question = useAlbumStore((s) => s.question);
  const answerStale = useAlbumStore((s) => s.answerStale);
  const answerDamaged = useAlbumStore((s) => s.answerDamaged);

  if (question?.kind === "stale") {
    return (
      <Dialog
        open
        title={albumText.staleTitle}
        onClose={() => void answerStale("view_only")}
        actions={
          <>
            <Button data-autofocus onClick={() => void answerStale("view_only")}>
              {albumText.openViewOnly}
            </Button>
            <Button kind="primary" onClick={() => void answerStale("anyway")}>
              {albumText.openAnyway}
            </Button>
          </>
        }
      >
        <p>{question.message}</p>
      </Dialog>
    );
  }
  if (question?.kind === "damaged") {
    return (
      <Dialog
        open
        title={albumText.damagedTitle}
        onClose={() => void answerDamaged("cancel")}
        actions={
          <>
            <Button data-autofocus onClick={() => void answerDamaged("cancel")}>
              {albumText.cancel}
            </Button>
            <Button onClick={() => void answerDamaged("view_only")}>{albumText.openViewOnly}</Button>
            {question.hasBackup && (
              <Button kind="primary" onClick={() => void answerDamaged("restore")}>
                {albumText.restoreBackup}
              </Button>
            )}
          </>
        }
      >
        <p>{question.message}</p>
      </Dialog>
    );
  }
  return null;
}
