import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { TidyFolder } from "../../api/bindings/TidyFolder";
import { appError, createFakeApi, type FakeApi, type FakeHandlers } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { Toasts, useToasts } from "../../controls";
import { albumText as t } from "../../text/album";
import { seriousFindings } from "../../test/axe";
import { useGridStore } from "../grid";
import { TidyDialog, defaultTicks } from "./TidyDialog";
import { TopBar } from "./TopBar";
import { useAlbumStore } from "./session";

const initial = useAlbumStore.getState();

const folders: TidyFolder[] = [
  {
    folder: "Day 1",
    gone: false,
    files: [
      { id: 1, file_name: "IMG_0412.jpg", keeps: [] },
      { id: 2, file_name: "IMG_0413.jpg", keeps: [] },
    ],
  },
  {
    folder: "Day 2",
    gone: false,
    files: [
      { id: 3, file_name: "IMG_0507.jpg", keeps: [] },
      {
        id: 4,
        file_name: "IMG_0511.jpg",
        keeps: ["starred", "notes", "2 people"],
      },
    ],
  },
  {
    folder: "Scans",
    gone: true,
    files: [{ id: 5, file_name: "s.jpg", keeps: [] }],
  },
];

beforeEach(() => {
  useAlbumStore.setState(initial, true);
  useAlbumStore.setState({
    status: fakeStatus("C:\\Photos", { scanning: false }),
  });
  useToasts.setState({ toasts: [] });
});

function show(overrides: Partial<FakeHandlers> = {}): {
  fake: FakeApi;
  onClose: ReturnType<typeof vi.fn>;
} {
  const fake = createFakeApi(overrides);
  setApi(fake);
  const onClose = vi.fn();
  render(
    <>
      <TidyDialog open onClose={onClose} />
      <Toasts />
    </>,
  );
  return { fake, onClose };
}

/** Waits for the list; generous, since a loaded machine can be slow to render it. */
const listed = () => screen.findByText(t.tidyExplain, undefined, { timeout: 4000 });
const box = (name: string) => screen.getByRole("checkbox", { name });
const removeButton = () => screen.getByRole("button", { name: /^Remove \d+ entr/ });

