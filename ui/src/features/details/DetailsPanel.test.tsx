import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { PhotoDetail } from "../../api/bindings/PhotoDetail";
import { appError, createFakeApi } from "../../api/fake";
import type { FakeApi, FakeHandlers } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { defaultSettings } from "../../api/fake/app";
import { fakeDetail } from "../../api/fake/photo";
import { seriousFindings } from "../../test/axe";
import { useAlbumStore } from "../album";
import { useSettingsStore } from "../app";
import { DetailsPanel, type CurrentPhoto } from "./DetailsPanel";

const initialAlbum = useAlbumStore.getState();

beforeEach(() => {
  useAlbumStore.setState(initialAlbum, true);
  useSettingsStore.setState({ settings: structuredClone(defaultSettings), error: null });
});

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

interface Setup {
  fake: FakeApi;
  onClose: ReturnType<typeof vi.fn>;
  onNotFound: ReturnType<typeof vi.fn>;
  show(current: CurrentPhoto | null): void;
}

function setup(current: CurrentPhoto | null, overrides: Partial<FakeHandlers> = {}): Setup {
  const fake = createFakeApi(overrides);
  setApi(fake);
  const onClose = vi.fn();
  const onNotFound = vi.fn();
  const view = (c: CurrentPhoto | null) => <DetailsPanel current={c} onClose={onClose} onNotFound={onNotFound} />;
  const { rerender } = render(view(current));
  return { fake, onClose, onNotFound, show: (c) => rerender(view(c)) };
}

function calls(fake: FakeApi, command: string) {
  return fake.calls.filter((c) => c.command === command);
}

const panel = () => screen.getByRole("complementary", { name: "Details" });

function section(title: string) {
  return screen.getByRole("button", { name: title });
}

/** The value next to a fact row's label. */
function row(label: string): string | null {
  const dt = within(panel())
    .queryAllByText(label, { selector: "dt" })
    .at(0);
  return dt?.nextElementSibling?.textContent ?? null;
}

