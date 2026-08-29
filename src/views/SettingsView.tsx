import { useEffect } from "react";
import { useAppStore } from "../store";
import { PROXY_MODES } from "../types";
import type { ProxyMode } from "../types";

export function SettingsView() {
  const config = useAppStore((s) => s.config);
  const platformInfo = useAppStore((s) => s.platformInfo);
  const refreshPlatformInfo = useAppStore((s) => s.refreshPlatformInfo);
  const saveConfig = useAppStore((s) => s.saveConfig);

  useEffect(() => {
    refreshPlatformInfo();
  }, [refreshPlatformInfo]);

  if (!config) {
    return <div className="p-6 text-sm text-slate-500 dark:text-slate-400">Loading…</div>;
  }

  function toggle(key: "autoStart" | "minimizeToTray") {
    if (!config) return;
    saveConfig({ ...config, [key]: !config[key] });
  }

  function setProxyMode(mode: ProxyMode) {
    if (!config) return;
    saveConfig({ ...config, proxyMode: mode });
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
      </section>
    </div>
  );
}
