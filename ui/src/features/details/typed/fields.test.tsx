import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../../api";
import type { InfoPatch } from "../../../api/bindings/InfoPatch";
import type { PhotoDetail } from "../../../api/bindings/PhotoDetail";
import { appError, createFakeApi } from "../../../api/fake";
import type { FakeApi, FakeHandlers } from "../../../api/fake";
import { fakeStatus } from "../../../api/fake/album";
import { defaultSettings } from "../../../api/fake/app";
import { fakeDetail } from "../../../api/fake/photo";
import { useToasts } from "../../../controls";
import { useAlbumStore } from "../../album";
import { useSettingsStore } from "../../app";
import { DetailsPanel, type CurrentPhoto } from "../DetailsPanel";

const initialAlbum = useAlbumStore.getState();

beforeEach(() => {
  useAlbumStore.setState(initialAlbum, true);
  useSettingsStore.setState({
    settings: structuredClone(defaultSettings),
    error: null,
  });
  useToasts.setState({ toasts: [] });
});

afterEach(() => {
  vi.useRealTimers();
});

function setup(current: CurrentPhoto | null, overrides: Partial<FakeHandlers> = {}) {
  const fake = createFakeApi(overrides);
  setApi(fake);
  const view = (c: CurrentPhoto | null) => <DetailsPanel current={c} onNotFound={() => {}} />;
  const { rerender } = render(view(current));
  return { fake, show: (c: CurrentPhoto | null) => rerender(view(c)) };
}

function patches(fake: FakeApi): { ids: number[]; patch: InfoPatch }[] {
  return fake.calls
    .filter((c) => c.command === "photo_set_info")
    .map((c) => c.args as { ids: number[]; patch: InfoPatch });
}

function withDetail(changes: Partial<PhotoDetail>): Partial<FakeHandlers> {
  return { photo_detail: ({ id }) => fakeDetail(id, changes) };
}

