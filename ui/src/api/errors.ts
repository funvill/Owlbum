import type { AppError } from "./bindings/AppError";
import { errorText } from "../text/errors";

/** Whether `value` has the shape of an `AppError`. */
export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v.code === "string" && typeof v.message === "string";
}

/**
 * Turns whatever a command rejected with into an `AppError`. Rust always
 * sends one; anything else (Tauri's own failures) becomes `internal`.
 */
export function toAppError(value: unknown): AppError {
  if (isAppError(value)) return value;
  const detail = value instanceof Error ? value.message : String(value);
  return { code: "internal", message: errorText.internal, details: { cause: detail } };
}
