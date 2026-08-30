#!/usr/bin/env node
// Downloads a pinned release of the real sing-box binary (SagerNet/sing-box)
// for the current platform/arch and stages it at
// `src-tauri/resources/singbox/sing-box[.exe]`, so `bundle.resources` can
// package it into the installer.
//
// This closes the single most severe gap found in this whole project: the
// sing-box CORE binary itself -- the thing this entire app exists to run --
// was never actually bundled with any installer. `core-manager`'s binary
// discovery only ever checked an env var override, a `.dev-bin/` dev
// convenience path, or a bare name relying on `$PATH`, so on a real user's
// machine "start proxy" always failed with "program not found". Mirrors the
// exact same "gitignored, fetched by a script that must run before any
// cargo command" pattern already established for `resources/dashboard/`
// (`fetch-dashboard.mjs`) and `resources/helper/` (`build-helper.mjs`), for
// the exact same reason: Tauri's build.rs validates every `bundle.resources`
// source path at *compile* time, not just at bundle time.
//
// Version is pinned (not "latest") so a config shape this app generates is
// always tested against the exact sing-box binary it ships -- this project
// has been bitten before by a sing-box release removing/changing a config
// shape (the `wireguard` outbound removed in 1.13.0) that only a real
// `sing-box check` run against the actual installed binary caught, not
// assumptions from documentation. Bumping SING_BOX_VERSION requires
// re-verifying the full `cargo test -- --ignored` real-binary suite in
// crates/core-manager against the new version first.
//
// Each archive's sha256 is pinned too (computed directly from GitHub's own
// release asset at pin time) so a compromised or corrupted download is
// caught rather than silently bundled into every user's installer -- this
// binary runs with real network/system access, unlike the dashboard's
// static assets, so it gets this extra rigor `fetch-dashboard.mjs` doesn't
// need.
//
// Usage: node scripts/fetch-singbox.mjs
// (wired up as `npm run fetch:singbox`; must run before any `cargo check`/
// `cargo build`/`cargo test`/`tauri dev`/`tauri build` on a fresh checkout,
// same as `fetch:dashboard` and `build:helper`.)

import { createHash } from "node:crypto";
import { chmod, mkdir, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync, inflateRawSync } from "node:zlib";

const SING_BOX_VERSION = "1.13.19";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, "..");
const OUT_DIR = join(REPO_ROOT, "src-tauri", "resources", "singbox");

// asset name (relative to the GitHub release) -> sha256 of that exact file,
// pinned by downloading it directly from
// https://github.com/SagerNet/sing-box/releases/download/v${SING_BOX_VERSION}/<asset>
// and hashing it. Re-pin both together when bumping SING_BOX_VERSION.
const ASSETS = {
    "win32-x64": {
        asset: `sing-box-${SING_BOX_VERSION}-windows-amd64.zip`,
        sha256: "e011a4def2f5e2b143ed54adb2b1a20a6be407806ab4442f3667f1dd817a2c8d",
        kind: "zip",
        binaryName: "sing-box.exe",
    },
    "darwin-x64": {
        asset: `sing-box-${SING_BOX_VERSION}-darwin-amd64.tar.gz`,
        sha256: "31ee722237d95774e101fbffeae6be6776249c5f7db229ad8ff00b45b22e6a00",
        kind: "tar.gz",
        binaryName: "sing-box",
    },
    "darwin-arm64": {
        asset: `sing-box-${SING_BOX_VERSION}-darwin-arm64.tar.gz`,
        sha256: "23bf191906f2dfc9f00e9f0092f274f3426ba9377327e903ff94e636b64d0997",
        kind: "tar.gz",
        binaryName: "sing-box",
    },
    "linux-x64": {
        asset: `sing-box-${SING_BOX_VERSION}-linux-amd64.tar.gz`,
        sha256: "ef88a9e577d474210867bd708933d042e9b70106529df2656182c9db90106aa1",
        kind: "tar.gz",
        binaryName: "sing-box",
    },
};

async function main() {
    const key = `${process.platform}-${process.arch}`;
    const target = ASSETS[key];
    if (!target) {
        throw new Error(
            `no pinned sing-box asset for platform-arch '${key}' -- supported: ${Object.keys(ASSETS).join(", ")}`,
        );
    }

    const url = `https://github.com/SagerNet/sing-box/releases/download/v${SING_BOX_VERSION}/${target.asset}`;
    console.log(`Fetching ${url} ...`);
    const response = await fetch(url);
    if (!response.ok) {
        throw new Error(`download failed: HTTP ${response.status} ${response.statusText}`);
    }
    const archiveBuffer = Buffer.from(await response.arrayBuffer());
    console.log(`Downloaded ${archiveBuffer.length} bytes; verifying checksum...`);

    const actualSha256 = createHash("sha256").update(archiveBuffer).digest("hex");
    if (actualSha256 !== target.sha256) {
        throw new Error(
            `sha256 mismatch for ${target.asset}: expected ${target.sha256}, got ${actualSha256} -- ` +
                `refusing to stage a sing-box binary that doesn't match its pinned checksum`,
        );
    }

    const binaryData =
        target.kind === "zip"
            ? extractFromZip(archiveBuffer, target.binaryName)
            : extractFromTarGz(archiveBuffer, target.binaryName);

    await rm(OUT_DIR, { recursive: true, force: true });
    await mkdir(OUT_DIR, { recursive: true });
    const destPath = join(OUT_DIR, target.binaryName);
    await writeFile(destPath, binaryData);
    if (process.platform !== "win32") {
        await chmod(destPath, 0o755);
    }
    console.log(`Staged ${destPath} (${binaryData.length} bytes)`);
}

