import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import type { Settings } from "../../api";
import { createFakeApi, type FakeApi } from "../../api/fake";
import { defaultSettings } from "../../api/fake/app";
import { Toasts, useToasts } from "../../controls";
import { seriousFindings } from "../../test/axe";
import { settingsText as t } from "../../text/app";
import { formatSize } from "../details/facts";
import { SettingsDialog, lastChecked, parseLimit } from "./SettingsDialog";
import { useSettingsStore } from "./settings";
import { followTheme, resolveTheme } from "./theme";

const BYTES = 12_400_000_000;
const SIZE = formatSize(BYTES);

/** A fake whose settings persist between calls. */
function show(start: Partial<Settings> = {}): {
  fake: FakeApi;
  onClose: ReturnType<typeof vi.fn>;
} {
  let settings: Settings = { ...structuredClone(defaultSettings), ...start };
  const fake = createFakeApi({
    app_settings_get: () => structuredClone(settings),
    app_settings_set: ({ patch }) => {
      settings = { ...settings, ...patch } as Settings;
      return structuredClone(settings);
    },
    app_cache_usage: () => ({ bytes: BYTES, limit_bytes: 20_000_000_000 }),
    app_cache_clear: () => ({ bytes_freed: BYTES }),
    app_version: () => ({ version: "0.1.0" }),
  });
  setApi(fake);
  const onClose = vi.fn();
  render(
    <>
      <SettingsDialog open onClose={onClose} />
      <Toasts />
    </>,
  );
  return { fake, onClose };
}

const setCalls = (fake: FakeApi) => fake.calls.filter((c) => c.command === "app_settings_set").map((c) => c.args);

beforeEach(() => {
  useSettingsStore.setState({ settings: null, error: null });
  useToasts.setState({ toasts: [] });
});

describe("the Settings dialog", () => {
  it("shows every section and passes the accessibility check", async () => {
    show();
    const dialog = await screen.findByRole("dialog", { name: t.title });
    for (const name of [t.general, t.view, t.maps, t.tagging, t.updates]) {
      expect(within(dialog).getByRole("region", { name })).toBeInTheDocument();
    }
    expect(await within(dialog).findByText(t.cacheUses(SIZE))).toBeInTheDocument();
    expect(within(dialog).getByText(t.versionLine("0.1.0", t.never))).toBeInTheDocument();
    expect(await seriousFindings(dialog)).toEqual([]);
  });

  it("saves each change at once", async () => {
    const user = userEvent.setup();
    const { fake } = show();
    await user.click(await screen.findByRole("checkbox", { name: t.reopenLast }));
    await user.click(screen.getByRole("checkbox", { name: t.mapsOnline }));
    await user.click(screen.getByRole("checkbox", { name: t.updatesAuto }));
    await waitFor(() =>
      expect(setCalls(fake)).toEqual([
        { patch: { reopen_last: false } },
        { patch: { maps_online: true } },
        { patch: { "updates.auto": false } },
      ]),
    );
  });

  it("switches edit_target with its radio buttons and asks for the counts again", async () => {
    const user = userEvent.setup();
    const { fake } = show();
    const group = await screen.findByRole("radiogroup", { name: t.editTarget });
    expect(within(group).getByRole("radio", { name: t.editAlbum })).toBeChecked();
    await user.click(within(group).getByRole("radio", { name: t.editFile }));
    await waitFor(() => expect(within(group).getByRole("radio", { name: t.editFile })).toBeChecked());
    expect(setCalls(fake)).toEqual([{ patch: { edit_target: "file" } }]);
    await waitFor(() => expect(fake.calls.some((c) => c.command === "edit_status")).toBe(true));
  });

  it("keeps a cache limit in range and puts a bad one back", async () => {
    const user = userEvent.setup();
    const { fake } = show();
    const field = await screen.findByRole("spinbutton", { name: t.cacheLimit });
    await user.clear(field);
    await user.type(field, "5000{Enter}");
    expect(field).toHaveValue(20);
    expect(screen.getByText(t.cacheRange)).toBeInTheDocument();
    await user.clear(field);
    await user.type(field, "50");
    await user.tab();
    await waitFor(() => expect(setCalls(fake)).toEqual([{ patch: { "cache.limit_gb": 50 } }]));
  });

  it("clears the cache only after the confirm strip", async () => {
    const user = userEvent.setup();
    const { fake } = show();
    await screen.findByText(t.cacheUses(SIZE));
    await user.click(screen.getByRole("button", { name: t.clearCache }));
    const strip = screen.getByRole("group", { name: t.clearCache });
    expect(within(strip).getByText(t.clearConfirm(SIZE))).toBeInTheDocument();
    await user.click(within(strip).getByRole("button", { name: t.clearCancel }));
    expect(fake.calls.some((c) => c.command === "app_cache_clear")).toBe(false);
    await user.click(screen.getByRole("button", { name: t.clearCache }));
    await user.click(screen.getByRole("button", { name: t.clear }));
    await waitFor(() => expect(fake.calls.some((c) => c.command === "app_cache_clear")).toBe(true));
    expect(await screen.findByText(t.cleared(SIZE))).toBeInTheDocument();
  });

  it("makes Owlbum the program that opens pictures, and stops", async () => {
    const user = userEvent.setup();
    const { fake } = show();
    const box = await screen.findByRole("checkbox", { name: t.openPictures });
    expect(box).not.toBeChecked();
    await user.click(box);
    await waitFor(() => expect(box).toBeChecked());
    expect(await screen.findByText(t.openPicturesConfirm)).toBeInTheDocument();
    await user.click(box);
    await waitFor(() => expect(box).not.toBeChecked());
    const asked = fake.calls.filter((c) => c.command === "app_associate").map((c) => c.args);
    expect(asked).toEqual([{ on: true }, { on: false }]);
  });

  it("leaves Open pictures out where it is not offered", async () => {
    setApi(createFakeApi({ app_associate_status: () => ({ available: false, on: false }) }));
    render(<SettingsDialog open onClose={() => {}} />);
    await screen.findByRole("checkbox", { name: t.reopenLast });
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: t.openPictures })).toBeNull());
  });

  it("moves the thumbnails to the bottom", async () => {
    const user = userEvent.setup();
    const { fake } = show();
    await user.click(await screen.findByRole("radio", { name: t.gridBottom }));
    await waitFor(() =>
      expect(setCalls(fake)).toContainEqual({ patch: { "panel.grid_place": "bottom", "panel.grid_shown": true } }),
    );
  });

  it("closes from its Close button", async () => {
    const user = userEvent.setup();
    const { onClose } = show();
    await screen.findByRole("checkbox", { name: t.reopenLast });
    const buttons = screen.getAllByRole("button", { name: t.close });
    await user.click(buttons[buttons.length - 1]!);
    expect(onClose).toHaveBeenCalled();
  });
});

