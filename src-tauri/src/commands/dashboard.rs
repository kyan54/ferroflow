//! Opens SagerNet/sing-box-dashboard's official web UI (sing-box's own
//! monitoring/connections dashboard, not this app's simpler Connections tab
//! -- see `docs/ipc-contract.md`'s "Live connections" section) in a second
//! window, pointed at the currently running sing-box's Clash API
//! (`experimental.clash_api.external_controller` -- see
//! `core_manager::clash_api`). Mirrors the sibling Electron app's behavior
//! of bundling the same dashboard's built assets and opening them in a
//! second `BrowserWindow` pre-wired to the local Clash API.
//!
//! The dashboard's static build is fetched separately by
//! `scripts/fetch-dashboard.mjs` (`npm run fetch:dashboard`) into
//! `src-tauri/resources/dashboard/` -- see that script's module doc comment
//! and `.gitignore` for why it's not committed.
//!
//! **Known upstream limitation (verified against a real sing-box, not
//! assumed):** the dashboard's current `gh-pages` build talks exclusively
//! gRPC-Web to a `daemon.StartedService`, not the classic Clash REST API --
//! there is no REST fallback, including for the basic overview view. That
//! service isn't among sing-box's documented `experimental.clash_api`
//! fields and isn't served by `sing-box run` on either the current stable
//! release or the newest available prerelease as of this writing (both
//! tested locally). So the window this command opens will currently show a
//! "connection failed" state regardless of how correct the wiring below
//! is -- confirmed by opening the fetched `index.html` directly and
//! checking its "Edit server" dialog, which does show this exact address
//! read back correctly. See `docs/ipc-contract.md`'s "sing-box dashboard"
//! section for the full investigation; this module's job (getting the
//! right address to the dashboard) is done and verified, the remaining gap
//! is upstream.
//!
//! ## How the dashboard is told which Clash API to talk to
//!
//! This fork of the dashboard (`SagerNet/sing-box-dashboard`, `gh-pages`
//! branch -- the branch itself *is* the Vite build output) takes **no URL
//! query parameters at all**: its minified bundle
//! (`assets/index-*.js`) contains no reference to `URLSearchParams` or
//! `location.search` anywhere. Confirmed by fetching the real gh-pages zip
//! and grepping the bundle directly, not by assuming based on other
//! Clash-API dashboards (Yacd/Razord/metacubexd all differ from this one).
//!
//! Instead, its entire server list lives in `localStorage`, under the key
//! `sing-box-dashboard.servers`, as JSON shaped like:
//!
//! ```json
//! { "servers": [{ "id": "...", "name": "...", "url": "...", "secret": "..." }],
//!   "activeId": "..." }
//! ```
//!
//! (There's also a legacy singular `sing-box-dashboard.server` key --
//! `{ "url": ..., "secret": ... }` -- that the app reads and deletes
//! exactly once, on first load, purely to migrate an older single-server
//! install into the modern list. That path is **not** what this module
//! uses: it only fires when `sing-box-dashboard.servers` has never been set
//! at all, so relying on it would mean a *second* dashboard open -- after
//! the app has already persisted a `servers` list once -- would silently
//! keep using whatever port a *previous* sing-box run happened to have,
//! since `clash_api_port` is a fresh ephemeral port every `core_manager`
//! `start()` call.)
//!
//! So this module writes directly to the modern `sing-box-dashboard.servers`
//! key on every window open, via `WebviewWindowBuilder::initialization_script`
//! (runs before any of the page's own scripts, on every navigation -- see
//! that method's docs). The script upserts one entry keyed by a fixed id
//! (`ferroflow-local`) with this run's current `http://127.0.0.1:<port>`
//! and empty `secret` (sing-box's Clash API here is started with no secret
//! configured -- loopback-only, see `docs/ipc-contract.md`'s "No auth" note
//! under "Live connections"), preserving any other servers the user may
//! have added by hand, and always sets `activeId` to it -- guaranteeing the
//! dashboard connects to *this* run's Clash API immediately on open, rather
//! than requiring the user to pick it from a list or landing on a stale
//! entry from a previous run.

use std::path::PathBuf;

use shared_types::{AppError, AppResult};
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

/// Dev-only override, checked first -- mirrors `core-manager`'s
/// `FERROFLOW_SINGBOX_PATH`/`locate_binary` and
/// `commands::helper_windows`'s `FERROFLOW_HELPER_PATH`/
/// `locate_helper_binary` conventions. Points at the *directory* containing
/// `index.html` (the dashboard is a folder of static assets, not a single
/// binary), so `index.html` is joined on before use.
const DASHBOARD_PATH_ENV: &str = "FERROFLOW_DASHBOARD_PATH";

const DASHBOARD_WINDOW_LABEL: &str = "dashboard";

/// `localStorage` key the dashboard's own bundle reads its server list from
/// -- see this module's doc comment for how that was determined.
const DASHBOARD_STORAGE_KEY: &str = "sing-box-dashboard.servers";

/// Fixed id for the entry this module upserts into the dashboard's server
/// list, so re-opening the dashboard (or restarting the proxy, which picks
/// a fresh Clash API port) updates the same entry rather than accumulating
/// a new one every time.
const DASHBOARD_SERVER_ID: &str = "ferroflow-local";

