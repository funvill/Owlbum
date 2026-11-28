// Writes the English text as a starting point for a translation
// (docs/translating.md). Usage, from ui/:
//   node scripts/text-template.mjs <out.json> [existing <tag>.json]
// Every key of the text modules, with its English: a string as it is, an
// entry that builds its text from arguments as its source code, for the
// translator to turn into a message. Given an existing translation, keys it
// already has keep their translation and only the missing ones are added.
import { readFileSync, writeFileSync } from "node:fs";
import { createServer } from "vite";

const [out, existing] = process.argv.slice(2);
if (!out) {
  console.error("Usage: node scripts/text-template.mjs <out.json> [existing <tag>.json]");
  process.exit(2);
}

const server = await createServer({ server: { middlewareMode: true }, appType: "custom", logLevel: "error" });
try {
  const { englishEntries } = await server.ssrLoadModule("/src/text/i18n/catalogue.ts");
  const kept = existing ? JSON.parse(readFileSync(existing, "utf8")) : {};
  const template = {};
  for (const [key, english] of englishEntries()) {
    if (typeof kept[key] === "string") template[key] = kept[key];
    else template[key] = typeof english === "string" ? english : `TRANSLATE: ${english.toString()}`;
  }
  writeFileSync(out, `${JSON.stringify(template, null, 2)}\n`);
  console.log(`Wrote ${Object.keys(template).length} keys to ${out}`);
} finally {
  await server.close();
}
