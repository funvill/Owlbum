import { beforeEach, describe, expect, it } from "vitest";
import { setApi } from "../../api";
import { appError, createFakeApi } from "../../api/fake";
import type { FakeApi } from "../../api/fake";
import { albumText } from "../../text/album";
import { idleScan, scanLabel, useScanStore } from "./store";

let fake: FakeApi;

beforeEach(async () => {
  useScanStore.getState().stop();
  useScanStore.setState({ progress: idleScan, error: null });
  fake = createFakeApi();
  setApi(fake);
  await useScanStore.getState().start();
});

describe("scan store", () => {
  it("follows scan_progress events", () => {
    fake.emit("scan_progress", {
      phase: "reading",
      found: 10,
      done: 4,
      failed: 1,
      finished: false,
      cancelled: false,
    });
    expect(useScanStore.getState().progress).toMatchObject({ phase: "reading", found: 10, done: 4, failed: 1 });

    fake.emit("scan_progress", { phase: "idle", found: 10, done: 10, failed: 1, finished: true, cancelled: false });
    expect(useScanStore.getState().progress.finished).toBe(true);
  });

  it("stops following when stopped", () => {
    useScanStore.getState().stop();
    fake.emit("scan_progress", { phase: "walking", found: 3, done: 0, failed: 0, finished: false, cancelled: false });
    expect(useScanStore.getState().progress).toEqual(idleScan);
  });

  it("asks Rust to rescan and to cancel", async () => {
    await useScanStore.getState().rescan();
    await useScanStore.getState().cancel();
    expect(fake.calls).toEqual([
      { command: "album_rescan", args: {} },
      { command: "album_scan_cancel", args: {} },
    ]);
  });

  it("keeps the error when rescan fails", async () => {
    fake.handle("album_rescan", () => {
      throw appError("no_album", "No album is open.");
    });
    await useScanStore.getState().rescan();
    expect(useScanStore.getState().error).toEqual({ code: "no_album", message: "No album is open." });
  });
});

describe("scanLabel", () => {
  it("names what the scan is doing", () => {
    expect(scanLabel({ ...idleScan, phase: "walking", found: 12 })).toBe(albumText.scanWalking);
    expect(scanLabel({ ...idleScan, phase: "reading", found: 12, done: 5 })).toBe("Reading photos 5 / 12");
    expect(scanLabel(idleScan)).toBeNull();
  });
});