/// Opens the sing-box dashboard window, or focuses it if already open.
/// Fails with `proxy_not_running` if sing-box isn't currently running (no
/// Clash API to point the dashboard at), and `dashboard_missing` if the
/// fetched dashboard assets can't be found by any of the three discovery
/// tiers (see `locate_dashboard_index`) -- most likely because
/// `npm run fetch:dashboard` was never run.
#[tauri::command]
pub async fn dashboard_open(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let port = state.core_manager.current_clash_api_port().await.ok_or_else(|| {
        AppError::new("proxy_not_running", "start the proxy before opening the dashboard")
    })?;

    if let Some(window) = app.get_webview_window(DASHBOARD_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let index_path = locate_dashboard_index(&app).map_err(|tried| {
        AppError::new(
            "dashboard_missing",
            format!(
                "could not find the sing-box dashboard's index.html; tried: {}. \
                 Run `npm run fetch:dashboard` first.",
                tried.join(", ")
            ),
        )
    })?;

    let url = Url::from_file_path(&index_path).map_err(|_| {
        AppError::new(
            "dashboard_missing",
            format!(
                "dashboard index.html path could not be converted to a file:// URL: {}",
                index_path.display()
            ),
        )
    })?;

    WebviewWindowBuilder::new(&app, DASHBOARD_WINDOW_LABEL, WebviewUrl::External(url))
        .title("sing-box dashboard")
        .inner_size(1100.0, 760.0)
        .initialization_script(seed_dashboard_storage_script(port))
        .build()
        .map_err(|e| {
            AppError::new("dashboard_open_failed", format!("failed to open dashboard window: {e}"))
        })?;

    Ok(())
}

/// Builds the JS run via `initialization_script` (before any page script,
/// on every navigation -- see module doc comment) that upserts this run's
/// local Clash API into the dashboard's `sing-box-dashboard.servers`
/// `localStorage` entry and marks it active. Values are JSON-serialized
/// (via `serde_json`) rather than string-interpolated as raw JS literals,
/// so nothing here needs hand-rolled JS-string escaping.
fn seed_dashboard_storage_script(clash_api_port: u16) -> String {
    let entry = serde_json::json!({
        "id": DASHBOARD_SERVER_ID,
        "name": "FerroFlow (local)",
        "url": format!("http://127.0.0.1:{clash_api_port}"),
        "secret": "",
    });
    // `serde_json::to_string` on a `&str`/String never fails, and
    // `Value::to_string()` for a `json!` object of plain strings/numbers
    // likewise can't fail -- `.unwrap()` here is just avoiding a `Result`
    // this function has no meaningful way to propagate anyway (this is
    // JS-source generation, not I/O).
    let key_json = serde_json::to_string(DASHBOARD_STORAGE_KEY).unwrap();
    let id_json = serde_json::to_string(DASHBOARD_SERVER_ID).unwrap();
    let entry_json = entry.to_string();

    format!(
        r#"(function() {{
  try {{
    var KEY = {key_json};
    var ID = {id_json};
    var entry = {entry_json};
    var raw = localStorage.getItem(KEY);
    var parsed = null;
    try {{ parsed = raw ? JSON.parse(raw) : null; }} catch (e) {{ parsed = null; }}
    var servers = (parsed && Array.isArray(parsed.servers)) ? parsed.servers : [];
    var idx = -1;
    for (var i = 0; i < servers.length; i++) {{
      if (servers[i] && servers[i].id === ID) {{ idx = i; break; }}
    }}
    if (idx >= 0) {{ servers[idx] = entry; }} else {{ servers.push(entry); }}
    localStorage.setItem(KEY, JSON.stringify({{ servers: servers, activeId: ID }}));
  }} catch (e) {{
    // Best-effort: if localStorage is unavailable for some reason, the
    // dashboard just falls back to its own default (empty/local) server,
    // same as a first-time visitor with no FerroFlow integration at all.
  }}
}})();"#
    )
}

/// Discovery order for the dashboard's `index.html`, mirroring
/// `commands::helper_windows::locate_helper_binary`'s three tiers: env var
/// override, then a dev-convenience path, then Tauri's bundled-resource
/// directory (packaged case -- see `tauri.conf.json`'s
/// `bundle.resources`). The dev-convenience tier is anchored via
/// `CARGO_MANIFEST_DIR` (this crate's own directory, `src-tauri/`) rather
/// than a plain relative path off the process's current working directory:
/// unlike the `.dev-bin`/`FERROFLOW_HELPER_PATH` convention elsewhere in
/// this codebase (which assumes the app is launched with the repo root as
/// cwd), the dashboard's fetched assets live under `src-tauri/resources/`
/// specifically -- the same location `tauri.conf.json` bundles from -- so
/// anchoring to the crate directory at compile time is both more precise
/// and correct regardless of the launching shell's cwd.
///
/// Returns the list of paths actually tried, in order, on failure so the
/// caller can build a useful error message.
fn locate_dashboard_index(app: &AppHandle) -> Result<PathBuf, Vec<String>> {
    let mut tried = Vec::new();

    if let Ok(dir) = std::env::var(DASHBOARD_PATH_ENV) {
        if !dir.is_empty() {
            let candidate = PathBuf::from(&dir).join("index.html");
            if candidate.is_file() {
                return Ok(candidate);
            }
            tried.push(candidate.display().to_string());
        }
    }

    let dev_candidate =
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/dashboard")).join("index.html");
    if dev_candidate.is_file() {
        return Ok(dev_candidate);
    }
    tried.push(dev_candidate.display().to_string());

    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("dashboard").join("index.html");
        if candidate.is_file() {
            return Ok(candidate);
        }
        tried.push(candidate.display().to_string());
    }

    Err(tried)
}
