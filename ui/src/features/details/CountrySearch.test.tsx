import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import { createFakeApi } from "../../api/fake";
import type { CountryMatch } from "../../api/bindings/CountryMatch";
import { CountrySearch, withCountry } from "./CountrySearch";

function setup(onChoose = vi.fn()) {
  const fake = createFakeApi();
  setApi(fake);
  render(<CountrySearch onChoose={onChoose} />);
  return { fake, onChoose, box: screen.getByRole("combobox", { name: "Search for a country…" }) };
}

describe("CountrySearch", () => {
  it("lists the countries whose name contains the typed text", async () => {
    const { fake, box } = setup();
    expect(box).toHaveAttribute("placeholder", "Search for a country…");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();

    await userEvent.type(box, "ger");

    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["Germany", "Algeria", "Niger", "Nigeria"]);
    expect(box).toHaveAttribute("aria-expanded", "true");
    expect(fake.calls.at(-1)).toEqual({ command: "app_place_search", args: { text: "ger" } });
  });

  it("shows nothing and asks nothing for empty text", async () => {
    const { fake, box } = setup();
    await userEvent.type(box, "  ");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(fake.calls).toEqual([]);
  });

  it("chooses a country by click, then clears and closes", async () => {
    const { onChoose, box } = setup();
    await userEvent.type(box, "fran");
    await userEvent.click(await screen.findByRole("option", { name: "France" }));

    expect(onChoose).toHaveBeenCalledTimes(1);
    expect(onChoose.mock.calls[0]![0]).toMatchObject({ name: "France" });
    expect(box).toHaveValue("");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("moves through the list with the arrow keys and chooses with Enter", async () => {
    const { onChoose, box } = setup();
    await userEvent.type(box, "ger");
    await screen.findAllByRole("option");

    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    const active = screen.getByRole("option", { name: "Algeria" });
    expect(active).toHaveAttribute("aria-selected", "true");
    expect(box).toHaveAttribute("aria-activedescendant", active.id);

    await userEvent.keyboard("{ArrowUp}{Enter}");
    expect(onChoose.mock.calls[0]![0]).toMatchObject({ name: "Germany" });
  });

  it("closes the list with Escape", async () => {
    const { onChoose, box } = setup();
    await userEvent.type(box, "fran");
    await screen.findAllByRole("option");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(onChoose).not.toHaveBeenCalled();
  });

  it("ignores an answer that arrives after a newer one", async () => {
    const fake = createFakeApi();
    let release: (value: CountryMatch[]) => void = () => {};
    fake.handle("app_place_search", ({ text }) =>
      text === "f"
        ? new Promise<CountryMatch[]>((resolve) => {
            release = resolve;
          })
        : [{ name: "Finland", lat: 64, lon: 26 }],
    );
    setApi(fake);
    render(<CountrySearch onChoose={vi.fn()} />);
    const box = screen.getByRole("combobox");

    fireEvent.change(box, { target: { value: "f" } });
    fireEvent.change(box, { target: { value: "fi" } });
    expect(await screen.findByRole("option", { name: "Finland" })).toBeInTheDocument();
    await act(async () => release([{ name: "France", lat: 46, lon: 2 }]));
    expect(screen.queryByRole("option", { name: "France" })).not.toBeInTheDocument();
  });

  it("is disabled in a view-only album", () => {
    setApi(createFakeApi());
    render(<CountrySearch onChoose={vi.fn()} disabled />);
    expect(screen.getByRole("combobox")).toBeDisabled();
  });
});

describe("withCountry", () => {
  const france: CountryMatch = { name: "France", lat: 46.6966, lon: 2.4351 };

  it("puts the name in Place name and fills empty coordinates with the central point", () => {
    expect(withCountry({ lat: "", lon: " ", name: "Old" }, france)).toEqual({
      lat: "46.6966",
      lon: "2.4351",
      name: "France",
    });
  });

  it("keeps coordinates already typed", () => {
    expect(withCountry({ lat: "48.85837", lon: "", name: "" }, france)).toEqual({
      lat: "48.85837",
      lon: "",
      name: "France",
    });
  });
});
