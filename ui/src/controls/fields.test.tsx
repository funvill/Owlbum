import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SearchBox, SelectField, TextArea, TextField } from "./fields";
import { seriousFindings } from "../test/axe";

describe("TextField", () => {
  it("is labelled, and reports each change", async () => {
    const onChange = vi.fn();
    render(<TextField label="Name" value="" onChange={onChange} placeholder="A name" />);
    const input = screen.getByRole("textbox", { name: "Name" });
    expect(input).toHaveAttribute("placeholder", "A name");
    await userEvent.type(input, "a");
    expect(onChange).toHaveBeenCalledWith("a");
  });

  it("can hide its label and keep its name", () => {
    render(<TextField label="Album name" hideLabel value="" onChange={() => {}} />);
    expect(screen.getByRole("textbox", { name: "Album name" })).toBeInTheDocument();
    expect(screen.getByText("Album name")).toHaveClass("ow-visually-hidden");
  });

  it("shows why a value is invalid under the field, with an alert icon", () => {
    render(<TextField label="Latitude" value="200" onChange={() => {}} error="Enter a number from −90 to 90." />);
    const input = screen.getByRole("textbox", { name: "Latitude" });
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("Enter a number from −90 to 90.");
    const message = screen.getByText("Enter a number from −90 to 90.");
    expect(message.closest(".ow-field__error")?.querySelector("svg")).not.toBeNull();
  });

  it("can be disabled", () => {
    render(<TextField label="Name" value="" onChange={() => {}} disabled />);
    expect(screen.getByRole("textbox", { name: "Name" })).toBeDisabled();
  });
});

describe("SelectField", () => {
  it("offers its options and reports the choice", async () => {
    const onChange = vi.fn();
    render(
      <SelectField
        label="Sort by"
        value="date"
        onChange={onChange}
        options={[
          { value: "date", label: "Date taken" },
          { value: "score", label: "Score" },
        ]}
      />,
    );
    const select = screen.getByRole("combobox", { name: "Sort by" });
    expect(select).toHaveValue("date");
    await userEvent.selectOptions(select, "score");
    expect(onChange).toHaveBeenCalledWith("score");
  });
});

describe("SearchBox", () => {
  function Search() {
    const [value, setValue] = useState("");
    return <SearchBox label="Search photos" value={value} onChange={setValue} clearLabel="Clear search" />;
  }

  it("has a search icon, and a clear button once it holds text", async () => {
    render(<Search />);
    const box = screen.getByRole("searchbox", { name: "Search photos" });
    expect(box.parentElement?.querySelector("svg")).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();

    await userEvent.type(box, "lake");
    await userEvent.click(screen.getByRole("button", { name: "Clear search" }));
    expect(box).toHaveValue("");
    expect(box).toHaveFocus();
    expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();
  });
});

describe("TextArea", () => {
  function Notes({ initial }: { initial: string }) {
    const [value, setValue] = useState(initial);
    return <TextArea label="Notes" value={value} onChange={setValue} />;
  }

  it("is three lines high at first", () => {
    render(<Notes initial="" />);
    expect(screen.getByRole("textbox", { name: "Notes" })).toHaveAttribute("rows", "3");
  });

  it("grows with its text up to twelve lines, then scrolls", async () => {
    render(<Notes initial={"1\n2\n3\n4\n5"} />);
    const area = screen.getByRole("textbox", { name: "Notes" });
    expect(area).toHaveAttribute("rows", "5");
    await userEvent.type(area, "{Enter}6{Enter}7{Enter}8{Enter}9{Enter}10{Enter}11{Enter}12{Enter}13{Enter}14");
    expect(area).toHaveAttribute("rows", "12");
  });
});

describe("fields", () => {
  it("have no serious accessibility findings", async () => {
    render(
      <>
        <TextField label="Name" value="" onChange={() => {}} error="Enter a name." />
        <SelectField label="Sort by" value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} />
        <SearchBox label="Search photos" value="lake" onChange={() => {}} clearLabel="Clear search" />
        <TextArea label="Notes" value="" onChange={() => {}} />
      </>,
    );
    expect(await seriousFindings()).toEqual([]);
  });
});
