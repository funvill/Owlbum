import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setApi } from "../../api";
import { createFakeApi } from "../../api/fake";
import { fakeScore } from "../../api/fake/photo";
import { ScoreCard } from "./ScoreCard";

function show(score = fakeScore(), revision = 1) {
  const fake = createFakeApi({ photo_score: () => score });
  setApi(fake);
  const view = (r: number) => <ScoreCard id={3} revision={r} />;
  const { rerender } = render(view(revision));
  return { fake, rerender: (r: number) => rerender(view(r)) };
}

describe("The score card", () => {
  it("shows the total, and on focus the lines with signed points, the total and the footer", async () => {
    show();
    const card = await screen.findByTestId("score-card");
    expect(card).toHaveTextContent("Photo score");
    expect(card).toHaveTextContent("105");
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.focus(card);
    const popup = screen.getByRole("tooltip");
    expect(popup).toHaveTextContent("What made this score");
    const rows = within(popup).getAllByRole("row");
    expect(rows.map((r) => r.textContent)).toEqual([
      "Focussharper than 80 % of this album+28",
      "Exposurewell exposed+19",
      "Colour varietysome colour across the frame+6",
      "Size12 megapixels+10",
      "Bursta similar shot in this burst is better−10",
      "Starstarred+50",
      "Your nudgesUp pressed 2 times more+2",
      "Total105",
    ]);
    expect(within(rows[4]!).getByText("−10")).toHaveClass("ow-score-card__points--minus");
    expect(within(rows[0]!).getByText("+28")).toHaveClass("ow-score-card__points--plus");
    expect(popup).toHaveTextContent("Up and Down nudge it by 1. Star +50, hidden −50.");

    fireEvent.blur(card);
    expect(screen.queryByRole("tooltip")).toBeNull();
    fireEvent.mouseEnter(card);
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
  });

  it("says when the photo has not been read yet", async () => {
    show(
      fakeScore({
        analysed: false,
        total: 10,
        lines: [{ label: "Size", why: "12 megapixels", points: 10 }],
      }),
    );
    const card = await screen.findByTestId("score-card");
    expect(card).toHaveTextContent("still reading the photo");
    fireEvent.focus(card);
    expect(screen.getByRole("tooltip")).toHaveTextContent(
      "Focus, exposure and colour are added once the photo has been read.",
    );
  });

  it("reads the score again when the photo's details are read again", async () => {
    const { fake, rerender } = show();
    await screen.findByTestId("score-card");
    rerender(2);
    await waitFor(() => expect(fake.calls.filter((c) => c.command === "photo_score")).toHaveLength(2));
  });
});
