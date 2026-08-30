#!/usr/bin/env node
// Builds this platform's privileged-helper binary (`crates/helper-windows`,
// `helper-macos`, or `helper-linux`) in release mode and stages it at
// `src-tauri/resources/helper/<binary-name>` so `tauri.conf.json`'s
// `bundle.resources` can package it into the installer.
//
// This has to be a separate step run *before* any `cargo check`/`cargo
// build`/`tauri dev`/`tauri build` of the `ferroflow` package, not just
// before bundling: Tauri's build.rs validates every `bundle.resources`
// source path at compile time, not just at bundle time (see the
// `fetch-dashboard.mjs` step this mirrors, and `src-tauri/resources/webview2/`
// in git history for the same lesson learned the hard way). The helper
// binary is its own separate `[[bin]]` in a workspace crate that `cargo
// build -p ferroflow` (what `tauri build`/`tauri dev` actually invoke) does
// NOT build as a side effect, so without this script the resources path
// would flat-out not exist on a fresh checkout.
//
// Usage: node scripts/build-helper.mjs
// (wired up as `npm run build:helper`; must run before `npm run tauri dev`
// or `npm run tauri build`, and before any `cargo check`/`cargo build`/
// `cargo test` of the workspace on a fresh checkout.)

import { copyFile, mkdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, "..");
const OUT_DIR = join(REPO_ROOT, "src-tauri", "resources", "helper");

const PLATFORM_CRATES = {
    win32: { crate: "helper-windows", binName: "ferroflow-helper-windows.exe" },
    darwin: { crate: "helper-macos", binName: "ferroflow-helper-macos" },
    linux: { crate: "helper-linux", binName: "ferroflow-helper-linux" },
};

function main() {
    const target = PLATFORM_CRATES[process.platform];
    if (!target) {
        throw new Error(`no privileged-helper crate mapped for process.platform=${process.platform}`);
    }
    const { crate, binName } = target;

    console.log(`building ${crate} (release)...`);
    const build = spawnSync("cargo", ["build", "--release", "-p", crate], {
        cwd: REPO_ROOT,
        stdio: "inherit",
    });
    if (build.status !== 0) {
        throw new Error(`cargo build -p ${crate} failed with exit code ${build.status}`);
    }

    const builtPath = join(REPO_ROOT, "target", "release", binName);
    if (!existsSync(builtPath)) {
        throw new Error(`expected build output at ${builtPath} but it doesn't exist`);
    }

    return mkdir(OUT_DIR, { recursive: true }).then(async () => {
        const destPath = join(OUT_DIR, binName);
        await copyFile(builtPath, destPath);
        console.log(`staged ${destPath}`);
    });
}

main().catch((err) => {
    console.error(`build-helper failed: ${err.message}`);
    process.exit(1);
});
