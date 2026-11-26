import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Dialog } from "./Dialog";
import { Button } from "./Button";
import { TextField } from "./fields";
import { layerRoot } from "./layers";
import { seriousFindings } from "../test/axe";

function Harness({ dirty = false, alert = false }: { dirty?: boolean; alert?: boolean }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  return (
    <>
      <Button onClick={() => setOpen(true)}>Rename</Button>
      <Dialog
        open={open}
        title="Rename album"
        alert={alert}
        dirty={dirty}
        onClose={() => setOpen(false)}
        actions={
          <>
            <Button onClick={() => setOpen(false)}>Keep</Button>
            <Button kind="primary" onClick={() => setOpen(false)}>
              Rename
            </Button>
          </>
        }
      >
        <TextField label="Name" value={name} onChange={setName} />
      </Dialog>
    </>
  );
}

async function openDialog() {
  await userEvent.click(screen.getByRole("button", { name: "Rename" }));
  const dialog = document.querySelector<HTMLElement>('[aria-modal="true"]');
  if (dialog === null) throw new Error("no dialog");
  return dialog;
}

describe("Dialog", () => {
  it("is a modal dialog named by its title, drawn in the layer root", async () => {
    render(<Harness />);
    const dialog = await openDialog();
    expect(dialog).toHaveAttribute("role", "dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleName("Rename album");
    expect(layerRoot().contains(dialog)).toBe(true);
  });

  it("is an alert dialog when it only confirms a deletion", async () => {
    render(<Harness alert />);
    expect(await openDialog()).toHaveAttribute("role", "alertdialog");
  });

  it("puts focus inside, keeps it there, and makes the rest of the page inert", async () => {
    const { container } = render(<Harness />);
    const dialog = await openDialog();
    const inside = within(dialog);
    expect(inside.getByRole("textbox", { name: "Name" })).toHaveFocus();
    expect(container.closest("[inert]")).not.toBeNull();

    // The last control is the primary button; Tab from it wraps to the x.
    const last = inside.getByRole("button", { name: "Rename" });
    last.focus();
    await userEvent.tab();
    expect(inside.getByRole("button", { name: "Close" })).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(last).toHaveFocus();
  });

  it("closes on Escape and returns focus to the control that opened it", async () => {
    const { container } = render(<Harness />);
    await openDialog();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: "Rename" })).toHaveFocus();
    expect(container.closest("[inert]")).toBeNull();
  });

  it("closes from its x button", async () => {
    render(<Harness />);
    await openDialog();
    const dialog = document.querySelector<HTMLElement>('[aria-modal="true"]')!;
    await userEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes when the backdrop is clicked, unless it holds unsaved input", async () => {
    const { unmount } = render(<Harness />);
    await openDialog();
    await userEvent.click(document.querySelector(".ow-dialog-backdrop")!);
    expect(screen.queryByRole("dialog")).toBeNull();
    unmount();

    render(<Harness dirty />);
    const dialog = await openDialog();
    await userEvent.click(document.querySelector(".ow-dialog-backdrop")!);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // Focus stays in the dialog, so Escape still reaches it.
    expect(dialog.contains(document.activeElement)).toBe(true);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("does not close when a click inside it ends on the backdrop", async () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Settings" onClose={onClose}>
        <p>Body</p>
      </Dialog>,
    );
    await userEvent.click(screen.getByText("Body"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("has no serious accessibility findings", async () => {
    render(<Harness />);
    await openDialog();
    expect(await seriousFindings(layerRoot())).toEqual([]);
  });
});
