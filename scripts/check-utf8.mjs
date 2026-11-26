// Fails when a tracked text file is not valid UTF-8 (for example a stray
// Windows-1252 byte where a typographic character was meant).
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const binary = /\.(jxl|png|jpe?g|gif|webp|bmp|tiff?|ico|icns|woff2?|ttf|otf|dng|mp4|sqlite)$/i;
const files = execFileSync("git", ["ls-files", "-z"], { encoding: "utf8" })
  .split("\0")
  .filter((f) => f && !binary.test(f));

const decoder = new TextDecoder("utf-8", { fatal: true });
const bad = [];
for (const file of files) {
  try {
    decoder.decode(readFileSync(file));
  } catch {
    bad.push(file);
  }
}
if (bad.length > 0) {
  console.error(`Not valid UTF-8:\n  ${bad.join("\n  ")}`);
  process.exit(1);
}
console.log(`${files.length} files ok`);
