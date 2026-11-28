import { afterEach, describe, expect, it } from "vitest";
import { focusOf, isRelease, keyMatches, matchKey } from "./match";
import type { KeyLike, MatchContext } from "./match";

/** A keydown with no modifiers, focus nowhere in particular. */
function key(k: string, extra: Partial<KeyLike> = {}): KeyLike {
  return {
    key: k,
    code: "",
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    repeat: false,
    ...extra,
  };
}

const browse: MatchContext = { place: "browse", focus: "none" };
const viewer: MatchContext = { place: "viewer", focus: "none" };
const edit: MatchContext = { place: "edit", focus: "none" };

const id = (e: KeyLike, ctx: MatchContext = browse) => matchKey(e, ctx)?.id ?? null;

describe("places", () => {
  it("finds a place's own rows", () => {
    expect(id(key("ArrowRight"))).toBe("browse.next");
    expect(id(key("ArrowRight"), viewer)).toBe("viewer.next");
    expect(id(key("Enter"), viewer)).toBe("viewer.close");
    expect(id(key("Enter"), edit)).toBe("edit.done");
    expect(id(key("Escape"), edit)).toBe("edit.cancel");
  });

  it("finds everywhere rows in every place", () => {
    for (const ctx of [browse, viewer, edit]) expect(id(key("?"), ctx)).toBe("app.shortcuts");
    expect(id(key("F6"), edit)).toBe("app.next_region");
    expect(id(key("F6", { shiftKey: true }), viewer)).toBe("app.previous_region");
  });

  it("gives browse keys nothing in the viewer or edit mode", () => {
    expect(id(key("Delete"), viewer)).toBeNull();
    expect(id(key("E"), viewer)).toBeNull();
    expect(id(key("a", { ctrlKey: true }), viewer)).toBeNull();
    expect(id(key("s"), edit)).toBeNull();
    expect(id(key("ArrowLeft"), edit)).toBeNull();
  });

  it("stars with both F and S", () => {
    expect(id(key("f"))).toBe("browse.star");
    expect(id(key("s"))).toBe("browse.star");
    expect(id(key("s"), viewer)).toBe("viewer.star");
  });

  it("opens the viewer with Enter and Space; Space plays in the viewer", () => {
    expect(id(key("Enter"))).toBe("browse.open_viewer");
    expect(id(key(" "))).toBe("browse.open_viewer");
    expect(id(key(" "), viewer)).toBe("viewer.play");
  });
});

describe("the old menu bar's keys", () => {
  it("are the page's now, everywhere and while typing (issue 197)", () => {
    expect(id(key("F1"))).toBe("app.about");
    expect(id(key("F11"))).toBe("app.full_screen");
    expect(id(key("o", { ctrlKey: true }))).toBe("app.open_folder");
    expect(id(key("q", { ctrlKey: true }), viewer)).toBe("app.quit");
    expect(id(key("w", { ctrlKey: true }), { place: "browse", focus: "typing" })).toBe("app.close_folder");
    // A test run is a development build.
    expect(id(key("r", { ctrlKey: true }))).toBe("app.reload");
  });

  it("can still be tested directly", () => {
    expect(keyMatches(key("o", { ctrlKey: true }), "app.open_folder")).toBe(true);
    expect(keyMatches(key("I", { ctrlKey: true, shiftKey: true }), "app.developer_tools")).toBe(
      true,
    );
  });
});

describe("letters", () => {
  it("match in either case (Caps Lock)", () => {
    expect(id(key("H"))).toBe("browse.hide");
    expect(id(key("h"))).toBe("browse.hide");
    expect(id(key("Z", { ctrlKey: true }))).toBe("browse.undo");
    expect(id(key("z", { ctrlKey: true }))).toBe("browse.undo");
  });

  it("fall back to the physical key on a layout of another alphabet", () => {
    expect(id(key("р", { code: "KeyH" }))).toBe("browse.hide");
    expect(id(key("ф", { code: "KeyA", ctrlKey: true }))).toBe("browse.select_all");
  });

  it("use the letter typed when it is a Latin letter, not the physical key", () => {
    // AZERTY: the key where QWERTY has A types q.
    expect(id(key("h", { code: "KeyJ" }))).toBe("browse.hide");
    // The typed q makes Ctrl+Q (Quit), wherever the key sits.
    expect(id(key("q", { code: "KeyA", ctrlKey: true }))).toBe("app.quit");
    expect(id(key("a", { code: "KeyQ", ctrlKey: true }))).toBe("browse.select_all");
  });

  it("never fall back for punctuation or symbols", () => {
    // Dvorak: the key where QWERTY has E types a full stop.
    expect(id(key(".", { code: "KeyE" }))).toBeNull();
    expect(id(key(";", { code: "KeyZ", ctrlKey: true }))).toBeNull();
    // Linux AltGr+E gives the euro sign with no Ctrl or Alt.
    expect(id(key("€", { code: "KeyE" }))).toBeNull();
    // Accented Latin letters are Latin: no fallback either.
    expect(id(key("é", { code: "KeyE" }))).toBeNull();
  });
});

