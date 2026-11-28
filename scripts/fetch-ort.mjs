// Puts Microsoft's ONNX Runtime library, checked against a known SHA-256,
// at resources/onnxruntime.dll, where the installer build takes it from
// (docs/08-desktop-and-release.md, "Bundle configuration"; docs/07-tagging.md,
// "Packaging"), with its licence and its own third-party notices beside it
// for scripts/notices.mjs. None of these is ever committed.
//
// Usage: node scripts/fetch-ort.mjs
//   Downloads the NuGet package (about 120 MB, kept only in memory), checks
//   its SHA-256 and takes the three files out of it. Files already in place
//   with the right library are kept, and nothing is downloaded.
//
// Windows x64 only for now (Linux work is paused): libonnxruntime.so comes
// from the same package when it resumes.

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

const VERSION = "1.31.1";
const PACKAGE_URL = `https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime/${VERSION}`;
const PACKAGE_SHA256 = "6871ccbc97f3d5bdb4095a1dcec33aa78d56f6ed8de5980ef5fa1d9f2e7c679a";
const ENTRY = "runtimes/win-x64/native/onnxruntime.dll";
const LIBRARY_SHA256 = "cb2fa518b18356617f6f9f460c012501da2819ec7e5c1d80bd8f5de20492db0c";
const LIBRARY_BYTES = 17253728;

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dir = path.join(root, "resources");
const dest = path.join(dir, "onnxruntime.dll");
// Inside the package (checked by its hash) -> beside the library.
const TEXTS = {
  LICENSE: "onnxruntime-LICENSE.txt",
  "ThirdPartyNotices.txt": "onnxruntime-ThirdPartyNotices.txt",
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function checked(bytes, what) {
  const got = sha256(bytes);
  if (bytes.length !== LIBRARY_BYTES || got !== LIBRARY_SHA256) {
    throw new Error(
      `${what} is not ONNX Runtime ${VERSION} for Windows x64 ` +
        `(${bytes.length} bytes, SHA-256 ${got}; expected ${LIBRARY_BYTES} bytes, ${LIBRARY_SHA256}).`,
    );
  }
  return bytes;
}

// Reads one file out of a zip (a .nupkg is a zip) through its central
// directory. Only "stored" and "deflate" entries, which is all NuGet makes.
function unzipEntry(zip, name) {
  let eocd = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 65557); i--) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("the package is not a zip file");
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let n = 0; n < count; n++) {
    if (zip.readUInt32LE(at) !== 0x02014b50) throw new Error("the package's zip directory is damaged");
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extraLen = zip.readUInt16LE(at + 30);
    const commentLen = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const entry = zip.toString("utf8", at + 46, at + 46 + nameLen);
    if (entry === name) {
      const dataAt = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(dataAt, dataAt + size);
      if (method === 0) return Buffer.from(data);
      if (method === 8) return zlib.inflateRawSync(data);
      throw new Error(`${name} uses zip method ${method}, which this script cannot read`);
    }
    at += 46 + nameLen + extraLen + commentLen;
  }
  throw new Error(`the package has no ${name}`);
}

async function main() {
  if (process.argv.length > 2) {
    console.error("Usage: node scripts/fetch-ort.mjs");
    process.exit(2);
  }

  const texts = Object.values(TEXTS).map((f) => path.join(dir, f));
  if (fs.existsSync(dest) && texts.every((f) => fs.existsSync(f))) {
    try {
      checked(fs.readFileSync(dest), dest);
      console.log(`ONNX Runtime ${VERSION} is already at ${dest}`);
      return;
    } catch {
      // A different file: fetch it again.
    }
  }

  console.log(`Downloading ${PACKAGE_URL}`);
  const res = await fetch(PACKAGE_URL);
  if (!res.ok) throw new Error(`the download failed: HTTP ${res.status}`);
  const pkg = Buffer.from(await res.arrayBuffer());
  const got = sha256(pkg);
  if (got !== PACKAGE_SHA256) {
    throw new Error(`the downloaded package has SHA-256 ${got}; expected ${PACKAGE_SHA256}`);
  }
  const library = checked(unzipEntry(pkg, ENTRY), ENTRY);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(dest, library);
  for (const [entry, file] of Object.entries(TEXTS)) {
    fs.writeFileSync(path.join(dir, file), unzipEntry(pkg, entry));
  }
  console.log(`ONNX Runtime ${VERSION} is at ${dest}`);
}

main().catch((e) => {
  console.error(`fetch-ort: ${e.message}`);
  process.exit(1);
});
