import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { setApi } from "../api";
import { createFakeApi } from "../api/fake";

// Testing Library waits with setTimeout and advances fake timers only when
// it sees Jest's; hand it Vitest's, so tests can use vi.useFakeTimers().
(globalThis as unknown as { jest: unknown }).jest = {
  advanceTimersByTime: (ms: number) => vi.advanceTimersByTime(ms),
};

// Every test starts with a fresh fake backend. A test that needs particular
// replies calls setApi(createFakeApi({ ... })) itself.
beforeEach(() => {
  setApi(createFakeApi());
});

afterEach(() => {
  cleanup();
});