describe("DetailsPanel", () => {
  it("says no photo is selected when there is no current photo", async () => {
    const { fake } = setup(null);
    expect(panel()).toHaveTextContent("No photo selected");
    expect(calls(fake, "photo_detail")).toEqual([]);
    expect(await seriousFindings(document.body)).toEqual([]);
  });

  it("shows the facts, camera, histogram and place of the current photo", async () => {
    const { fake } = setup({ id: 42, file_name: "IMG_0042.jpg", rel_path: "2024/Trip/IMG_0042.jpg" });
    await screen.findByText("Taken");

    expect(calls(fake, "photo_detail")).toEqual([{ command: "photo_detail", args: { id: 42 } }]);
    expect(screen.getByRole("heading", { level: 2, name: "IMG_0042.jpg" })).toBeInTheDocument();
    expect(row("Taken")).toBe("12 Jun 2024, 14:03");
    expect(row("Size")).toBe("4032 × 3024 px · 3.1 MB");
    expect(row("Resolution")).toBe("12.2 MP · 4:3");
    expect(row("Shape")).toBe("Landscape");
    expect(row("Format")).toBe("JPEG");
    expect(row("Folder")).toBe("2024/Trip");
    expect(row("Lens")).toBe("Fixture 24-70mm F2.8");
    expect(row("Shutter")).toBe("1/250 s");
    expect(row("Country")).toBe("France");
    expect(row("Coordinates")).toBe("48.85837, 2.29448");

    await screen.findByRole("img", { name: /Histogram/ });
    expect(calls(fake, "photo_histogram")).toEqual([{ command: "photo_histogram", args: { id: 42 } }]);
    expect(screen.getByText("Shadows clipped 0.4 %")).not.toHaveClass("ow-histogram__high");
    expect(screen.getByText("Highlights clipped 2.3 %")).toHaveClass("ow-histogram__high");

    const titles = within(panel())
      .getAllByRole("button", { expanded: true })
      .map((b) => b.textContent);
    expect(titles).toEqual(["Photo", "Camera", "Histogram", "Place", "People", "Date and time", "Description and notes"]);
    expect(await seriousFindings(document.body)).toEqual([]);
  });

  it("shows the row's file name and an empty body while the first answer is on its way", async () => {
    const answers: ReturnType<typeof deferred<PhotoDetail>>[] = [];
    const { show } = setup(
      { id: 1, file_name: "a.jpg" },
      {
        photo_detail: () => {
          const d = deferred<PhotoDetail>();
          answers.push(d);
          return d.promise;
        },
      },
    );
    expect(screen.getByRole("heading", { level: 2, name: "a.jpg" })).toBeInTheDocument();
    expect(screen.queryByText("Taken")).not.toBeInTheDocument();
    await act(async () => answers[0]!.resolve(fakeDetail(1, { file_name: "a.jpg" })));
    expect(screen.getByText("Taken")).toBeInTheDocument();

    // Another photo: its name at once, and never the old photo's facts.
    show({ id: 2, file_name: "b.jpg" });
    expect(screen.getByRole("heading", { level: 2, name: "b.jpg" })).toBeInTheDocument();
    expect(screen.queryByText("Taken")).not.toBeInTheDocument();
  });

  it("throws away an answer for a photo that is no longer current", async () => {
    const answers = new Map<number, ReturnType<typeof deferred<PhotoDetail>>>();
    const { show } = setup(
      { id: 1 },
      {
        photo_detail: ({ id }) => {
          const d = deferred<PhotoDetail>();
          answers.set(id, d);
          return d.promise;
        },
      },
    );
    show({ id: 2 });
    await act(async () => answers.get(2)!.resolve(fakeDetail(2, { file_name: "two.jpg" })));
    await act(async () => answers.get(1)!.resolve(fakeDetail(1, { file_name: "one.jpg" })));
    expect(screen.getByRole("heading", { level: 2, name: "two.jpg" })).toBeInTheDocument();
    expect(screen.queryByText("one.jpg")).not.toBeInTheDocument();
  });

  it("reads the details again on album_changed for a newer revision only", async () => {
    let revision = 5;
    let description: string | null = null;
    const { fake } = setup({ id: 3 }, { photo_detail: ({ id }) => fakeDetail(id, { revision, description }) });
    await screen.findByText("Taken");
    expect(calls(fake, "photo_detail")).toHaveLength(1);

    await act(async () => fake.emit("album_changed", { revision: 5, reason: "info", rekeyed: [] }));
    expect(calls(fake, "photo_detail")).toHaveLength(1);

    revision = 6;
    description = "Sunset over the lake";
    await act(async () => fake.emit("album_changed", { revision: 6, reason: "info", rekeyed: [] }));
    await screen.findByDisplayValue("Sunset over the lake");
    expect(calls(fake, "photo_detail")).toHaveLength(2);
  });

  it("asks for the histogram again only when the photo or its thumbnail changes", async () => {
    let revision = 1;
    let thumb = "tk-a";
    const { fake } = setup({ id: 3 }, { photo_detail: ({ id }) => fakeDetail(id, { revision, thumb_key: thumb }) });
    await screen.findByRole("img", { name: /Histogram/ });
    expect(calls(fake, "photo_histogram")).toHaveLength(1);

    revision = 2;
    await act(async () => fake.emit("album_changed", { revision: 2, reason: "info", rekeyed: [] }));
    await waitFor(() => expect(calls(fake, "photo_detail")).toHaveLength(2));
    expect(calls(fake, "photo_histogram")).toHaveLength(1);

    revision = 3;
    thumb = "tk-b";
    await act(async () => fake.emit("album_changed", { revision: 3, reason: "scan", rekeyed: [3] }));
    await waitFor(() => expect(calls(fake, "photo_histogram")).toHaveLength(2));
  });

  it("re-runs the query when the photo is not found", async () => {
    const { onNotFound } = setup(
      { id: 9, file_name: "gone.jpg" },
      {
        photo_detail: () => {
          throw appError("photo_not_found", "That photo is no longer in this album.");
        },
      },
    );
    await waitFor(() => expect(onNotFound).toHaveBeenCalledTimes(1));
    expect(screen.queryByText("Taken")).not.toBeInTheDocument();
  });

  it("says so when the details cannot be read", async () => {
    setup(
      { id: 9, file_name: "x.jpg" },
      {
        photo_detail: () => {
          throw appError("internal", "Something went wrong.");
        },
      },
    );
    await screen.findByText("The details of this photo could not be read.");
  });

  it("clears the selection with its close button", async () => {
    const { onClose } = setup({ id: 1 });
    await screen.findByText("Taken");
    await userEvent.click(screen.getByRole("button", { name: "Close details" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("folds a section, saves it in the setting, and asks nothing for a folded histogram", async () => {
    const { fake, show } = setup({ id: 1 });
    await screen.findByRole("img", { name: /Histogram/ });
    expect(calls(fake, "photo_histogram")).toHaveLength(1);

    await userEvent.click(section("Histogram"));
    expect(section("Histogram")).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("img", { name: /Histogram/ })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(calls(fake, "app_settings_set").at(-1)?.args).toEqual({ patch: { sections: { histogram: false } } }),
    );

    show({ id: 2 });
    await screen.findByText("Taken");
    await act(async () => {});
    expect(calls(fake, "photo_histogram")).toHaveLength(1);
    expect(section("Histogram")).toHaveAttribute("aria-expanded", "false");
  });

  it("starts with the sections the setting folds", async () => {
    useSettingsStore.setState({ settings: { ...structuredClone(defaultSettings), sections: { camera: false } } });
    setup({ id: 1 });
    await screen.findByText("Taken");
    expect(section("Camera")).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Lens")).not.toBeInTheDocument();
    expect(section("Photo")).toHaveAttribute("aria-expanded", "true");
  });

  it("shows Length and no Histogram section for a video", async () => {
    const { fake } = setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, { kind: "video", format: "mp4", width: 1920, height: 1080, duration_ms: 83_500 }),
      },
    );
    await screen.findByText("Taken");
    expect(row("Length")).toBe("1:23");
    expect(screen.queryByRole("button", { name: "Histogram" })).not.toBeInTheDocument();
    expect(calls(fake, "photo_histogram")).toEqual([]);
  });

  it("marks a typed date as changed", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, {
            date: { effective: "1999-12-31T23:59:00", camera: "2024-06-12T14:03:00", typed: "1999-12-31T23:59:00" },
          }),
      },
    );
    await screen.findByText("Taken");
    expect(row("Taken")).toBe("31 Dec 1999, 23:59 changed");
  });

  it("says when there are no camera details", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, {
            camera: {
              camera: null,
              lens: null,
              focal_mm: null,
              focal35_mm: null,
              aperture: null,
              shutter_s: null,
              iso: null,
              bias_ev: null,
              flash: null,
            },
          }),
      },
    );
    await screen.findByText("No camera details in this file");
  });

  it("leaves out the Country and Coordinates rows for a photo with no place", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, { place: { lat: null, lon: null, gps: null, typed: false, name: null, country: null } }),
      },
    );
    await screen.findByText("Taken");
    expect(row("Country")).toBeNull();
    expect(row("Coordinates")).toBeNull();
    expect(section("Place")).toBeInTheDocument();
  });

  it("leaves out Country but keeps Coordinates at sea", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, { place: { lat: 30, lon: -40, gps: { lat: 30, lon: -40 }, typed: false, name: null, country: null } }),
      },
    );
    await screen.findByText("Taken");
    expect(row("Country")).toBeNull();
    expect(row("Coordinates")).toBe("30.00000, -40.00000");
  });

  it("says the histogram could not be drawn when it fails", async () => {
    setup(
      { id: 5 },
      {
        photo_histogram: () => {
          throw appError("internal", "No thumbnail.");
        },
      },
    );
    await screen.findByText("The histogram could not be drawn.");
  });

  it("notes that a view-only album cannot be changed", async () => {
    useAlbumStore.setState({ status: fakeStatus("D:/Photos",{ view_only: "read_only" }) });
    setup({ id: 1 });
    await screen.findByText("This album is view-only, so nothing here can be changed.");
  });

  it("shows people, description and notes in their fields", async () => {
    setup(
      { id: 1 },
      { photo_detail: ({ id }) => fakeDetail(id, { people: ["Ann", "Bob"], description: "Tower", notes: null }) },
    );
    await screen.findByText("Ann");
    expect(screen.getByText("Bob")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue("Tower");
  });
});