/** Same minimal ZIP reader as `fetch-dashboard.mjs` (stored/deflate only,
 * central-directory-driven) -- see that script for the full explanation of
 * why this is hand-rolled instead of a dependency. Returns just the single
 * named entry's decompressed data, searching without the archive's
 * top-level folder prefix (unknown/irrelevant here, unlike
 * `fetch-dashboard.mjs`, since this only needs the one file). */
function extractFromZip(buffer, binaryName) {
    const eocdOffset = findEndOfCentralDirectory(buffer);
    const entryCount = buffer.readUInt16LE(eocdOffset + 10);
    const centralDirOffset = buffer.readUInt32LE(eocdOffset + 16);

    let offset = centralDirOffset;
    for (let i = 0; i < entryCount; i++) {
        const signature = buffer.readUInt32LE(offset);
        if (signature !== 0x02014b50) {
            throw new Error(`malformed zip: expected central directory signature at offset ${offset}`);
        }
        const compressionMethod = buffer.readUInt16LE(offset + 10);
        const compressedSize = buffer.readUInt32LE(offset + 20);
        const nameLength = buffer.readUInt16LE(offset + 28);
        const extraLength = buffer.readUInt16LE(offset + 30);
        const commentLength = buffer.readUInt16LE(offset + 32);
        const localHeaderOffset = buffer.readUInt32LE(offset + 42);
        const nameStart = offset + 46;
        const name = buffer.toString("utf8", nameStart, nameStart + nameLength);

        if (name.endsWith(binaryName)) {
            return readZipEntryData(buffer, localHeaderOffset, compressedSize, compressionMethod, name);
        }
        offset = nameStart + nameLength + extraLength + commentLength;
    }
    throw new Error(`'${binaryName}' not found in zip`);
}

function readZipEntryData(buffer, localHeaderOffset, compressedSize, compressionMethod, name) {
    const signature = buffer.readUInt32LE(localHeaderOffset);
    if (signature !== 0x04034b50) {
        throw new Error(`malformed zip: expected local file header signature for '${name}'`);
    }
    const nameLength = buffer.readUInt16LE(localHeaderOffset + 26);
    const extraLength = buffer.readUInt16LE(localHeaderOffset + 28);
    const dataStart = localHeaderOffset + 30 + nameLength + extraLength;
    const compressed = buffer.subarray(dataStart, dataStart + compressedSize);

    if (compressionMethod === 0) return compressed;
    if (compressionMethod === 8) return inflateRawSync(compressed);
    throw new Error(`unsupported zip compression method ${compressionMethod} for '${name}'`);
}

function findEndOfCentralDirectory(buffer) {
    const signature = 0x06054b50;
    const minOffset = Math.max(0, buffer.length - 65536 - 22);
    for (let i = buffer.length - 22; i >= minOffset; i--) {
        if (buffer.readUInt32LE(i) === signature) return i;
    }
    throw new Error("malformed zip: could not find End Of Central Directory record");
}

/** Minimal (uncompressed, post-gunzip) USTAR reader: fixed 512-byte header
 * blocks, a NUL-terminated name in the first 100 bytes, an octal size field
 * at offset 124..136, file data padded up to the next 512-byte boundary.
 * Covers every real-world GNU/POSIX tar, including sing-box's release
 * archives (confirmed against actual downloaded assets, not just the
 * format spec) -- no long-name (prefix/GNU-longname) entries here since
 * sing-box's archives only have three flat top-level files. */
function extractFromTarGz(gzBuffer, binaryName) {
    const tar = gunzipSync(gzBuffer);
    let offset = 0;
    while (offset + 512 <= tar.length) {
        const header = tar.subarray(offset, offset + 512);
        if (header.every((b) => b === 0)) break; // end-of-archive marker

        const name = header.toString("utf8", 0, 100).replace(/\0.*$/, "");
        const sizeOctal = header.toString("utf8", 124, 136).replace(/\0.*$/, "").trim();
        const size = parseInt(sizeOctal, 8) || 0;
        const dataStart = offset + 512;

        if (name.endsWith(binaryName) && size > 0) {
            return tar.subarray(dataStart, dataStart + size);
        }

        offset = dataStart + Math.ceil(size / 512) * 512;
    }
    throw new Error(`'${binaryName}' not found in tar`);
}

main().catch((err) => {
    console.error(`fetch-singbox failed: ${err.message}`);
    process.exit(1);
});
