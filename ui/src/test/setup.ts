import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach } from "vitest";
import { cleanup } from "@testing-library/react";
import { setApi } from "../api";
import { createFakeApi } from "../api/fake";

// Every test starts with a fresh fake backend. A test that needs particular
// replies calls setApi(createFakeApi({ ... })) itself.
beforeEach(() => {
  setApi(createFakeApi());
});

afterEach(() => {
  cleanup();
});
