import { beforeEach, describe, expect, it } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { ModelInfo } from "../../api/bindings/ModelInfo";
import type { ModelsState } from "../../api/bindings/ModelsState";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeModelsState } from "../../api/fake/tagging";
import { seriousFindings } from "../../test/axe";
import { taggingSettingsText as t } from "../../text/app";
import { TaggingSettings, modelSize, modelSpeed } from "./TaggingSettings";

/** The fake's models state, with some models changed. */
function stateWith(changes: Record<string, Partial<ModelInfo>>, rest: Partial<ModelsState> = {}): ModelsState {
  const s = fakeModelsState();
  s.models = s.models.map((m) => ({ ...m, ...changes[m.id] }));
  return { ...s, ...rest };
}

function show(state: ModelsState = fakeModelsState()): FakeApi {
  let current = state;
  const fake = createFakeApi({ tagging_models: () => structuredClone(current) });
  setApi(fake);
  render(<TaggingSettings />);
  // Later reads see what a test sets with `next`.
  (fake as FakeApi & { next: (s: ModelsState) => void }).next = (s) => {
    current = s;
  };
  return fake;
}

const row = (label: string) => {
  const radio = screen.getByRole("radio", { name: new RegExp(`^${label} \\(`) });
  const r = radio.closest(".ow-tagging-settings__row");
  if (!(r instanceof HTMLElement)) throw new Error("no row");
  return within(r);
};

const calls = (fake: FakeApi, command: string) => fake.calls.filter((c) => c.command === command).map((c) => c.args);

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("the size and speed text", () => {
  it("writes megabytes, gigabytes and the speed as the spec says", () => {
    expect(modelSize(155_851_446)).toBe("156 MB");
    expect(modelSize(277_470_179)).toBe("277 MB");
    expect(modelSize(1_300_000_000)).toBe("1.3 GB");
    expect(modelSize(4_210_000)).toBe("4.2 MB");
    expect(modelSpeed(0.1)).toBe("less than a second a photo");
    expect(modelSpeed(6)).toBe("about 6 s a photo");
    expect(modelSpeed(19.6)).toBe("about 20 s a photo");
  });
});

