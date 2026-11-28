import type { ReactNode } from "react";
import { api, toAppError } from "../../api";
import type { UpdateMessage } from "../../api/bindings/UpdateMessage";
import { Banner, Button, Dialog, showToast } from "../../controls";
import { fatalText, updateText as u, whatsNewText as w } from "../../text/app";
import {
  answerNotice,
  closeMessage,
  closeWhatsNew,
  updateCommand,
  useLifecycle,
} from "./lifecycle";
import "./LifecycleDialogs.css";

/**
 * The first-start notice (docs/08-desktop-and-release.md, "What the first
 * start says"): no update check runs before it is answered.
 */
export function UpdateNotice() {
  const notice = useLifecycle((s) => s.notice);
  if (!notice) return null;
  return (
    <Banner
      tone="info"
      actions={
        <>
          <Button size="sm" onClick={() => void answerNotice(false)}>
            {u.turnOff}
          </Button>
          <Button size="sm" kind="primary" onClick={() => void answerNotice(true)}>
            {u.ok}
          </Button>
        </>
      }
    >
      {u.notice}
    </Banner>
  );
}

interface Shape {
  text: string;
  /** The buttons, the default first: it is drawn last, as the primary. */
  buttons: { label: string; act: () => void }[];
}

/** The words and buttons of one update message (docs/08, "Messages"). */
function shape(m: UpdateMessage): Shape {
  const close = { label: u.close, act: () => {} };
  const releasePage = { label: u.openReleasePage, act: () => updateCommand("app_open_release_page") };
  switch (m.kind) {
    case "found":
      return {
        text: u.found(m.latest, m.current),
        buttons: [
          { label: u.download, act: () => updateCommand("app_update_download") },
          // The offer stays open while the notes are read.
          { label: u.releaseNotes, act: () => updateCommand("app_open_release_notes") },
          { label: u.later, act: () => {} },
        ],
      };
    case "up_to_date":
      return { text: u.upToDate(m.current), buttons: [{ label: u.ok, act: () => {} }] };
    case "check_failed":
      return { text: u.checkFailed(m.reason), buttons: [{ label: u.ok, act: () => {} }] };
    case "dev_build":
      return { text: u.devBuild, buttons: [{ label: u.ok, act: () => {} }] };
    case "not_set_up":
      return { text: u.notSetUp, buttons: [releasePage, close] };
    case "download_failed":
      return {
        text: u.downloadFailed(m.reason),
        buttons: [{ label: u.tryAgain, act: () => updateCommand("app_update_download") }, close],
      };
    case "downloaded":
      return {
        text: u.downloaded(m.version),
        buttons: [
          { label: u.restartNow, act: () => updateCommand("app_update_install") },
          { label: u.later, act: () => {} },
        ],
      };
    case "cannot_install":
      return { text: u.cannotInstall(m.problem), buttons: [releasePage, close] };
    case "install_failed":
      return { text: u.installFailed(m.reason), buttons: [releasePage, { label: u.ok, act: () => {} }] };
  }
}

/** An update message, as a page dialog titled Owlbum. */
export function UpdateMessageDialog() {
  const message = useLifecycle((s) => s.message);
  if (message === null) return null;
  const { text, buttons } = shape(message);
  const keepsOpen = (label: string) => label === u.releaseNotes;
  return (
    <Dialog
      open
      title={u.title}
      onClose={closeMessage}
      actions={
        <>
          {[...buttons].reverse().map((b, i) => (
            <Button
              key={b.label}
              kind={i === buttons.length - 1 ? "primary" : "default"}
              data-autofocus={i === buttons.length - 1 ? true : undefined}
              onClick={() => {
                b.act();
                if (!keepsOpen(b.label)) closeMessage();
              }}
            >
              {b.label}
            </Button>
          ))}
        </>
      }
    >
      <p>{text}</p>
    </Dialog>
  );
}

/** What's new: one bullet per item, under its version when there are several. */
export function WhatsNewDialog() {
  const shown = useLifecycle((s) => s.whatsNew);
  const version = useLifecycle((s) => s.version);
  if (shown === null) return null;
  const several = shown.entries.length > 1;
  const newest = shown.entries[0]?.version ?? version;
  return (
    <Dialog
      open
      size="wide"
      title={w.title(newest)}
      onClose={closeWhatsNew}
      actions={
        <Button kind="primary" data-autofocus onClick={closeWhatsNew}>
          {w.ok}
        </Button>
      }
    >
      <div className="ow-whats-new">
        {shown.entries.length === 0 && <p>{w.nothing}</p>}
        {shown.entries.map((entry) => (
          <section key={entry.version}>
            {several && <h3 className="ow-whats-new__version">{entry.version}</h3>}
            {entry.sections.map((s) => (
              <div key={s.heading}>
                <h4 className="ow-whats-new__heading">{s.heading}</h4>
                <ul>
                  {s.items.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </div>
            ))}
          </section>
        ))}
      </div>
    </Dialog>
  );
}

function run(command: "app_copy_details" | "app_open_log_folder" | "app_restart" | "app_quit", done?: string) {
  api()
    .invoke(command)
    .then(
      () => {
        if (done !== undefined) showToast({ kind: "info", message: done });
      },
      (e: unknown) => showToast({ kind: "error", message: toAppError(e).message }),
    );
}

/**
 * A fatal error of phase `run`, drawn over everything (docs/08, "The fatal
 * error dialog"). The technical block stays in Rust: Copy details asks
 * Rust to put it on the clipboard.
 */
export function FatalDialog() {
  const fatal = useLifecycle((s) => s.fatal);
  if (fatal === null) return null;
  const [message, ...advice] = fatal.message.split("\n\n");
  const paragraphs: ReactNode[] = [message, ...advice].map((p, i) => <p key={i}>{p}</p>);
  return (
    <Dialog
      open
      alert
      dirty
      title={fatal.title}
      onClose={() => run("app_quit")}
      actions={
        <>
          <Button onClick={() => run("app_copy_details", fatalText.copied)}>{fatalText.copyDetails}</Button>
          <Button onClick={() => run("app_open_log_folder")}>{fatalText.openLogFolder}</Button>
          <Button onClick={() => run("app_quit")}>{fatalText.close}</Button>
          <Button kind="primary" data-autofocus onClick={() => run("app_restart")}>
            {fatalText.restart}
          </Button>
        </>
      }
    >
      {paragraphs}
    </Dialog>
  );
}