describe("Tidy dialog", () => {
  it("says it is looking, then that there is nothing to tidy", async () => {
    let answer!: (f: TidyFolder[]) => void;
    show({
      album_tidy_list: () => new Promise<TidyFolder[]>((r) => (answer = r)),
    });
    expect(screen.getByText(t.tidyLooking)).toBeInTheDocument();
    await act(async () => answer([]));
    expect(screen.getByText(t.tidyNothing)).toBeInTheDocument();
    // The x and the Close button.
    expect(screen.getAllByRole("button", { name: t.close })).toHaveLength(2);
  });

  it("does not list while a scan runs", async () => {
    useAlbumStore.setState({
      status: fakeStatus("C:\\Photos", { scanning: true }),
    });
    const { fake } = show();
    expect(await screen.findByText(t.tidyScanning)).toBeInTheDocument();
    expect(fake.calls.some((c) => c.command === "album_tidy_list")).toBe(false);
  });

  it("shows Rust's sentence when the album folder can't be reached", async () => {
    show({
      album_tidy_list: () => {
        throw appError("album_unreachable", "The album folder can't be reached. Nothing was removed.");
      },
    });
    expect(await screen.findByText("The album folder can't be reached. Nothing was removed.")).toBeInTheDocument();
  });

  it("ticks only files that lose nothing typed, outside gone folders", async () => {
    expect([...defaultTicks(folders)].sort()).toEqual([1, 2, 3]);
    show({ album_tidy_list: () => structuredClone(folders) });
    await listed();
    expect(box("Day 1")).toBeChecked();
    expect(box("IMG_0511.jpg")).not.toBeChecked();
    expect(box("Day 2")).toHaveProperty("indeterminate", true);
    expect(screen.getByText("starred · notes · 2 people")).toBeInTheDocument();
    expect(box("Scans")).not.toBeChecked();
    expect(screen.getByText(t.tidyFolderGone)).toBeInTheDocument();
    expect(screen.getByText(t.tidySelected(1, 2))).toBeInTheDocument();
    expect(screen.getByText(t.tidySelected(0, 1))).toBeInTheDocument();
    expect(removeButton()).toHaveTextContent("Remove 3 entries");
    expect(await seriousFindings()).toEqual([]);
  });

  it("a folder's box ticks or clears its files, and the count follows in the singular", async () => {
    const user = userEvent.setup();
    show({ album_tidy_list: () => structuredClone(folders) });
    await listed();
    await user.click(box("Day 1"));
    expect(box("IMG_0412.jpg")).not.toBeChecked();
    expect(removeButton()).toHaveTextContent("Remove 1 entry");
    await user.click(box("IMG_0507.jpg"));
    expect(removeButton()).toHaveTextContent("Remove 0 entries");
    expect(removeButton()).toHaveAttribute("aria-disabled", "true");
    await user.click(box("Scans"));
    expect(box("s.jpg")).toBeChecked();
  });

  it("removes the ticked rows, closes and says how many", async () => {
    const user = userEvent.setup();
    const { fake, onClose } = show({
      album_tidy_list: () => structuredClone(folders),
      album_tidy_remove: ({ ids }) => ({
        removed: ids.length,
        kept: 0,
        revision: 2,
      }),
    });
    await listed();
    await user.click(removeButton());
    const call = fake.calls.find((c) => c.command === "album_tidy_remove");
    expect((call?.args as { ids: number[] }).ids.sort()).toEqual([1, 2, 3]);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(useToasts.getState().toasts.map((x) => x.message)).toEqual(["Removed 3 entries."]);
  });

  it("says when rows were kept because their files came back, and shows them", async () => {
    const user = userEvent.setup();
    const refresh = vi.spyOn(useGridStore.getState(), "refresh").mockResolvedValue();
    show({
      album_tidy_list: () => structuredClone(folders),
      album_tidy_remove: () => ({ removed: 1, kept: 2, revision: 2 }),
    });
    await listed();
    await user.click(removeButton());
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((x) => x.message)).toEqual([
        "Removed 1 entry. 2 photos were found again and kept.",
      ]),
    );
    expect(refresh).toHaveBeenCalled();
    refresh.mockRestore();
  });

  it("on an error shows it and loads the list again", async () => {
    const user = userEvent.setup();
    const { fake, onClose } = show({
      album_tidy_list: () => structuredClone(folders),
      album_tidy_remove: () => {
        throw appError("scan_running", "Owlbum is still reading the album. Tidy when it has finished.");
      },
    });
    await listed();
    await user.click(removeButton());
    await waitFor(() => expect(fake.calls.filter((c) => c.command === "album_tidy_list")).toHaveLength(2));
    expect(onClose).not.toHaveBeenCalled();
    expect(useToasts.getState().toasts[0]?.kind).toBe("error");
  });
});

describe("Tidy button", () => {
  it("opens the dialog", async () => {
    const user = userEvent.setup();
    setApi(createFakeApi({ album_tidy_list: () => [] }));
    render(<TopBar />);
    await user.click(screen.getByRole("button", { name: /Tidy/ }));
    const dialog = await screen.findByRole("dialog", { name: t.tidyTitle });
    expect(await within(dialog).findByText(t.tidyNothing)).toBeInTheDocument();
  });

  it("is disabled in a view-only album", () => {
    setApi(createFakeApi());
    useAlbumStore.setState({
      status: fakeStatus("C:\\Photos", {
        view_only: "chosen",
        scanning: false,
      }),
    });
    render(<TopBar />);
    expect(screen.getByRole("button", { name: /Tidy/ })).toHaveAttribute("aria-disabled", "true");
  });
});
