import { describe, expect, it } from "vitest";
import { isAppError, toAppError } from "./errors";

describe("toAppError", () => {
  it("passes an AppError through", () => {
    const e = { code: "no_album" as const, message: "No album is open." };
    expect(toAppError(e)).toBe(e);
    expect(isAppError(e)).toBe(true);
  });

  it("turns anything else into an internal error", () => {
    const e = toAppError("command app_nope not found");
    expect(e.code).toBe("internal");
    expect(e.details).toEqual({ cause: "command app_nope not found" });
    expect(isAppError("x")).toBe(false);
    expect(isAppError(null)).toBe(false);
  });
});
