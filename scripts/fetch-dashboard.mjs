#!/usr/bin/env node
// Downloads SagerNet/sing-box-dashboard's built static assets (the `gh-pages`
// branch -- that branch *is* the Vite build output, not source) and extracts
// them into `src-tauri/resources/dashboard/`, stripping the zip's top-level
// `sing-box-dashboard-gh-pages/` folder so `index.html` lands directly at
// `src-tauri/resources/dashboard/index.html`.
//
// Mirrors `crates/core-manager`'s `.dev-bin/`-style "fetched dev/build
// dependency, gitignored, not committed" convention -- see
// `src-tauri/resources/dashboard/` in `.gitignore`.
//
// Zero-dependency by design: this is a project setup script, not app code,
// so it deliberately avoids adding a `yauzl`/`extract-zip`-style npm
// dependency (or shelling out to a platform `tar`/`unzip`, whose ZIP support
// varies -- GNU tar can't extract ZIP at all, and which `tar.exe` a given
// Windows PATH resolves to is not guaranteed) in favor of a small self-
// contained ZIP reader built on Node's built-in `zlib` (raw DEFLATE is
// exactly what `zlib.inflateRawSync` does, and "stored"/method-0 entries
// need no decompression at all -- those two methods cover every real-world
// ZIP, including GitHub's codeload archives).
//
// Usage: node scripts/fetch-dashboard.mjs
// (wired up as `npm run fetch:dashboard`)

import { mkdir, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateRawSync } from "node:zlib";

const ZIP_URL = "https://codeload.github.com/SagerNet/sing-box-dashboard/zip/refs/heads/gh-pages";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, "..");
const OUT_DIR = join(REPO_ROOT, "src-tauri", "resources", "dashboard");

// The zip's single top-level folder (`<repo>-<branch>/`, GitHub's codeload
// convention) that gets stripped so `index.html` ends up directly under
// `OUT_DIR` rather than one level down.
const STRIP_PREFIX = "sing-box-dashboard-gh-pages/";

async function main() {
    console.log(`Fetching ${ZIP_URL} ...`);
    const response = await fetch(ZIP_URL);
    if (!response.ok) {
        throw new Error(`download failed: HTTP ${response.status} ${response.statusText}`);
    }
    const zipBuffer = Buffer.from(await response.arrayBuffer());
    console.log(`Downloaded ${zipBuffer.length} bytes; extracting...`);

    const entries = readZipEntries(zipBuffer);
    if (entries.length === 0) {
        throw new Error("zip contained no entries -- refusing to wipe an existing dashboard/ with nothing");
    }

    // Clean slate: a stale previous fetch's files (e.g. from an older
    // dashboard version with differently-hashed asset filenames) should
    // never linger alongside the new ones.
    await rm(OUT_DIR, { recursive: true, force: true });
    await mkdir(OUT_DIR, { recursive: true });

    let fileCount = 0;
    for (const entry of entries) {
        if (entry.isDirectory) continue;

        let relativePath = entry.name;
        if (relativePath.startsWith(STRIP_PREFIX)) {
            relativePath = relativePath.slice(STRIP_PREFIX.length);
        }
        if (relativePath === "") continue;

        const destPath = join(OUT_DIR, relativePath);
        await mkdir(dirname(destPath), { recursive: true });
        await writeFile(destPath, entry.data);
        fileCount += 1;
    }

    console.log(`Extracted ${fileCount} files into ${OUT_DIR}`);

    const indexPath = join(OUT_DIR, "index.html");
    const { existsSync } = await import("node:fs");
    if (!existsSync(indexPath)) {
        throw new Error(`expected ${indexPath} to exist after extraction, but it does not`);
    }
    console.log(`OK: ${indexPath} present.`);
}

