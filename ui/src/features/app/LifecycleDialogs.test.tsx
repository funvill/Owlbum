import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { setApi } from "../../api";
import type { Startup } from "../../api/bindings/Startup";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { aboutText, fatalText, updateText as u, whatsNewText as w } from "../../text/app";
import { seriousFindings } from "../../test/axe";
import { AboutDialog } from "./AboutDialog";
import { listenLifecycle, openWhatsNew, startLifecycle, useLifecycle } from "./lifecycle";
import { FatalDialog, UpdateMessageDialog, UpdateNotice, WhatsNewDialog } from "./LifecycleDialogs";
import { describe as describeError, followPageErrors } from "./pageLog";

let fake: FakeApi;
let stop: (() => void)[] = [];

const initial = useLifecycle.getState();

beforeEach(async () => {
  fake = createFakeApi();
  setApi(fake);
  useLifecycle.setState(initial, true);
  stop = await listenLifecycle();
});

afterEach(() => {
  for (const s of stop) s();
});

function commands(prefix: string): string[] {
  return fake.calls.map((c) => c.command).filter((c) => c.startsWith(prefix));
}

const entry = (version: string, items: string[]) => ({ version, sections: [{ heading: "Added", items }] });

function startup(extra: Partial<Startup>): Startup {
  return {
    version: "0.2.0",
    platform: "windows",
    arch: "x86_64",
    package: "nsis",
    dev: false,
    settings: {} as Startup["settings"],
    recent: [],
    launch_folder: null,
    reopen: null,
    show_update_notice: false,
    whats_new: [],
    ...extra,
  };
}

describe("the first-start update notice", () => {
  it("is shown until answered, and Turn off turns the daily check off", async () => {
    const user = userEvent.setup();
    act(() => startLifecycle(startup({ show_update_notice: true })));
    render(<UpdateNotice />);
    expect(screen.getByText(u.notice)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: u.turnOff }));
    expect(screen.queryByText(u.notice)).toBeNull();
    const answer = fake.calls.find((c) => c.command === "app_update_notice_answer");
    expect(answer?.args).toEqual({ keep_on: false });
  });

  it("OK keeps it on", async () => {
    const user = userEvent.setup();
    act(() => startLifecycle(startup({ show_update_notice: true })));
    render(<UpdateNotice />);
    await user.click(screen.getByRole("button", { name: u.ok }));
    expect(fake.calls.find((c) => c.command === "app_update_notice_answer")?.args).toEqual({ keep_on: true });
  });

  it("is not shown once answered", () => {
    act(() => startLifecycle(startup({ show_update_notice: false })));
    render(<UpdateNotice />);
    expect(screen.queryByText(u.notice)).toBeNull();
  });
});

describe("update messages", () => {
  it("offers a found version: Download is the default and starts the download", async () => {
    const user = userEvent.setup();
    render(<UpdateMessageDialog />);
    act(() => fake.emit("update_message", { kind: "found", latest: "0.3.0", current: "0.2.0" }));
    expect(screen.getByText(u.found("0.3.0", "0.2.0"))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: u.download })).toHaveFocus();
    expect(await seriousFindings()).toEqual([]);
    // Release notes keeps the offer open.
    await user.click(screen.getByRole("button", { name: u.releaseNotes }));
    expect(screen.getByText(u.found("0.3.0", "0.2.0"))).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: u.download }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(commands("app_")).toContain("app_open_release_notes");
    expect(commands("app_update_")).toEqual(["app_update_state", "app_update_download"]);
  });

  it("asks to restart when downloaded, and Later keeps it", async () => {
    const user = userEvent.setup();
    render(<UpdateMessageDialog />);
    act(() => fake.emit("update_message", { kind: "downloaded", version: "0.3.0" }));
    expect(screen.getByText(u.downloaded("0.3.0"))).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: u.later }));
    expect(commands("app_update_install")).toEqual([]);
    act(() => fake.emit("update_message", { kind: "downloaded", version: "0.3.0" }));
    await user.click(screen.getByRole("button", { name: u.restartNow }));
    expect(commands("app_update_install")).toEqual(["app_update_install"]);
  });

  it("says when this copy cannot update itself", async () => {
    const user = userEvent.setup();
    render(<UpdateMessageDialog />);
    act(() => fake.emit("update_message", { kind: "not_set_up" }));
    expect(screen.getByText(u.notSetUp)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: u.openReleasePage }));
    expect(commands("app_open_release_page")).toEqual(["app_open_release_page"]);
  });

  it("follows the updater's state", () => {
    act(() => fake.emit("update_state", { state: "downloading", version: "0.3.0", percent: 40 }));
    expect(useLifecycle.getState().update.percent).toBe(40);
  });
});