describe("modifiers", () => {
  it("ignore a key pressed with a modifier the row does not name", () => {
    expect(id(key("S", { shiftKey: true }))).toBeNull();
    expect(id(key("s", { ctrlKey: true }))).toBeNull();
    expect(id(key("s", { altKey: true }))).toBeNull();
    expect(id(key("Z", { ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(id(key("Enter", { ctrlKey: true }))).toBeNull();
    expect(id(key("Escape", { shiftKey: true }), edit)).toBeNull();
  });

  it("ignore Meta (the Windows or Super key) always", () => {
    expect(id(key("s", { metaKey: true }))).toBeNull();
    expect(id(key("ArrowLeft", { metaKey: true }))).toBeNull();
    expect(id(key("?", { metaKey: true, shiftKey: true }))).toBeNull();
  });

  it("need the modifiers the row names", () => {
    expect(id(key("a"))).toBeNull();
    expect(id(key("a", { ctrlKey: true }))).toBe("browse.select_all");
    expect(id(key("ArrowLeft", { shiftKey: true }))).toBe("browse.extend_previous");
    expect(id(key("ArrowLeft"))).toBe("browse.previous");
  });
});

describe("symbols", () => {
  it("match on the character, whatever Shift it took", () => {
    expect(id(key("?", { shiftKey: true }))).toBe("app.shortcuts");
    expect(id(key("+", { shiftKey: true }), viewer)).toBe("viewer.zoom_in");
    expect(id(key("+"), viewer)).toBe("viewer.zoom_in");
    expect(id(key("="), viewer)).toBe("viewer.zoom_in");
    expect(id(key("-"), viewer)).toBe("viewer.zoom_out");
    expect(id(key("0", { shiftKey: true }), viewer)).toBe("viewer.fit");
  });

  it("match through AltGr, which Windows reports as Ctrl+Alt", () => {
    const altGr = { ctrlKey: true, altKey: true };
    expect(id(key("[", altGr))).toBe("browse.rotate_left");
    expect(id(key("]", altGr))).toBe("browse.rotate_right");
    expect(id(key("\\", altGr))).toBe("browse.before");
    expect(id(key("[", { getModifierState: (m) => m === "AltGraph" }))).toBe("browse.rotate_left");
  });

  it("are ignored with Ctrl or Alt alone", () => {
    expect(id(key("[", { ctrlKey: true }))).toBeNull();
    expect(id(key("]", { altKey: true }))).toBeNull();
  });
});

describe("repeat and hold", () => {
  it("acts on auto-repeat only for repeat rows", () => {
    expect(matchKey(key("ArrowRight", { repeat: true }), browse)).toMatchObject({
      id: "browse.next",
      act: true,
    });
    expect(matchKey(key("ArrowUp", { repeat: true }), browse)).toMatchObject({
      id: "browse.score_up",
      act: false,
    });
    expect(matchKey(key("Enter", { repeat: true }), edit)).toMatchObject({
      id: "edit.done",
      act: false,
    });
    expect(matchKey(key("s"), browse)).toMatchObject({ act: true });
  });

  it("starts a hold once; later auto-repeats do not act", () => {
    expect(matchKey(key("\\"), browse)).toMatchObject({ id: "browse.before", act: true });
    expect(matchKey(key("\\", { repeat: true }), browse)).toMatchObject({ act: false });
  });

  it("ends a hold on the key's release, whatever the modifiers or focus", () => {
    expect(isRelease(key("\\"), "browse.before")).toBe(true);
    expect(isRelease(key("\\", { shiftKey: true, ctrlKey: true }), "viewer.before")).toBe(true);
    expect(isRelease(key("["), "edit.before")).toBe(false);
  });
});

describe("handled events", () => {
  it("are left alone", () => {
    expect(id(key("s", { defaultPrevented: true }))).toBeNull();
    expect(id(key("s", { isComposing: true }))).toBeNull();
    expect(id(key("Dead"))).toBeNull();
  });
});

describe("focus", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  function focusOn(html: string, selector: string) {
    document.body.innerHTML = html;
    const el = document.querySelector(selector);
    if (!el) throw new Error(`no ${selector}`);
    return el;
  }

  it.each([
    ["a text box", '<input id="t">', "#t", "typing"],
    ["a search box", '<input id="t" type="search">', "#t", "typing"],
    ["a text area", '<textarea id="t"></textarea>', "#t", "typing"],
    ["a drop-down", '<select id="t"></select>', "#t", "typing"],
    ["an editable element", '<div contenteditable="true"><p id="t">x</p></div>', "#t", "typing"],
    ["a slider", '<input id="t" type="range">', "#t", "none"],
    ["a slider role", '<div id="t" role="slider" tabindex="0"></div>', "#t", "none"],
    ["a checkbox", '<input id="t" type="checkbox">', "#t", "control"],
    ["a button", '<button id="t">x</button>', "#t", "control"],
    ["a link", '<a id="t" href="#">x</a>', "#t", "control"],
    ["a grid tile", '<div role="grid"><div id="t" role="gridcell" tabindex="0"></div></div>', "#t", "none"],
    ["a menu item", '<div role="menu"><div id="t" role="menuitem">x</div></div>', "#t", "menu"],
    ["a dialog", '<div role="dialog"><button id="t">x</button></div>', "#t", "dialog"],
    ["a dialog element", '<dialog open><span id="t">x</span></dialog>', "#t", "dialog"],
    ["the Filters panel", '<div data-keys="dialog"><button id="t">x</button></div>', "#t", "dialog"],
    [
      "a key surface such as the viewer",
      '<div role="dialog" data-keys="surface"><button id="t">x</button></div>',
      "#t",
      "control",
    ],
    [
      "a dialog inside a key surface",
      '<div data-keys="surface"><div role="alertdialog"><button id="t">x</button></div></div>',
      "#t",
      "dialog",
    ],
    ["the page", "<div id='t'></div>", "#t", "none"],
  ])("counts %s", (_what, html, selector, expected) => {
    expect(focusOf(focusOn(html, selector))).toBe(expected);
  });

  it("counts nothing as typing when there is no element", () => {
    expect(focusOf(null)).toBe("none");
    expect(focusOf(window)).toBe("none");
  });

  it("reads focus from the event's target when the context gives none", () => {
    const input = focusOn('<input id="t">', "#t");
    expect(matchKey(key("s", { target: input }), { place: "browse" })).toBeNull();
    const div = focusOn("<div id='t'></div>", "#t");
    expect(matchKey(key("s", { target: div }), { place: "browse" })?.id).toBe("browse.star");
  });

  it("while typing, ignores every row except region moves", () => {
    const typing: MatchContext = { place: "browse", focus: "typing" };
    for (const k of ["s", "ArrowLeft", "?", "[", "Delete", "Enter", "Escape"]) {
      expect(id(key(k), typing), k).toBeNull();
    }
    expect(id(key("z", { ctrlKey: true }), typing)).toBeNull();
    expect(id(key("c", { ctrlKey: true }), typing)).toBeNull();
    expect(id(key("F6"), typing)).toBe("app.next_region");
  });

  it("in a dialog, ignores every row", () => {
    const dialog: MatchContext = { place: "browse", focus: "dialog" };
    for (const k of ["s", "?", "F6", "Escape", "Enter"]) expect(id(key(k), dialog), k).toBeNull();
  });

  it("on a button or link, leaves Enter and Space to it", () => {
    const control: MatchContext = { place: "browse", focus: "control" };
    expect(id(key("Enter"), control)).toBeNull();
    expect(id(key(" "), control)).toBeNull();
    expect(id(key("Enter"), { place: "edit", focus: "control" })).toBeNull();
    expect(id(key(" "), { place: "viewer", focus: "control" })).toBeNull();
    expect(id(key("s"), control)).toBe("browse.star");
    expect(id(key("ArrowRight"), control)).toBe("browse.next");
  });

  it("in a menu, leaves its own keys to it and lets the rest through", () => {
    const menu: MatchContext = { place: "browse", focus: "menu" };
    for (const k of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "Enter", " ", "Escape"]) {
      expect(id(key(k), menu), k).toBeNull();
    }
    expect(id(key("h"), menu)).toBe("browse.hide");
    expect(id(key("Delete"), menu)).toBe("browse.trash");
  });

  it("on a slider in edit mode, Enter is still Done", () => {
    const slider = focusOn('<input id="t" type="range">', "#t");
    expect(matchKey(key("Enter", { target: slider }), { place: "edit" })?.id).toBe("edit.done");
  });
});
