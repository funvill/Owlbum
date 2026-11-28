// The pseudo-language (docs/09-visual-design.md, "Languages"): English with
// every letter accented, a third longer, between brackets. "Open folder"
// becomes "[Óþéñ ƒóļðéŕ ~~~~]". Text that shows unchanged did not come from
// the text modules, and a layout that breaks under it will break under a
// longer language. Its tag is Windows' own pseudo-locale tag.

/** The pseudo-language's tag. */
export const PSEUDO_LANGUAGE = "qps-ploc";

const ACCENTED: Record<string, string> = {
  a: "å", b: "ƀ", c: "ç", d: "ð", e: "é", f: "ƒ", g: "ĝ", h: "ĥ", i: "î", j: "ĵ", k: "ķ", l: "ļ", m: "ɱ",
  n: "ñ", o: "ó", p: "þ", q: "ǫ", r: "ŕ", s: "š", t: "ţ", u: "û", v: "ṽ", w: "ŵ", x: "ẋ", y: "ý", z: "ž",
  A: "Å", B: "Ɓ", C: "Ç", D: "Ð", E: "É", F: "Ƒ", G: "Ĝ", H: "Ĥ", I: "Î", J: "Ĵ", K: "Ķ", L: "Ļ", M: "Ṁ",
  N: "Ñ", O: "Ó", P: "Þ", Q: "Ǫ", R: "Ŕ", S: "Š", T: "Ţ", U: "Û", V: "Ṽ", W: "Ŵ", X: "Ẋ", Y: "Ý", Z: "Ž",
};

/** How much longer the pseudo-language makes text: a third, as German often is. */
export const PSEUDO_GROWTH = 1 / 3;

/** A text in the pseudo-language. Empty text stays empty. */
export function pseudo(text: string): string {
  if (text === "") return text;
  const accented = text.replace(/[A-Za-z]/g, (c) => ACCENTED[c] ?? c);
  return `[${accented} ${"~".repeat(Math.ceil(text.length * PSEUDO_GROWTH))}]`;
}
