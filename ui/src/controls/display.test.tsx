import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Banner } from "./Banner";
import { ProgressBar } from "./ProgressBar";
import { Chip, Pill } from "./Chip";
import { EmptyState } from "./EmptyState";
import { MiddleEllipsis, splitForEllipsis } from "./MiddleEllipsis";
import { Button } from "./Button";
import { layerRoot } from "./layers";
import { seriousFindings } from "../test/axe";

describe("Banner", () => {
  it("is a status with a large icon and its sentence", () => {
    render(<Banner tone="warn" icon="lock">This album is view-only.</Banner>);
    const banner = screen.getByRole("status");
    expect(banner).toHaveTextContent("This album is view-only.");
    expect(banner).toHaveClass("ow-banner--warn");
    expect(banner.querySelector("svg")).toHaveAttribute("width", "20");
  });

  it("is an alert when it reports an error, with the alert icon by default", () => {
    render(<Banner tone="error">The album database is damaged.</Banner>);
    expect(screen.getByRole("alert")).toHaveClass("ow-banner--error");
  });

  it("carries small buttons and an x when it can be dismissed", async () => {
    const onDismiss = vi.fn();
    render(
      <Banner tone="info" actions={<Button size="sm">Show</Button>} onDismiss={onDismiss}>
        Some files are not shown.
      </Banner>,
    );
    expect(screen.getByRole("button", { name: "Show" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});

describe("ProgressBar", () => {
  it("reports its value and total, with a visible label", () => {
    render(<ProgressBar label="Reading 1,240 of 5,000" value={1240} max={5000} />);
    const bar = screen.getByRole("progressbar", { name: "Reading 1,240 of 5,000" });
    expect(bar).toHaveAttribute("aria-valuenow", "1240");
    expect(bar).toHaveAttribute("aria-valuemax", "5000");
    expect(screen.getByText("Reading 1,240 of 5,000")).toBeVisible();
  });

  it("has no value when the total is unknown", () => {
    render(<ProgressBar label="Looking for photos" />);
    const bar = screen.getByRole("progressbar", { name: "Looking for photos" });
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveClass("ow-progress--unknown");
  });

  it("shows a problem in the error colour with an alert icon", () => {
    render(<ProgressBar label="Stopped: the disk is full" value={3} max={10} failed />);
    const label = screen.getByText("Stopped: the disk is full");
    expect(label.closest(".ow-progress")).toHaveClass("ow-progress--failed");
    expect(label.closest(".ow-progress")?.querySelector("svg")).not.toBeNull();
  });
});

describe("Chip", () => {
  it("is a plain label, or a button when it can be clicked", async () => {
    const onClick = vi.fn();
    render(
      <>
        <Chip label="lake" count={2} />
        <Chip label="tree" onClick={onClick} />
      </>,
    );
    expect(screen.getByText("lake ×2")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "tree" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("is an active filter with an x that removes it", async () => {
    const onRemove = vi.fn();
    render(<Chip label="star:yes" active onRemove={onRemove} />);
    expect(screen.getByText("star:yes").closest(".ow-chip")).toHaveClass("ow-chip--active");
    await userEvent.click(screen.getByRole("button", { name: "Remove star:yes" }));
    expect(onRemove).toHaveBeenCalledOnce();
  });
});

describe("Pill", () => {
  it("draws a badge in a tone", () => {
    render(<Pill tone="unsaved">Edited · not saved</Pill>);
    expect(screen.getByText("Edited · not saved")).toHaveClass("ow-pill", "ow-pill--unsaved");
  });
});

describe("EmptyState", () => {
  it("shows a heading, a sentence and at most one button", () => {
    render(
      <EmptyState heading="No photos match the filter" action={<Button>Clear filter</Button>}>
        Try fewer filters.
      </EmptyState>,
    );
    expect(screen.getByRole("heading", { name: "No photos match the filter" })).toBeInTheDocument();
    expect(screen.getByText("Try fewer filters.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear filter" })).toBeInTheDocument();
  });

  it("can show the owl mark, hidden from assistive technology", () => {
    const { container } = render(<EmptyState heading="Open a folder of photos" mark />);
    const mark = container.querySelector(".ow-owl-mark");
    expect(mark).not.toBeNull();
    expect(mark).toHaveAttribute("aria-hidden", "true");
    expect(mark).toHaveAttribute("width", "64");
  });
});

describe("MiddleEllipsis", () => {
  it("keeps the end of a name together, so the extension stays readable", () => {
    expect(splitForEllipsis("IMG_20240716_183011_holiday.jpg")).toEqual(["IMG_20240716_183011_ho", "liday.jpg"]);
    expect(splitForEllipsis("a.jpg")).toEqual(["", "a.jpg"]);
  });

  it("gives the whole text to assistive technology", () => {
    render(<MiddleEllipsis text="IMG_20240716_183011_holiday.jpg" />);
    const el = screen.getByText((_, node) => node?.classList.contains("ow-middle-ellipsis") ?? false);
    expect(el).toHaveAttribute("aria-label", "IMG_20240716_183011_holiday.jpg");
  });
});

describe("layer root", () => {
  it("is one element at the end of the page", () => {
    const root = layerRoot();
    expect(layerRoot()).toBe(root);
    expect(document.body.lastElementChild).toBe(root);
  });
});

describe("display controls", () => {
  it("have no serious accessibility findings", async () => {
    render(
      <>
        <Banner tone="info" onDismiss={() => {}}>
          Some files are not shown.
        </Banner>
        <ProgressBar label="Reading" value={1} max={2} />
        <Chip label="lake" onClick={() => {}} />
        <Chip label="star:yes" active onRemove={() => {}} />
        <EmptyState heading="No photos" mark />
      </>,
    );
    expect(await seriousFindings()).toEqual([]);
    expect(layerRoot()).toBeInTheDocument();
  });
});