describe("What's new", () => {
  it("shows the versions since the last start and records them when closed", async () => {
    const user = userEvent.setup();
    act(() => startLifecycle(startup({ whats_new: [entry("0.2.0", ["Two."]), entry("0.1.5", ["One and a half."])] })));
    render(<WhatsNewDialog />);
    expect(screen.getByRole("dialog", { name: w.title("0.2.0") })).toBeInTheDocument();
    expect(screen.getByText("Two.")).toBeInTheDocument();
    expect(screen.getByText("0.1.5")).toBeInTheDocument();
    expect(commands("app_whats_new_seen")).toEqual([]);
    await user.click(screen.getByRole("button", { name: w.ok }));
    expect(commands("app_whats_new_seen")).toEqual(["app_whats_new_seen"]);
  });

  it("Help > What's new shows this version's notes and records nothing", async () => {
    const user = userEvent.setup();
    act(() => startLifecycle(startup({})));
    render(<WhatsNewDialog />);
    await act(() => openWhatsNew());
    expect(screen.getByText(w.nothing)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: w.ok }));
    expect(commands("app_whats_new_seen")).toEqual([]);
  });
});

describe("the fatal error dialog", () => {
  it("shows the message and the advice, and copies the details through Rust", async () => {
    const user = userEvent.setup();
    render(<FatalDialog />);
    act(() => fake.emit("app_fatal", { title: "Owlbum stopped working", message: "It stopped.\n\nRestart Owlbum." }));
    expect(screen.getByRole("alertdialog", { name: "Owlbum stopped working" })).toBeInTheDocument();
    expect(screen.getByText("Restart Owlbum.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: fatalText.restart })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: fatalText.copyDetails }));
    await user.click(screen.getByRole("button", { name: fatalText.openLogFolder }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: fatalText.restart }));
    expect(commands("app_")).toEqual(
      expect.arrayContaining(["app_copy_details", "app_open_log_folder", "app_restart"]),
    );
  });
});

describe("the About box's buttons", () => {
  it("opens the licence and the notices, and hides Check for updates while unavailable", async () => {
    const user = userEvent.setup();
    render(<AboutDialog open onClose={() => {}} />);
    await user.click(await screen.findByRole("button", { name: aboutText.notices }));
    await user.click(screen.getByRole("button", { name: aboutText.licence }));
    expect(commands("app_open_")).toEqual(["app_open_notices", "app_open_licence"]);
    expect(screen.queryByRole("button", { name: aboutText.checkForUpdates })).toBeNull();
  });

  it("closes and checks when the updater is set up", async () => {
    const user = userEvent.setup();
    let closed = false;
    act(() => fake.emit("update_state", { state: "idle", version: null, percent: null }));
    render(<AboutDialog open onClose={() => (closed = true)} />);
    await user.click(screen.getByRole("button", { name: aboutText.checkForUpdates }));
    expect(closed).toBe(true);
    expect(commands("app_check_updates")).toEqual(["app_check_updates"]);
  });
});

describe("the page's errors", () => {
  it("go to the log through app_log", () => {
    const target = new EventTarget() as unknown as Window;
    const stopFollowing = followPageErrors(target);
    target.dispatchEvent(Object.assign(new Event("error"), { error: new Error("boom"), message: "boom" }));
    stopFollowing();
    target.dispatchEvent(Object.assign(new Event("error"), { error: new Error("later"), message: "later" }));
    const logged = fake.calls.filter((c) => c.command === "app_log");
    expect(logged).toHaveLength(1);
    expect(logged[0]?.args).toMatchObject({ level: "error" });
    expect(JSON.stringify(logged[0]?.args)).toContain("boom");
    expect(describeError("plain")).toBe("plain");
  });
});
