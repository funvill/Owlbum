import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { setApi } from "../api";
import { createFakeApi } from "../api/fake";
import { useSettingsStore } from "../features/app/settings";

// Testing Library waits with setTimeout and advances fake timers only when
// it sees Jest's; hand it Vitest's, so tests can use vi.useFakeTimers().
(globalThis as unknown as { jest: unknown }).jest = {
  advanceTimersByTime: (ms: number) => vi.advanceTimersByTime(ms),
};

// Every test starts with a fresh fake backend. A test that needs particular
// replies calls setApi(createFakeApi({ ... })) itself.
// Settings start unloaded too: a save timer that fired late in one test
// (the thumbnail size, 300 ms after the slider moved) must not hand its
// value to the next test, which made the grid tests fail under load (#113).
beforeEach(() => {
  setApi(createFakeApi());
  useSettingsStore.setState({ settings: null, error: null });
});

afterEach(() => {
  cleanup();
});