describe("Settings → Photo tagging", () => {
  it("shows loading, then both parts with Off and their models, smaller first", async () => {
    show();
    expect(screen.getByText(t.loading)).toBeInTheDocument();
    const describe = await screen.findByRole("radiogroup", { name: t.parts.describe.title });
    const radios = within(describe).getAllByRole("radio");
    expect(radios.map((r) => r.closest("label")?.textContent)).toEqual([
      "Off",
      "Florence-2 base (smaller)277 MB · about 6 s a photo · MIT",
      "Florence-2 large (larger)832 MB · about 20 s a photo · MIT",
    ]);
    expect(radios[0]).toBeChecked();
    expect(radios[1]).toBeDisabled();
    const keywords = screen.getByRole("radiogroup", { name: t.parts.keywords.title });
    expect(within(keywords).getByText("156 MB · less than a second a photo · MIT")).toBeInTheDocument();
    expect(await seriousFindings(document.body)).toEqual([]);
  });

  it("shows a failure to load in place of the section", async () => {
    const fake = createFakeApi({
      tagging_models: () => {
        throw appError("internal", "Couldn't read the settings.");
      },
    });
    setApi(fake);
    render(<TaggingSettings />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't read the settings.");
  });

  it("puts the right thing at the right of each model row", async () => {
    show(
      stateWith(
        {
          "florence2-base-q8": { state: "absent" },
          "florence2-large-q8": { state: "partial" },
          "clip-base-q8": { state: "ok" },
          "clip-large-q8": { state: "damaged" },
        },
        { settings: { describe: null, keywords: "clip-base-q8" } },
      ),
    );
    await screen.findByRole("radiogroup", { name: t.parts.describe.title });
    expect(row("Florence-2 base").getByRole("button", { name: t.download })).toBeInTheDocument();
    expect(row("Florence-2 large").getByRole("button", { name: t.continueDownload })).toBeInTheDocument();
    expect(row("CLIP base").getByRole("button", { name: t.remove })).toBeInTheDocument();
    expect(row("CLIP base").queryByRole("button", { name: t.downloadAgain })).toBeNull();
    expect(row("CLIP large").getByRole("button", { name: t.downloadAgain })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^CLIP base/ })).toBeChecked();
  });

  it("shows Checking… for a model being checked", async () => {
    show(stateWith({ "clip-base-q8": { state: "checking" }, "clip-large-q8": { state: "unverified" } }));
    await screen.findByRole("radiogroup", { name: t.parts.keywords.title });
    expect(row("CLIP base").getByText(t.checking)).toBeInTheDocument();
    expect(row("CLIP large").getByText(t.checking)).toBeInTheDocument();
  });

  it("offers Download again and Remove for a chosen model that is not working", async () => {
    show(
      stateWith(
        { "clip-base-q8": { state: "ok" } },
        { settings: { describe: null, keywords: "clip-base-q8" }, not_working: ["keywords"] },
      ),
    );
    await screen.findByRole("radiogroup", { name: t.parts.keywords.title });
    expect(row("CLIP base").getByRole("button", { name: t.downloadAgain })).toBeInTheDocument();
    expect(row("CLIP base").getByRole("button", { name: t.remove })).toBeInTheDocument();
  });

  it("asks before downloading, with Download focused, and then downloads", async () => {
    const user = userEvent.setup();
    const fake = show();
    await screen.findByRole("radiogroup", { name: t.parts.describe.title });
    await user.click(row("Florence-2 base").getByRole("button", { name: t.download }));
    const strip = screen.getByRole("group", { name: t.confirm("Florence-2 base", "277 MB") });
    const go = within(strip).getByRole("button", { name: t.download });
    await waitFor(() => expect(go).toHaveFocus());
    await user.click(within(strip).getByRole("button", { name: t.cancel }));
    expect(calls(fake, "tagging_download")).toEqual([]);
    await user.click(row("Florence-2 base").getByRole("button", { name: t.download }));
    await user.click(within(screen.getByRole("group", { name: /^Download Florence-2 base/ })).getByRole("button", { name: t.download }));
    expect(calls(fake, "tagging_download")).toEqual([{ model: "florence2-base-q8" }]);
  });

  it("follows download_progress, disables other downloads, and reads the models again when it ends", async () => {
    const user = userEvent.setup();
    const fake = show();
    await screen.findByRole("radiogroup", { name: t.parts.describe.title });
    act(() => fake.emit("download_progress", { model: "clip-base-q8", received: 41, total: 100, error: null, finished: false }));
    const bar = row("CLIP base").getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "41");
    expect(row("CLIP base").getByText("41%")).toBeInTheDocument();
    const other = row("Florence-2 base").getByRole("button", { name: t.download });
    expect(other).toHaveAttribute("aria-disabled", "true");
    await user.click(row("CLIP base").getByRole("button", { name: t.cancel }));
    expect(calls(fake, "tagging_download_cancel")).toHaveLength(1);

    const before = calls(fake, "tagging_models").length;
    (fake as FakeApi & { next: (s: ModelsState) => void }).next(stateWith({ "clip-base-q8": { state: "ok" } }));
    act(() => fake.emit("download_progress", { model: "clip-base-q8", received: 100, total: 100, error: null, finished: true }));
    await waitFor(() => expect(calls(fake, "tagging_models").length).toBe(before + 1));
    expect(await row("CLIP base").findByRole("button", { name: t.remove })).toBeInTheDocument();
  });

  it("shows the last download error", async () => {
    show(fakeModelsState());
    await screen.findByRole("radiogroup", { name: t.parts.describe.title });
    document.body.innerHTML = "";
    show({ ...fakeModelsState(), error: "Couldn't download CLIP large: The download stalled. Try again." });
    expect(await screen.findByRole("alert")).toHaveTextContent("The download stalled");
  });

  it("saves a choice at once, and a refused choice puts the radio back", async () => {
    const user = userEvent.setup();
    const fake = show(stateWith({ "clip-base-q8": { state: "ok" } }));
    await screen.findByRole("radiogroup", { name: t.parts.keywords.title });
    await user.click(screen.getByRole("radio", { name: /^CLIP base/ }));
    expect(calls(fake, "tagging_set")).toEqual([{ describe: null, keywords: "clip-base-q8" }]);
    await waitFor(() => expect(screen.getByRole("radio", { name: /^CLIP base/ })).toBeChecked());

    fake.handle("tagging_set", () => {
      throw appError("model_not_ready", "Choose a downloaded model of the right kind, or Off.");
    });
    const off = within(screen.getByRole("radiogroup", { name: t.parts.keywords.title })).getByRole("radio", { name: t.off });
    await user.click(off);
    expect(await screen.findByRole("alert")).toHaveTextContent("Choose a downloaded model");
    expect(screen.getByRole("radio", { name: /^CLIP base/ })).toBeChecked();
  });

  it("removes a model", async () => {
    const user = userEvent.setup();
    const fake = show(stateWith({ "clip-base-q8": { state: "ok" } }));
    await screen.findByRole("radiogroup", { name: t.parts.keywords.title });
    await user.click(row("CLIP base").getByRole("button", { name: t.remove }));
    expect(calls(fake, "tagging_remove")).toEqual([{ model: "clip-base-q8" }]);
  });
});
