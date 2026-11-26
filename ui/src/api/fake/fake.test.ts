import { describe, expect, it } from "vitest";
import { appError, createFakeApi } from ".";
import { fakeFormats } from "./app";

describe("the fake backend", () => {
  it("answers with the default handlers", async () => {
    const fake = createFakeApi();
    await expect(fake.invoke("app_version")).resolves.toEqual({ version: "0.0.0-fake" });
  });

  it("answers app_formats with registry rows", async () => {
    const fake = createFakeApi();
    const formats = await fake.invoke("app_formats");
    expect(formats).toEqual(fakeFormats);
    expect(formats.map((f) => f.kind)).toEqual(["image", "image", "raw", "video"]);
    // Each call answers a fresh copy, so a test cannot change another's rows.
    formats[0]?.extensions.push(".x");
    const again = await fake.invoke("app_formats");
    expect(again[0]?.extensions).not.toContain(".x");
  });

  it("lets a test replace a handler", async () => {
    const fake = createFakeApi();
    fake.handle("app_version", () => ({ version: "9.9.9" }));
    await expect(fake.invoke("app_version")).resolves.toEqual({ version: "9.9.9" });
  });

  it("rejects with the AppError a handler throws", async () => {
    const fake = createFakeApi({
      app_version: () => {
        throw appError("view_only", "Locked.", { computer: "LAPTOP" });
      },
    });
    await expect(fake.invoke("app_version")).rejects.toEqual({
      code: "view_only",
      message: "Locked.",
      details: { computer: "LAPTOP" },
    });
  });
});
