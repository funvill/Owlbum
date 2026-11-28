// The pseudo-language over whole screens (docs/09-visual-design.md,
// "Languages"; issue 76): every word the page draws must come out accented,
// which proves it went through the text modules. What may stay plain is
// data (file and folder names from the fake album) and text Rust writes,
// which is translated on the Rust side.

import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setApi } from "../../api";
import { createFakeApi } from "../../api/fake";
import { fakeStatus } from "../../api/fake/album";
import { Toasts } from "../../controls";
import { Shell } from "../../features/album/Shell";
import { resetAlbumSession, useAlbumStore } from "../../features/album/session";
import { resetGrid } from "../../features/grid";
import { SettingsDialog } from "../../features/app/SettingsDialog";
import { AboutDialog } from "../../features/app/AboutDialog";
import { seriousFindings } from "../../test/axe";
import { albumText } from "../album";
import { aboutText, settingsText } from "../app";
import { applyEnglish, currentEntries, englishEntries } from "./catalogue";
import { showLanguage } from "./languages";
import { PSEUDO_GROWTH, PSEUDO_LANGUAGE, pseudo } from "./pseudo";

beforeAll(async () => {
  await showLanguage(PSEUDO_LANGUAGE);
});

afterAll(() => {
  applyEnglish();
});

const initial = useAlbumStore.getState();

beforeEach(() => {
  resetGrid();
  resetAlbumSession();
  useAlbumStore.setState(initial, true);
});

/** Plain words that are data, not interface text. */
const DATA = [
  /^[A-Z]:\\/, // a folder path
  /^[\w-]+\.[a-z0-9]{3,4}$/i, // a file name
  /^https?:\/\//, // an address
  /^Copyright \(c\)/, // the licence, which stays as written
  /^(Florence-2|CLIP) (base|large)$/, // model names, from Rust's model list
  /^English$/, // a language's name, always in its own language
];

/** Every piece of text a person sees or hears in `root`. */
function shownText(root: Element): string[] {
  const found: string[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const text = node.textContent?.trim() ?? "";
    if (text !== "" && node.parentElement?.closest("script, style, svg") == null) found.push(text);
  }
  for (const el of root.querySelectorAll("[aria-label], [title], [placeholder], [alt]")) {
    for (const name of ["aria-label", "title", "placeholder", "alt"]) {
      const value = el.getAttribute(name)?.trim();
      if (value) found.push(value);
    }
  }
  return found;
}

/** The pieces that hold a plain English word and are not data. */
function plain(root: Element): string[] {
  // Strip what the pseudo-language made, then look for plain letters left.
  return shownText(root).filter((text) => {
    const outside = text.replace(/\[[^\]]*\]/g, " ").trim();
    return /[A-Za-z]{2,}/.test(outside) && !DATA.some((re) => re.test(outside));
  });
}

describe("the pseudo-language", () => {
  it("accents every letter and makes text a third longer", () => {
    const out = pseudo("Open folder");
    expect(out).not.toMatch(/[A-Za-z]/);
    expect(out.length).toBeGreaterThanOrEqual("Open folder".length * (1 + PSEUDO_GROWTH));
  });

  it("reaches every text entry", () => {
    const now = currentEntries();
    expect(now.size).toBeGreaterThan(500);
    for (const [key, english] of englishEntries()) {
      const used = now.get(key);
      if (typeof english === "string") expect(used, key).toBe(pseudo(english));
      else expect(used, key).not.toBe(english);
    }
  });

  it("leaves no plain English on the start screen", async () => {
    setApi(createFakeApi());
    const { container } = render(
      <>
        <Shell />
        <Toasts />
      </>,
    );
    expect(await screen.findByRole("button", { name: albumText.openFolder })).toBeInTheDocument();
    await act(async () => {});
    expect(plain(container)).toEqual([]);
    expect(await seriousFindings(container)).toEqual([]);
  });

  it("leaves no plain English on the review screen", async () => {
    const status = fakeStatus("D:\\Trip", { photos: 3 });
    const fake = createFakeApi({ album_open: () => ({ status, session: null }), album_status: () => status });
    setApi(fake);
    render(
      <>
        <Shell />
        <Toasts />
      </>,
    );
    await waitFor(() => expect(fake.calls.some((c) => c.command === "app_startup")).toBe(true));
    act(() => fake.emit("open_folder", { path: "D:\\Trip", photo: null }));
    expect(await screen.findByRole("region", { name: albumText.photosRegion }, { timeout: 3000 })).toBeInTheDocument();
    await act(async () => {});
    expect(plain(document.body)).toEqual([]);
  });

  it("leaves no plain English in Settings", async () => {
    setApi(createFakeApi());
    render(<SettingsDialog open onClose={() => {}} />);
    expect(await screen.findByRole("checkbox", { name: settingsText.reopenLast })).toBeInTheDocument();
    await act(async () => {});
    expect(plain(document.body)).toEqual([]);
    expect(await seriousFindings(document.body)).toEqual([]);
    await userEvent.click(screen.getByRole("radio", { name: settingsText.themeLight }));
  });

  it("leaves no plain English in About", async () => {
    setApi(createFakeApi());
    render(<AboutDialog open onClose={() => {}} />);
    expect(await screen.findByRole("heading", { name: aboutText.title })).toBeInTheDocument();
    await act(async () => {});
    expect(plain(document.body)).toEqual([]);
  });
});
