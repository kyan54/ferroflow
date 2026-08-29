import { useEffect } from "react";
import { useAppStore } from "../store";
import { PROXY_MODES, PROXY_MODE_TYPES } from "../types";
import type { ProxyMode, ProxyModeType } from "../types";

export function SettingsView() {
  const config = useAppStore((s) => s.config);
  const platformInfo = useAppStore((s) => s.platformInfo);
  const refreshPlatformInfo = useAppStore((s) => s.refreshPlatformInfo);
  const saveConfig = useAppStore((s) => s.saveConfig);
  const helperStatus = useAppStore((s) => s.helperStatus);
  const helperBusy = useAppStore((s) => s.helperBusy);
  const refreshHelperStatus = useAppStore((s) => s.refreshHelperStatus);
  const installHelper = useAppStore((s) => s.installHelper);
  const uninstallHelper = useAppStore((s) => s.uninstallHelper);
  const exportBackup = useAppStore((s) => s.exportBackup);
  const importBackup = useAppStore((s) => s.importBackup);
  const exportDiagnostic = useAppStore((s) => s.exportDiagnostic);

  useEffect(() => {
    refreshPlatformInfo();
    refreshHelperStatus();
  }, [refreshPlatformInfo, refreshHelperStatus]);

  if (!config) {
    return <div className="p-6 text-sm text-slate-500 dark:text-slate-400">Loading…</div>;
  }

  function toggle(key: "autoStart" | "minimizeToTray" | "connectionHistoryEnabled") {
    if (!config) return;
    saveConfig({ ...config, [key]: !config[key] });
  }

  function setProxyMode(mode: ProxyMode) {
    if (!config) return;
    saveConfig({ ...config, proxyMode: mode });
  }

  function setProxyModeType(modeType: ProxyModeType) {
    if (!config) return;
    saveConfig({ ...config, proxyModeType: modeType });
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-6 p-6">
      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <h2 className="text-lg font-semibold">Platform</h2>
        {platformInfo ? (
          <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
            <dt className="text-slate-500 dark:text-slate-400">OS</dt>
            <dd>{platformInfo.platform}</dd>
            <dt className="text-slate-500 dark:text-slate-400">Architecture</dt>
            <dd>{platformInfo.arch}</dd>
            <dt className="text-slate-500 dark:text-slate-400">OS version</dt>
            <dd>{platformInfo.osVersion || "—"}</dd>
            <dt className="text-slate-500 dark:text-slate-400">Running as admin</dt>
            <dd>{platformInfo.isAdmin ? "Yes" : "No"}</dd>
          </dl>
        ) : (
          <p className="mt-4 text-sm text-slate-500 dark:text-slate-400">Loading…</p>
        )}
      </section>

      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <h2 className="text-lg font-semibold">Behavior</h2>

        <label className="mt-4 flex items-center justify-between text-sm">
          <span>Start automatically on login</span>
          <input
            type="checkbox"
            checked={config.autoStart}
            onChange={() => toggle("autoStart")}
          />
        </label>

        <label className="mt-3 flex items-center justify-between text-sm">
          <span>Minimize to tray on close</span>
          <input
            type="checkbox"
            checked={config.minimizeToTray}
            onChange={() => toggle("minimizeToTray")}
          />
        </label>

        <label className="mt-3 flex items-center justify-between text-sm">
          <span>Record connection history</span>
          <input
            type="checkbox"
            checked={config.connectionHistoryEnabled}
            onChange={() => toggle("connectionHistoryEnabled")}
          />
        </label>
        <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
          Off by default. Only applies the next time the proxy starts -- toggling this while
          already connected does not retroactively record the current session. Recorded locally as
          plain, unencrypted JSON, capped at the most recent 1000 finished connections.
        </p>

        <div className="mt-4">
          <label className="flex flex-col gap-1 text-sm">
            Proxy mode
            <select
              value={config.proxyMode}
              onChange={(e) => setProxyMode(e.target.value as ProxyMode)}
              className="w-48 rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
            >
              {PROXY_MODES.map((mode) => (
                <option key={mode} value={mode}>
                  {mode}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="mt-4">
          <label className="flex flex-col gap-1 text-sm">
            Takeover mode
            <select
              value={config.proxyModeType}
              onChange={(e) => setProxyModeType(e.target.value as ProxyModeType)}
              className="w-48 rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
            >
              {PROXY_MODE_TYPES.map((modeType) => (
                <option key={modeType} value={modeType}>
                  {modeType}
                </option>
              ))}
            </select>
          </label>
          {config.proxyModeType === "tun" && !helperStatus?.ready && (
            <p className="mt-2 text-sm text-amber-600 dark:text-amber-400">
              TUN mode needs the privileged helper installed (see below) — starting the proxy
              without it will fail.
            </p>
          )}
        </div>
      </section>

      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <h2 className="text-lg font-semibold">Privileged helper</h2>
        <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
          Required for TUN mode. Installed once with a single admin prompt; after that, starting
          and stopping the proxy never prompts again.
        </p>

        {helperStatus ? (
          <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
            <dt className="text-slate-500 dark:text-slate-400">Status</dt>
            <dd>{helperStatus.ready ? "Installed and running" : "Not installed"}</dd>
            {helperStatus.version && (
              <>
                <dt className="text-slate-500 dark:text-slate-400">Version</dt>
                <dd>{helperStatus.version}</dd>
              </>
            )}
          </dl>
        ) : (
          <p className="mt-4 text-sm text-slate-500 dark:text-slate-400">Checking…</p>
        )}

        <div className="mt-4 flex gap-2">
          {!helperStatus?.ready ? (
            <button
              onClick={() => installHelper()}
              disabled={helperBusy}
              className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
            >
              {helperBusy ? "Installing…" : "Install helper"}
            </button>
          ) : (
            <button
              onClick={() => uninstallHelper()}
              disabled={helperBusy}
              className="rounded-md px-4 py-2 text-sm font-medium text-red-600 hover:bg-red-50 disabled:opacity-50 dark:text-red-400 dark:hover:bg-red-950"
            >
              {helperBusy ? "Removing…" : "Remove helper"}
            </button>
          )}
        </div>
      </section>

      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <h2 className="text-lg font-semibold">Backup & diagnostics</h2>
        <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
          Back up your servers, rules, and settings to a file you can move to another machine, or
          export a redacted diagnostic report safe to paste into a bug report.
        </p>

        <div className="mt-4 flex flex-wrap gap-2">
          <button
            onClick={() => exportBackup()}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
          >
            Export backup
          </button>
          <button
            onClick={() => importBackup()}
            className="rounded-md px-4 py-2 text-sm font-medium text-red-600 hover:bg-red-50 disabled:opacity-50 dark:text-red-400 dark:hover:bg-red-950"
          >
            Import backup
          </button>
          <button
            onClick={() => exportDiagnostic()}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
          >
            Export diagnostic report
          </button>
        </div>
      </section>
    </div>
  );
}