describe("helpers", () => {
  it("accepts a whole number of GB from 1 to 1000", () => {
    expect(parseLimit("1")).toBe(1);
    expect(parseLimit(" 1000 ")).toBe(1000);
    expect(parseLimit("0")).toBeNull();
    expect(parseLimit("1001")).toBeNull();
    expect(parseLimit("2.5")).toBeNull();
    expect(parseLimit("")).toBeNull();
  });

  it("says when the last update check was", () => {
    const now = new Date(2026, 10, 27, 15, 0);
    expect(lastChecked(null, now)).toBe(t.never);
    expect(lastChecked(new Date(2026, 10, 27, 9, 14).getTime(), now)).toBe(t.today("09:14"));
    expect(lastChecked(new Date(2026, 10, 20, 8, 5).getTime(), now)).toBe(t.onDay("20 Nov 2026", "08:05"));
  });
});

describe("theme and language (issues 75 and 76)", () => {
  it("saves the theme and redraws the page in it", async () => {
    const stop = followTheme();
    const { fake } = show();
    const system = await screen.findByRole("radio", { name: t.themeSystem });
    expect(system).toBeChecked();
    // jsdom has no system preference: the system counts as dark.
    expect(document.documentElement.dataset.theme).toBe("dark");
    await userEvent.click(screen.getByRole("radio", { name: t.themeLight }));
    await waitFor(() => expect(setCalls(fake)).toEqual([{ patch: { theme: "light" } }]));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    await userEvent.click(screen.getByRole("radio", { name: t.themeDark }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    stop();
  });

  it("resolves System by the system's choice", () => {
    expect(resolveTheme("system", true)).toBe("light");
    expect(resolveTheme("system", false)).toBe("dark");
    expect(resolveTheme("dark", true)).toBe("dark");
    expect(resolveTheme("light", false)).toBe("light");
  });

  it("saves a language, or null for the system's, and says when it is shown", async () => {
    const { fake } = show();
    const select = await screen.findByRole("combobox", { name: t.language });
    expect(select).toHaveValue("");
    await userEvent.selectOptions(select, "en");
    await waitFor(() => expect(setCalls(fake)).toEqual([{ patch: { language: "en" } }]));
    expect(await screen.findByText(t.languageNote)).toBeInTheDocument();
    await userEvent.selectOptions(select, "");
    await waitFor(() => expect(setCalls(fake)).toContainEqual({ patch: { language: null } }));
  });
});
