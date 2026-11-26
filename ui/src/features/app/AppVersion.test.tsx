import { describe, expect, it, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { setApi } from "../../api";
import { appError, createFakeApi } from "../../api/fake";
import { AppVersion } from "./AppVersion";
import { useAppStore } from "./store";

describe("AppVersion", () => {
  beforeEach(() => {
    useAppStore.setState({ version: null, error: null });
  });

  it("shows the version Rust reports", async () => {
    const fake = createFakeApi({ app_version: () => ({ version: "1.2.3" }) });
    setApi(fake);

    render(<AppVersion />);

    expect(await screen.findByText("Version 1.2.3")).toBeInTheDocument();
    expect(fake.calls).toEqual([{ command: "app_version", args: {} }]);
  });

  it("shows the error message when the command fails", async () => {
    setApi(
      createFakeApi({
        app_version: () => {
          throw appError("internal", "The version could not be read.");
        },
      }),
    );

    render(<AppVersion />);

    expect(await screen.findByRole("alert")).toHaveTextContent("The version could not be read.");
  });
});
