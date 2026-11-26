import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import { appError, createFakeApi, type FakeApi } from "../../api/fake";
import { fakeDetail } from "../../api/fake/photo";
import { createQueryFake, type FakePhoto } from "../../api/fake/query";
import { useSettingsStore } from "../app";
import { resetGrid, useGridStore } from "../grid";
import { Review } from "./Review";

function photos(n: number): FakePhoto[] {
  return Array.from({ length: n }, (_, i) => ({ id: i + 1, rel_path: `day/p${i + 1}.jpg` }));
}

let fake: FakeApi;
const grid = () => useGridStore.getState();
const calls = (command: string) => fake.calls.filter((c) => c.command === command);

beforeEach(() => {
  resetGrid();
  useSettingsStore.setState({ settings: null, error: null });
  fake = createFakeApi({
    ...createQueryFake(photos(5)).handlers,
    photo_detail: ({ id }) => fakeDetail(id, { file_name: `p${id}.jpg`, rel_path: `day/p${id}.jpg` }),
  });
  setApi(fake);
});

afterEach(() => resetGrid());

describe("Review", () => {
  it("shows the grid's current photo in the details panel", async () => {
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result?.count).toBe(5));
    act(() => grid().setSelection({ current: null, anchor: null, selection: [] }));
    expect(screen.getByRole("complementary", { name: "Details" })).toHaveTextContent("No photo selected");

    act(() => grid().setSelection({ current: { id: 3, index: 2 }, anchor: { id: 3, index: 2 }, selection: [3] }));
    expect(await screen.findByRole("heading", { level: 2, name: "p3.jpg" })).toBeInTheDocument();
    await screen.findByText("Taken");
    expect(calls("photo_detail").at(-1)?.args).toEqual({ id: 3 });
  });

  it("clears the selection with Close details", async () => {
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    act(() => grid().setSelection({ current: { id: 2, index: 1 }, anchor: { id: 2, index: 1 }, selection: [2] }));
    await screen.findByText("Taken");
    await userEvent.click(screen.getByRole("button", { name: "Close details" }));
    expect(grid().current).toBeNull();
    expect(grid().selection).toEqual([]);
    expect(screen.getByRole("complementary", { name: "Details" })).toHaveTextContent("No photo selected");
  });

  it("re-runs the query when the photo is not found", async () => {
    fake.handle("photo_detail", () => {
      throw appError("photo_not_found", "That photo is no longer in this album.");
    });
    render(<Review busy={() => false} />);
    await waitFor(() => expect(grid().result).not.toBeNull());
    const before = calls("query_run").length;
    act(() => grid().setSelection({ current: { id: 4, index: 3 }, anchor: { id: 4, index: 3 }, selection: [4] }));
    await waitFor(() => expect(calls("query_run").length).toBe(before + 1));
  });
});
