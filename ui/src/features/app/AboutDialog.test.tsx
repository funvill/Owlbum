import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { setApi } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { aboutText as t } from "../../text/app";
import { seriousFindings } from "../../test/axe";
import { AboutDialog } from "./AboutDialog";

let fake: FakeApi;

beforeEach(() => {
  fake = createFakeApi();
  setApi(fake);
});

describe("the About box", () => {
  it("shows the logo, the version, the data folder and the licence", async () => {
    render(<AboutDialog open onClose={() => {}} />);
    const dialog = screen.getByRole("dialog", { name: t.title });
    expect(await screen.findByText(t.name("0.0.0-fake"))).toBeInTheDocument();
    expect(screen.getByText(t.versionLine("0.0.0-fake", "windows", "x86_64"))).toBeInTheDocument();
    expect(screen.getByText("C:\\Users\\sam\\AppData\\Local\\OwlbumData")).toBeInTheDocument();
    expect(screen.getByText("Copyright (c) 2026 Owlbum contributors")).toBeInTheDocument();
    expect(dialog.querySelector("img")).not.toBeNull();
    // OK is focused, so Enter or Esc closes the box.
    expect(screen.getByRole("button", { name: t.ok })).toHaveFocus();
    expect(await seriousFindings()).toEqual([]);
  });

  it("opens the repository and the feedback page through Rust, never by itself", async () => {
    const user = userEvent.setup();
    render(<AboutDialog open onClose={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "https://github.com/funvill/Owlbum" }));
    await user.click(screen.getByRole("button", { name: t.feedback }));
    const opened = fake.calls.map((c) => c.command).filter((c) => c.startsWith("app_open_"));
    expect(opened).toEqual(["app_open_repository", "app_open_feedback"]);
  });

  it("closes on OK", async () => {
    const user = userEvent.setup();
    let closed = false;
    render(<AboutDialog open onClose={() => (closed = true)} />);
    await user.click(screen.getByRole("button", { name: t.ok }));
    expect(closed).toBe(true);
  });
});