/**
 * Minimal ZIP reader: locates the End Of Central Directory record, walks the
 * central directory for each entry's metadata (name, compression method,
 * compressed/uncompressed sizes -- these are always trustworthy in the
 * central directory, unlike the local header when the "data descriptor"
 * general-purpose bit is set), then reads each entry's actual file data by
 * seeking to its local header (only to compute where the data starts -- the
 * local header's own name/extra-field lengths can differ slightly from the
 * central directory's) and decompressing exactly `compressedSize` bytes.
 *
 * Supports compression method 0 (stored) and 8 (deflate) -- every entry in a
 * GitHub codeload zip uses one of these two.
 */
function readZipEntries(buffer) {
    const eocdOffset = findEndOfCentralDirectory(buffer);
    const entryCount = buffer.readUInt16LE(eocdOffset + 10);
    const centralDirOffset = buffer.readUInt32LE(eocdOffset + 16);

    const entries = [];
    let offset = centralDirOffset;
    for (let i = 0; i < entryCount; i++) {
        const signature = buffer.readUInt32LE(offset);
        if (signature !== 0x02014b50) {
            throw new Error(`malformed zip: expected central directory signature at offset ${offset}`);
        }

        const compressionMethod = buffer.readUInt16LE(offset + 10);
        const compressedSize = buffer.readUInt32LE(offset + 20);
        const uncompressedSize = buffer.readUInt32LE(offset + 24);
        const nameLength = buffer.readUInt16LE(offset + 28);
        const extraLength = buffer.readUInt16LE(offset + 30);
        const commentLength = buffer.readUInt16LE(offset + 32);
        const externalAttrs = buffer.readUInt32LE(offset + 38);
        const localHeaderOffset = buffer.readUInt32LE(offset + 42);

        const nameStart = offset + 46;
        const name = buffer.toString("utf8", nameStart, nameStart + nameLength);

        // MS-DOS directory attribute bit, or (more reliably here, since
        // these are Unix-made zips) a trailing slash on the name.
        const isDirectory = name.endsWith("/") || (externalAttrs & 0x10) !== 0;

        entries.push({
            name,
            isDirectory,
            compressionMethod,
            compressedSize,
            uncompressedSize,
            localHeaderOffset,
        });

        offset = nameStart + nameLength + extraLength + commentLength;
    }

    for (const entry of entries) {
        if (entry.isDirectory) continue;
        entry.data = readEntryData(buffer, entry);
    }

    return entries;
}

function readEntryData(buffer, entry) {
    const local = entry.localHeaderOffset;
    const signature = buffer.readUInt32LE(local);
    if (signature !== 0x04034b50) {
        throw new Error(`malformed zip: expected local file header signature for '${entry.name}'`);
    }
    const nameLength = buffer.readUInt16LE(local + 26);
    const extraLength = buffer.readUInt16LE(local + 28);
    const dataStart = local + 30 + nameLength + extraLength;
    const compressed = buffer.subarray(dataStart, dataStart + entry.compressedSize);

    if (entry.compressionMethod === 0) {
        return compressed;
    }
    if (entry.compressionMethod === 8) {
        return inflateRawSync(compressed);
    }
    throw new Error(
        `unsupported zip compression method ${entry.compressionMethod} for '${entry.name}' ` +
            `(only stored/deflate are supported)`,
    );
}

/** Scans backward from the end of the buffer for the EOCD signature
 * (0x06054b50). It can't just be at a fixed offset from the end because the
 * trailing zip comment field has a variable, attacker/tool-controlled
 * length -- though GitHub's codeload zips don't set one, searching properly
 * costs nothing and is correct for any zip. */
function findEndOfCentralDirectory(buffer) {
    const signature = 0x06054b50;
    const minOffset = Math.max(0, buffer.length - 65536 - 22);
    for (let i = buffer.length - 22; i >= minOffset; i--) {
        if (buffer.readUInt32LE(i) === signature) {
            return i;
        }
    }
    throw new Error("malformed zip: could not find End Of Central Directory record");
}

main().catch((err) => {
    console.error(`fetch-dashboard failed: ${err.message}`);
    process.exit(1);
});
