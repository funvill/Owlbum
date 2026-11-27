import { beforeEach, describe, expect, it } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { AlbumStatus } from "../../api/bindings/AlbumStatus";
import { appError, createFakeApi } from "../../api/fake";
import type { FakeApi, FakeHandlers } from "../../api/fake";
import { fakeStatus, pageOf } from "../../api/fake/album";
import { Toasts, useToasts } from "../../controls";
import { albumText as t } from "../../text/album";
import { seriousFindings } from "../../test/axe";
import { Notices } from "./Notices";
import { useAlbumStore } from "./session";

const initial = useAlbumStore.getState();

beforeEach(() => {
  useAlbumStore.setState(initial, true);
  useToasts.setState({ toasts: [] });
});

const banner = () => document.querySelector(".ow-banner");

const status = (over: Partial<AlbumStatus>) =>
  fakeStatus("D:\\Photos",{ scanning: false, ...over });

const heic = Array.from({ length: 3 }, (_, i) => ({
  rel_path: `2024/${i}.heic`,
  label: "HEIC",
  reason: "HEIC photos are not supported yet.",
}));
const broken = [
  {
    id: 9,
    rel_path: "2024/broken",
    code: "decode",
    message: "This file could not be read: bad bytes",
  },
];

function setup(s: AlbumStatus, handlers: Partial<FakeHandlers> = {}): FakeApi {
  const fake = createFakeApi({
    album_unsupported: ({ offset, limit }) => pageOf(heic, offset, limit),
    album_scan_errors: ({ offset, limit }) => pageOf(broken, offset, limit),
    ...handlers,
  });
  setApi(fake);
  useAlbumStore.setState({ status: s });
  render(
    <>
      <Notices />
      <Toasts />
    </>,
  );
  return fake;
}

describe("the notices", () => {
  it("say nothing when no file is left out", () => {
    setup(status({}));
    expect(banner()).toBeNull();
  });

  it("name each kind, in the singular for one file", () => {
    setup(
      status({ unsupported: { count: 1, labels: ["HEIC"] }, unreadable: 1 }),
    );
    expect(
      screen.getByRole("button", { name: "1 file not supported yet (HEIC)" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "1 file could not be read" }),
    ).toBeInTheDocument();
  });

  it("list the labels in order and write thousands separators", () => {
    setup(
      status({
        unsupported: { count: 2140, labels: ["HEIC", "AVIF"] },
        unreadable: 0,
      }),
    );
    expect(banner()).toHaveTextContent(
      "2,140 files not supported yet (HEIC, AVIF)",
    );
    expect(screen.queryByText(/could not be read/)).toBeNull();
  });

  it("hide with the x until the numbers change", async () => {
    setup(
      status({ unsupported: { count: 3, labels: ["HEIC"] }, unreadable: 0 }),
    );
    await userEvent.click(screen.getByRole("button", { name: /dismiss/i }));
    expect(banner()).toBeNull();
    act(() =>
      useAlbumStore.setState({
        status: status({
          unsupported: { count: 3, labels: ["HEIC"] },
          unreadable: 0,
          revision: 4,
        }),
      }),
    );
    expect(banner()).toBeNull();
    act(() =>
      useAlbumStore.setState({
        status: status({
          unsupported: { count: 3, labels: ["HEIC"] },
          unreadable: 2,
        }),
      }),
    );
    expect(banner()).toHaveTextContent(t.unreadableNotice(2));
  });

  it("open Files not shown with every file and its reason, and copy the list", async () => {
    const fake = setup(
      status({ unsupported: { count: 3, labels: ["HEIC"] }, unreadable: 1 }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: t.unreadableNotice(1) }),
    );
    const dialog = await screen.findByRole("dialog", { name: t.notShownTitle });
    const list = await within(dialog).findByRole("list");
    const items = within(list).getAllByRole("listitem");
    expect(items.map((i) => i.textContent)).toEqual([
      "2024/0.heicHEIC photos are not supported yet.",
      "2024/1.heicHEIC photos are not supported yet.",
      "2024/2.heicHEIC photos are not supported yet.",
      "2024/brokenThis file could not be read: bad bytes",
    ]);
    const close = within(dialog)
      .getAllByRole("button", { name: t.close })
      .at(-1)!;
    expect(close).toHaveFocus();
    expect(await seriousFindings(document.body)).toEqual([]);

    await userEvent.click(
      within(dialog).getByRole("button", { name: t.copyList }),
    );
    expect(
      fake.calls.filter((c) => c.command === "album_copy_not_shown"),
    ).toHaveLength(1);
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((x) => x.message)).toContain(
        t.listCopied,
      ),
    );

    await userEvent.click(close);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("read the list a page at a time until it is all there", async () => {
    const many = Array.from({ length: 1201 }, (_, i) => ({
      ...heic[0]!,
      rel_path: `${i}.heic`,
    }));
    const fake = setup(
      status({ unsupported: { count: 1201, labels: ["HEIC"] }, unreadable: 0 }),
      {
        album_unsupported: ({ offset, limit }) => pageOf(many, offset, limit),
        album_scan_errors: ({ offset, limit }) => pageOf([], offset, limit),
      },
    );
    await userEvent.click(
      screen.getByRole("button", { name: t.unsupportedNotice(1201, ["HEIC"]) }),
    );
    const dialog = await screen.findByRole("dialog");
    await waitFor(() =>
      expect(within(dialog).getAllByRole("listitem")).toHaveLength(1201),
    );
    expect(
      fake.calls.filter((c) => c.command === "album_unsupported"),
    ).toHaveLength(3);
  });

  it("report a failed copy", async () => {
    setup(
      status({ unsupported: { count: 3, labels: ["HEIC"] }, unreadable: 0 }),
      {
        album_copy_not_shown: () => {
          throw appError("clipboard_failed", "Couldn't copy the list: busy");
        },
      },
    );
    await userEvent.click(
      screen.getByRole("button", { name: t.unsupportedNotice(3, ["HEIC"]) }),
    );
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("list");
    await userEvent.click(
      within(dialog).getByRole("button", { name: t.copyList }),
    );
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((x) => x.message)).toContain(
        "Couldn't copy the list: busy",
      ),
    );
  });
});
