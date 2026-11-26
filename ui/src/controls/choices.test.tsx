import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Checkbox, RadioGroup } from "./Checkbox";
import { Slider } from "./Slider";
import { Segmented } from "./Segmented";
import { seriousFindings } from "../test/axe";

describe("Checkbox", () => {
  it("toggles from its label as well as its box", async () => {
    const onChange = vi.fn();
    render(<Checkbox label="Show hidden photos" checked={false} onChange={onChange} />);
    await userEvent.click(screen.getByText("Show hidden photos"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("shows the mixed state", () => {
    render(<Checkbox label="All" checked="mixed" onChange={() => {}} />);
    const box = screen.getByRole("checkbox", { name: "All" }) as HTMLInputElement;
    expect(box.indeterminate).toBe(true);
    expect(box).toBePartiallyChecked();
  });

  it("can be disabled", async () => {
    const onChange = vi.fn();
    render(<Checkbox label="All" checked onChange={onChange} disabled />);
    expect(screen.getByRole("checkbox", { name: "All" })).toBeDisabled();
  });
});

describe("RadioGroup", () => {
  function Group() {
    const [value, setValue] = useState("album");
    return (
      <RadioGroup
        label="Where edits are kept"
        value={value}
        onChange={setValue}
        options={[
          { value: "album", label: "In the album", description: "The photo file is not changed." },
          { value: "file", label: "In the photo file" },
        ]}
      />
    );
  }

  it("is a labelled group with one choice on, and descriptions under labels", async () => {
    render(<Group />);
    const group = screen.getByRole("radiogroup", { name: "Where edits are kept" });
    expect(group).toBeInTheDocument();
    const album = screen.getByRole("radio", { name: "In the album" });
    expect(album).toBeChecked();
    expect(album).toHaveAccessibleDescription("The photo file is not changed.");

    await userEvent.click(screen.getByText("In the photo file"));
    expect(screen.getByRole("radio", { name: "In the photo file" })).toBeChecked();
    expect(album).not.toBeChecked();
  });
});

describe("Slider", () => {
  function Straighten({ onChange }: { onChange?: (v: number) => void }) {
    const [value, setValue] = useState(10);
    return (
      <Slider
        label="Straighten"
        min={-45}
        max={45}
        step={0.5}
        neutral={0}
        value={value}
        onChange={(v) => {
          setValue(v);
          onChange?.(v);
        }}
      />
    );
  }

  it("is a labelled slider and shows its value as a number beside it", () => {
    render(<Straighten />);
    const slider = screen.getByRole("slider", { name: "Straighten" });
    expect(slider).toHaveValue("10");
    expect(screen.getByText("10")).toHaveClass("ow-slider__value");
  });

  it("reports changes as numbers", () => {
    const onChange = vi.fn();
    render(<Straighten onChange={onChange} />);
    fireEvent.change(screen.getByRole("slider", { name: "Straighten" }), { target: { value: "12.5" } });
    expect(onChange).toHaveBeenCalledWith(12.5);
  });

  it("returns to the neutral middle on double-click", async () => {
    render(<Straighten />);
    const slider = screen.getByRole("slider", { name: "Straighten" });
    await userEvent.dblClick(slider);
    expect(slider).toHaveValue("0");
  });

  it("fills from the middle outwards when it has a neutral middle", () => {
    render(<Straighten />);
    const slider = screen.getByRole("slider", { name: "Straighten" });
    expect(slider.style.getPropertyValue("--_from")).toBe("50%");
    expect(slider.style.getPropertyValue("--_to")).toBe(`${(55 / 90) * 100}%`);
  });

  it("fills from the start without a neutral middle", () => {
    render(<Slider label="Size" min={96} max={320} step={8} value={160} onChange={() => {}} />);
    const slider = screen.getByRole("slider", { name: "Size" });
    expect(slider.style.getPropertyValue("--_from")).toBe("0%");
  });
});

describe("Segmented", () => {
  function Views({ as }: { as?: "radiogroup" | "tablist" }) {
    const [value, setValue] = useState("grid");
    return (
      <Segmented
        label="View"
        as={as}
        value={value}
        onChange={setValue}
        options={[
          { value: "grid", label: "Grid" },
          { value: "timeline", label: "Timeline" },
          { value: "map", label: "Map", disabled: true },
        ]}
      />
    );
  }

  it("is a radio group with one tab stop, moved by the arrow keys", async () => {
    render(<Views />);
    expect(screen.getByRole("radiogroup", { name: "View" })).toBeInTheDocument();
    const grid = screen.getByRole("radio", { name: "Grid" });
    const timeline = screen.getByRole("radio", { name: "Timeline" });
    expect(grid).toHaveAttribute("aria-checked", "true");
    expect(grid).toHaveAttribute("tabindex", "0");
    expect(timeline).toHaveAttribute("tabindex", "-1");

    await userEvent.tab();
    expect(grid).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(timeline).toHaveFocus();
    expect(timeline).toHaveAttribute("aria-checked", "true");
    // The disabled Map is skipped, so Right wraps to Grid.
    await userEvent.keyboard("{ArrowRight}");
    expect(grid).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(timeline).toHaveFocus();
  });

  it("is a tab list where it switches views", async () => {
    render(<Views as="tablist" />);
    expect(screen.getByRole("tablist", { name: "View" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Grid" })).toHaveAttribute("aria-selected", "true");
    await userEvent.click(screen.getByRole("tab", { name: "Timeline" }));
    expect(screen.getByRole("tab", { name: "Timeline" })).toHaveAttribute("aria-selected", "true");
  });
});

describe("choices", () => {
  it("have no serious accessibility findings", async () => {
    render(
      <>
        <Checkbox label="All" checked="mixed" onChange={() => {}} />
        <RadioGroup label="Edits" value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} />
        <Slider label="Size" min={0} max={10} value={5} onChange={() => {}} />
        <Segmented label="View" value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} />
      </>,
    );
    expect(await seriousFindings()).toEqual([]);
  });
});