describe("typed fields", () => {
  it("saves the description on blur, trimmed, and shows Saved for 1,800 ms", async () => {
    const user = userEvent.setup();
    const { fake } = setup({ id: 1 });
    const box = await screen.findByRole("textbox", { name: "Description" });
    await user.type(box, "  At the tower {Enter}");
    expect(patches(fake)).toEqual([{ ids: [1], patch: { description: "At the tower" } }]);
    const status = await screen.findByText("Saved");
    expect(status).toHaveAttribute("role", "status");
    await waitFor(() => expect(screen.queryByText("Saved")).toBeNull(), {
      timeout: 2500,
    });
  });

  it("does not send an unchanged value, and sends null for emptied text", async () => {
    const user = userEvent.setup();
    const { fake } = setup({ id: 1 }, withDetail({ notes: "old" }));
    const notes = await screen.findByRole("textbox", { name: "Notes" });
    await user.click(notes);
    await user.tab();
    expect(patches(fake)).toEqual([]);
    await user.clear(notes);
    await user.tab();
    expect(patches(fake)).toEqual([{ ids: [1], patch: { notes: null } }]);
  });

  it("adds a line with Enter in notes instead of saving", async () => {
    const user = userEvent.setup();
    const { fake } = setup({ id: 1 });
    const notes = await screen.findByRole("textbox", { name: "Notes" });
    await user.type(notes, "a{Enter}b");
    expect(patches(fake)).toEqual([]);
    await user.tab();
    expect(patches(fake)).toEqual([{ ids: [1], patch: { notes: "a\nb" } }]);
  });

  it("keeps the text and shows a toast and a note when a save fails", async () => {
    const user = userEvent.setup();
    setup(
      { id: 1 },
      {
        photo_set_info: () => {
          throw appError("internal", "Something went wrong while saving to the album database.");
        },
      },
    );
    const box = await screen.findByRole("textbox", { name: "Description" });
    await user.type(box, "Kept{Enter}");
    await waitFor(() => expect(useToasts.getState().toasts).toHaveLength(1));
    expect(useToasts.getState().toasts[0]).toMatchObject({ kind: "error" });
    expect(await screen.findByText("Something went wrong while saving to the album database.")).toBeInTheDocument();
    expect(box).toHaveValue("Kept");
  });

  it("keeps a draft when an answer arrives with a new saved value", async () => {
    const user = userEvent.setup();
    let notes: string | null = null;
    const { fake } = setup(
      { id: 1 },
      {
        photo_detail: ({ id }) => fakeDetail(id, { notes, revision: notes === null ? 1 : 5 }),
      },
    );
    const box = await screen.findByRole("textbox", { name: "Notes" });
    await user.type(box, "typing");
    notes = "from elsewhere";
    act(() => fake.emit("album_changed", { revision: 5, reason: "info" }));
    await waitFor(() => expect(fake.calls.filter((c) => c.command === "photo_detail")).toHaveLength(2));
    expect(box).toHaveValue("typing");
  });

  it("saves changed text once when the photo changes without a blur", async () => {
    const user = userEvent.setup();
    const { fake, show } = setup({ id: 1 });
    const box = await screen.findByRole("textbox", { name: "Description" });
    await user.type(box, "Unsaved");
    show({ id: 2 });
    await screen.findByText(/IMG_0002\.jpg$/);
    expect(patches(fake)).toEqual([{ ids: [1], patch: { description: "Unsaved" } }]);
  });

  it("does not send an invalid date when leaving, and shows the date message on blur", async () => {
    const user = userEvent.setup();
    const { fake, show } = setup({ id: 1 });
    const date = await screen.findByRole("textbox", {
      name: "Date and time taken",
    });
    expect(date).toHaveValue("2024-06-12 14:03:00");
    await user.clear(date);
    await user.type(date, "2024-02-30");
    await user.tab();
    expect(await screen.findByText("That is not a valid date and time. Use YYYY-MM-DD HH:MM.")).toBeInTheDocument();
    expect(patches(fake)).toEqual([]);
    show({ id: 2 });
    await screen.findByText(/IMG_0002\.jpg$/);
    expect(patches(fake)).toEqual([]);
  });

  it("sends a typed date, and Use the camera's date clears it", async () => {
    const user = userEvent.setup();
    const { fake } = setup(
      { id: 1 },
      withDetail({
        date: {
          effective: "2020-01-01T10:00:00",
          camera: "2024-06-12T14:03:00",
          typed: "2020-01-01T10:00:00",
        },
      }),
    );
    const date = await screen.findByRole("textbox", {
      name: "Date and time taken",
    });
    await user.clear(date);
    await user.type(date, "2021-05-05 08:00{Enter}");
    await user.click(screen.getByRole("button", { name: "Use the camera's date" }));
    expect(patches(fake).map((p) => p.patch)).toEqual([
      { date_override: "2021-05-05T08:00:00" },
      { date_override: null },
    ]);
  });

  it("reads Clear the date when the file has no date", async () => {
    setup(
      { id: 1 },
      withDetail({
        date: {
          effective: "2020-01-01T10:00:00",
          camera: null,
          typed: "2020-01-01T10:00:00",
        },
      }),
    );
    expect(await screen.findByRole("button", { name: "Clear the date" })).toBeInTheDocument();
  });

  it("adds people with Enter, commas and blur, removes with x and Backspace", async () => {
    const user = userEvent.setup();
    const { fake } = setup({ id: 1 }, { album_people: () => [{ name: "Ann Lee", count: 4 }] });
    const box = await screen.findByRole("combobox", { name: "People" });
    await user.type(box, "ann lee{Enter}");
    await user.type(box, "Bob, Cy");
    await user.tab();
    expect(patches(fake).map((p) => p.patch)).toEqual([
      { people: ["Ann Lee"] },
      { people: ["Ann Lee", "Bob"] },
      { people: ["Ann Lee", "Bob", "Cy"] },
    ]);
    await user.click(screen.getByRole("button", { name: /Bob/ }));
    expect(patches(fake).at(-1)?.patch).toEqual({ people: ["Ann Lee", "Cy"] });
    await user.click(box);
    await user.keyboard("{Backspace}{Backspace}");
    expect(patches(fake).at(-1)?.patch).toEqual({ people: null });
  });

  it("suggests the album's names not on the photo and narrows them as you type", async () => {
    const user = userEvent.setup();
    const { fake } = setup(
      { id: 1 },
      {
        ...withDetail({ people: ["Bob"] }),
        album_people: () => [
          { name: "Bob", count: 9 },
          { name: "Ann Lee", count: 4 },
          { name: "Émile", count: 2 },
        ],
      },
    );
    const box = await screen.findByRole("combobox", { name: "People" });
    await user.click(box);
    const list = await screen.findByRole("listbox", {
      name: "Names used in this album",
    });
    expect(
      within(list)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Ann Lee", "Émile"]);
    await user.type(box, "emi");
    await user.keyboard("{ArrowDown}{Enter}");
    expect(patches(fake).at(-1)?.patch).toEqual({ people: ["Bob", "Émile"] });
  });

  it("saves the location group when focus leaves it, with a message for bad input", async () => {
    const user = userEvent.setup();
    const { fake } = setup({ id: 1 });
    const group = await screen.findByRole("group", { name: "Location" });
    const lat = within(group).getByRole("textbox", { name: "Latitude" });
    const name = within(group).getByRole("textbox", { name: "Place name" });
    await user.clear(lat);
    await user.type(lat, "abc");
    await user.click(name);
    expect(patches(fake)).toEqual([]);
    await user.click(screen.getByRole("textbox", { name: "Notes" }));
    expect(await screen.findByText("Latitude must be a number.")).toBeInTheDocument();
    await user.clear(lat);
    await user.type(lat, "10,5");
    await user.type(name, "Spot{Enter}");
    expect(patches(fake).at(-1)?.patch).toEqual({
      location: { lat: 10.5, lon: 2.29448, name: "Spot" },
    });
  });

  it("disables every field in a view-only album", async () => {
    useAlbumStore.setState({
      status: fakeStatus("D:/Photos", { view_only: "read_only" }),
    });
    setup(
      { id: 1 },
      withDetail({
        date: {
          effective: "2020-01-01T10:00:00",
          camera: "2024-06-12T14:03:00",
          typed: "2020-01-01T10:00:00",
        },
      }),
    );
    await screen.findByRole("textbox", { name: "Description" });
    const panel = screen.getByRole("complementary", { name: "Details" });
    const controls = [
      ...within(panel).getAllByRole("textbox"),
      ...within(panel).getAllByRole("combobox"),
      screen.getByRole("button", { name: "Use the camera's date" }),
    ];
    expect(controls.length).toBeGreaterThan(6);
    for (const c of controls) expect(c).toBeDisabled();
  });
});
