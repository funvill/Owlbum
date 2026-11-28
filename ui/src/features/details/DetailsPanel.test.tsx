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
import { resetGrid, useGridStore } from "../grid";
import { resetMapFocus, useMapFocus } from "../map";
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
  onNotFound: ReturnType<typeof vi.fn>;
  show(current: CurrentPhoto | null): void;
}

function setup(current: CurrentPhoto | null, overrides: Partial<FakeHandlers> = {}): Setup {
  const fake = createFakeApi(overrides);
  setApi(fake);
  const onNotFound = vi.fn();
  const view = (c: CurrentPhoto | null) => <DetailsPanel current={c} onNotFound={onNotFound} />;
  const { rerender } = render(view(current));
  return { fake, onNotFound, show: (c) => rerender(view(c)) };
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
    const { fake } = setup({ id: 42 });
    await screen.findByText("Taken");

    expect(calls(fake, "photo_detail")).toEqual([{ command: "photo_detail", args: { id: 42 } }]);
    // No file name heading over the panel; the Path row holds the file.
    expect(screen.queryByRole("heading", { level: 2 })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close details" })).not.toBeInTheDocument();
    expect(row("Taken")).toBe("12 Jun 2024, 14:03");
    expect(row("Size")).toBe("4032 × 3024 px · 3.1 MB");
    expect(row("Resolution")).toBe("12.2 MP · 4:3");
    expect(row("Shape")).toBe("Landscape");
    expect(row("Format")).toBe("JPEG");
    expect(row("Folder")).toBe("2024/Trip");
    expect(row("Path")).toBe("D:\\Photos\\2024\\Trip\\IMG_0042.jpg");
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
    expect(titles).toEqual([
      "Photo",
      "Camera",
      "Histogram",
      "Place",
      "People",
      "Date and time",
      "Notes",
      "EXIF",
    ]);
    expect(await seriousFindings(document.body)).toEqual([]);
  });

  it("shows an empty body, and no Copy, while the first answer is on its way", async () => {
    const answers: ReturnType<typeof deferred<PhotoDetail>>[] = [];
    const { show } = setup(
      { id: 1 },
      {
        photo_detail: () => {
          const d = deferred<PhotoDetail>();
          answers.push(d);
          return d.promise;
        },
      },
    );
    expect(screen.queryByText("Taken")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Copy" })).not.toBeInTheDocument();
    await act(async () => answers[0]!.resolve(fakeDetail(1, { file_name: "a.jpg" })));
    expect(screen.getByText("Taken")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy" })).toBeInTheDocument();

    // Another photo: never the old photo's facts.
    show({ id: 2 });
    expect(screen.queryByText("Taken")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Copy" })).not.toBeInTheDocument();
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
    await act(async () => answers.get(2)!.resolve(fakeDetail(2, { abs_path: "D:\\two.jpg" })));
    await act(async () => answers.get(1)!.resolve(fakeDetail(1, { abs_path: "D:\\one.jpg" })));
    expect(row("Path")).toBe("D:\\two.jpg");
    expect(screen.queryByText("D:\\one.jpg")).not.toBeInTheDocument();
  });

  it("reads the details again on album_changed for a newer revision only", async () => {
    let revision = 5;
    let notes: string | null = null;
    const { fake } = setup({ id: 3 }, { photo_detail: ({ id }) => fakeDetail(id, { revision, notes }) });
    await screen.findByText("Taken");
    expect(calls(fake, "photo_detail")).toHaveLength(1);

    await act(async () => fake.emit("album_changed", { revision: 5, reason: "info", rekeyed: [] }));
    expect(calls(fake, "photo_detail")).toHaveLength(1);

    revision = 6;
    notes = "Sunset over the lake";
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
      { id: 9 },
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
      { id: 9 },
      {
        photo_detail: () => {
          throw appError("internal", "Something went wrong.");
        },
      },
    );
    await screen.findByText("The details of this photo could not be read.");
  });

  it("copies the path with the button beside it", async () => {
    const { fake } = setup({ id: 7 });
    await screen.findByText("Taken");
    await userEvent.click(screen.getByRole("button", { name: "Copy path" }));
    await waitFor(() => expect(calls(fake, "file_copy_paths")).toEqual([{ command: "file_copy_paths", args: { photo_ids: [7] } }]));
  });

  it("lists the file's EXIF tags in their own section", async () => {
    const { fake } = setup({ id: 7 });
    await screen.findByText("ExposureTime");
    expect(calls(fake, "photo_exif")).toEqual([{ command: "photo_exif", args: { id: 7 } }]);
    expect(row("Make")).toBe("Owlbum");
    expect(row("ExposureTime")).toBe("1/250");
  });

  it("says when the file has no EXIF data", async () => {
    setup({ id: 7 }, { photo_exif: () => [] });
    expect(await screen.findByText("No EXIF data in this file")).toBeInTheDocument();
  });

  it("copies everything the panel shows as plain text", async () => {
    let copied = "";
    const { fake } = setup(
      { id: 42 },
      {
        file_copy_text: ({ content }) => {
          copied = content;
          return null;
        },
      },
    );
    await screen.findByText("Taken");
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => expect(calls(fake, "file_copy_text")).toHaveLength(1));
    expect(copied).toContain("Photo score: 105");
    expect(copied).toContain("Photo\nTaken: 12 Jun 2024, 14:03");
    expect(copied).toContain("Path: D:\\Photos\\2024\\Trip\\IMG_0042.jpg");
    expect(copied).toContain("Camera\nCamera: ");
    expect(copied).toContain("Country: France");
    expect(copied).toContain("EXIF\nMake: Owlbum\nModel: Fixture One");
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

  it("puts Copy in the Photo section, below the score, not in a head over the panel", async () => {
    setup({ id: 42 });
    await screen.findByText("Taken");
    const copy = screen.getByRole("button", { name: "Copy" });
    const photo = section("Photo").closest("section")!;
    expect(photo).toContainElement(copy);
    expect(panel().querySelector("header")).toBeNull();
    // The score card comes first.
    const score = await screen.findByText(/Photo score/);
    expect(score.compareDocumentPosition(copy) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("gives the panel's buttons an icon as well as their words", async () => {
    setup({ id: 42 });
    await screen.findByText("Taken");
    for (const name of ["Copy", "Copy path", "Pick on map", "Show on map"]) {
      expect(screen.getByRole("button", { name }).querySelector("svg")).not.toBeNull();
    }
  });

  it("shows Show on map beside Pick on map, which switches to the map and centres this photo", async () => {
    resetGrid();
    resetMapFocus();
    const { fake } = setup({ id: 42 });
    await screen.findByText("Taken");
    const pick = screen.getByRole("button", { name: "Pick on map" });
    const show = screen.getByRole("button", { name: "Show on map" });
    expect(pick.parentElement).toBe(show.closest(".ow-details__actions"));
    await userEvent.click(show);
    expect(useGridStore.getState().query.view).toBe("map");
    expect(useMapFocus.getState().id).toBe(42);
    await waitFor(() =>
      expect(calls(fake, "app_settings_set").some((c) => JSON.stringify(c.args).includes('"view":"map"'))).toBe(true),
    );
    resetGrid();
    resetMapFocus();
  });

  it("leaves out Show on map for a photo with no place", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, { place: { lat: null, lon: null, gps: null, typed: false, name: null, country: null } }),
      },
    );
    await screen.findByText("Taken");
    expect(screen.getByRole("button", { name: "Pick on map" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Show on map" })).not.toBeInTheDocument();
  });

  it("shows None for Country and Coordinates, and no Place name box to type in, for a photo with no place", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, { place: { lat: null, lon: null, gps: null, typed: false, name: null, country: null } }),
      },
    );
    await screen.findByText("Taken");
    expect(row("Country")).toBe("None");
    expect(row("Coordinates")).toBe("None");
    expect(section("Place")).toBeInTheDocument();
    // Only Place name is typed; the place itself comes from Pick on map.
    expect(screen.getByRole("textbox", { name: "Place name" })).toBeDisabled();
    expect(screen.queryByRole("textbox", { name: "Latitude" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Location" })).toBeNull();
  });

  it("shows no Country but the Coordinates at sea", async () => {
    setup(
      { id: 5 },
      {
        photo_detail: ({ id }) =>
          fakeDetail(id, { place: { lat: 30, lon: -40, gps: { lat: 30, lon: -40 }, typed: false, name: null, country: null } }),
      },
    );
    await screen.findByText("Taken");
    expect(row("Country")).toBe("None");
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

  it("shows people and notes in their fields, with no Description box", async () => {
    setup(
      { id: 1 },
      { photo_detail: ({ id }) => fakeDetail(id, { people: ["Ann", "Bob"], notes: "Tower" }) },
    );
    await screen.findByText("Ann");
    expect(screen.getByText("Bob")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Notes" })).toHaveValue("Tower");
    expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull();
  });
});
