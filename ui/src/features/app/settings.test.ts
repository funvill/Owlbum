import { beforeEach, describe, expect, it } from "vitest";
import { setApi } from "../../api";
import type { Settings } from "../../api";
import { appError, createFakeApi } from "../../api/fake";
import { defaultSettings } from "../../api/fake/app";
import { useSettingsStore } from "./settings";

describe("the settings store", () => {
  beforeEach(() => {
    useSettingsStore.setState({ settings: null, error: null });
  });

  it("loads every setting from Rust", async () => {
    const fake = createFakeApi();
    setApi(fake);

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().settings).toEqual(defaultSettings);
    expect(fake.calls).toEqual([{ command: "app_settings_get", args: {} }]);
  });

  it("saves a change through app_settings_set and keeps Rust's answer", async () => {
    const fake = createFakeApi();
    setApi(fake);
    await useSettingsStore.getState().load();

    await useSettingsStore.getState().update({ thumb_size: 240, "panel.grid_width": 500 });

    expect(fake.calls[1]).toEqual({
      command: "app_settings_set",
      args: { patch: { thumb_size: 240, "panel.grid_width": 500 } },
    });
    const s = useSettingsStore.getState().settings;
    expect(s?.thumb_size).toBe(240);
    expect(s?.["panel.grid_width"]).toBe(500);
    expect(s?.view).toBe("grid");
  });

  it("keeps the old settings and the error when a value is refused", async () => {
    const fake = createFakeApi();
    setApi(fake);
    await useSettingsStore.getState().load();

    await useSettingsStore.getState().update({ "cache.limit_gb": 5000 });

    expect(useSettingsStore.getState().settings).toEqual(defaultSettings);
    expect(useSettingsStore.getState().error?.code).toBe("setting_out_of_range");
  });

  it("clears an earlier error after a change is saved", async () => {
    let fail = true;
    const saved: Settings = { ...defaultSettings, stack_similar: false };
    setApi(
      createFakeApi({
        app_settings_set: () => {
          if (fail) throw appError("internal", "Something went wrong while saving Owlbum's settings.");
          return saved;
        },
      }),
    );

    await useSettingsStore.getState().update({ stack_similar: false });
    expect(useSettingsStore.getState().error?.code).toBe("internal");

    fail = false;
    await useSettingsStore.getState().update({ stack_similar: false });
    expect(useSettingsStore.getState().error).toBeNull();
    expect(useSettingsStore.getState().settings).toEqual(saved);
  });

  it("reports a failed load", async () => {
    setApi(
      createFakeApi({
        app_settings_get: () => {
          throw appError("internal", "Something went wrong while reading Owlbum's settings.");
        },
      }),
    );

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().settings).toBeNull();
    expect(useSettingsStore.getState().error?.message).toContain("reading Owlbum's settings");
  });
});

describe("the fake app data commands", () => {
  it("answers the defaults Rust documents", async () => {
    const fake = createFakeApi();
    const s = await fake.invoke("app_settings_get");
    expect(s.thumb_size).toBe(160);
    expect(s["panel.grid_width"]).toBe(420);
    expect(s["panel.details_width"]).toBe(340);
    expect(s["cache.limit_gb"]).toBe(20);
    expect(s.window).toEqual({ x: null, y: null, width: 1440, height: 900, maximised: false });
    expect(s.reopen_last).toBe(true);
  });

  it("refuses a thumbnail size outside 96 to 320", async () => {
    const fake = createFakeApi();
    await expect(fake.invoke("app_settings_set", { patch: { thumb_size: 95 } })).rejects.toMatchObject({
      code: "setting_out_of_range",
      details: { key: "thumb_size" },
    });
    await expect(fake.invoke("app_settings_set", { patch: { thumb_size: 96 } })).resolves.toMatchObject({
      thumb_size: 96,
    });
  });

  it("has an empty recent list by default, and a test can fill it", async () => {
    const fake = createFakeApi();
    await expect(fake.invoke("app_recent_list")).resolves.toEqual([]);

    const list = [
      { root: "/photos/b", name: "b", opened_at: 2, exists: false },
      { root: "/photos/a", name: "a", opened_at: 1, exists: true },
    ];
    fake.handle("app_recent_list", () => list);
    fake.handle("app_recent_remove", ({ root }) => list.filter((r) => r.root !== root));
    await expect(fake.invoke("app_recent_list")).resolves.toEqual(list);
    await expect(fake.invoke("app_recent_remove", { root: "/photos/b" })).resolves.toEqual([list[1]]);
  });
});
