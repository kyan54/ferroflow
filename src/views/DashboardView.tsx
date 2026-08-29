import { useEffect } from "react";
import { useAppStore } from "../store";

function formatUptime(secs: number | null | undefined): string {
  if (secs == null) return "—";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

export function DashboardView() {
  const config = useAppStore((s) => s.config);
  const proxyStatus = useAppStore((s) => s.proxyStatus);
  const systemProxyStatus = useAppStore((s) => s.systemProxyStatus);
  const proxyBusy = useAppStore((s) => s.proxyBusy);
  const refreshProxyStatus = useAppStore((s) => s.refreshProxyStatus);
  const refreshSystemProxyStatus = useAppStore((s) => s.refreshSystemProxyStatus);
  const selectServer = useAppStore((s) => s.selectServer);
  const startProxy = useAppStore((s) => s.startProxy);
  const stopProxy = useAppStore((s) => s.stopProxy);
  const openDashboard = useAppStore((s) => s.openDashboard);

  useEffect(() => {
    refreshProxyStatus();
    refreshSystemProxyStatus();
    const interval = setInterval(refreshProxyStatus, 2000);
    return () => clearInterval(interval);
  }, [refreshProxyStatus, refreshSystemProxyStatus]);

  const servers = config?.servers ?? [];
  const selectedServerId = config?.selectedServerId ?? "";
  const running = proxyStatus?.running ?? false;
  const canStart = !running && !!selectedServerId && !proxyBusy;
  const canStop = running && !proxyBusy;

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-6 p-6">
      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Proxy status</h2>
          <button
            onClick={refreshProxyStatus}
            className="text-sm text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
          >
            Refresh
          </button>
        </div>

        <div className="mt-4 flex items-center gap-3">
          <span
            className={`h-3 w-3 rounded-full ${running ? "bg-emerald-500" : "bg-slate-400"}`}
          />
          <span className="font-medium">{running ? "Running" : "Stopped"}</span>
        </div>

        <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
          <dt className="text-slate-500 dark:text-slate-400">PID</dt>
          <dd>{proxyStatus?.pid ?? "—"}</dd>
          <dt className="text-slate-500 dark:text-slate-400">Uptime</dt>
          <dd>{formatUptime(proxyStatus?.uptimeSecs)}</dd>
          <dt className="text-slate-500 dark:text-slate-400">Active server</dt>
          <dd>
            {servers.find((s) => s.id === proxyStatus?.currentServerId)?.name ??
              proxyStatus?.currentServerId ??
              "—"}
          </dd>
        </dl>

        {proxyStatus?.error && (
          <p className="mt-4 rounded-md bg-red-50 px-3 py-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
            {proxyStatus.error}
            {proxyStatus.errorCode ? ` (${proxyStatus.errorCode})` : ""}
          </p>
        )}

        <div className="mt-5 flex items-center gap-3">
          <select
            value={selectedServerId}
            onChange={(e) => selectServer(e.target.value || null)}
            disabled={running}
            className="flex-1 rounded-md border border-slate-300 bg-white px-3 py-2 text-sm disabled:opacity-60 dark:border-slate-600 dark:bg-slate-900"
          >
            <option value="">Select a server…</option>
            {servers.map((server) => (
              <option key={server.id} value={server.id}>
                {server.name} ({server.protocol})
              </option>
            ))}
          </select>

          {running ? (
            <button
              onClick={stopProxy}
              disabled={!canStop}
              className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
            >
              Stop
            </button>
          ) : (
            <button
              onClick={() => selectedServerId && startProxy(selectedServerId)}
              disabled={!canStart}
              title={!selectedServerId ? "Select a server first" : undefined}
              className="rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
            >
              Start
            </button>
          )}
        </div>

        {servers.length === 0 && (
          <p className="mt-3 text-sm text-slate-500 dark:text-slate-400">
            No servers configured yet — add one in the Servers tab.
          </p>
        )}

        <div className="mt-4 border-t border-slate-200 pt-4 dark:border-slate-700">
          <button
            onClick={openDashboard}
            disabled={!running}
            title={!running ? "Start the proxy first" : undefined}
            className="rounded-md border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100 disabled:opacity-50 dark:border-slate-600 dark:text-slate-200 dark:hover:bg-slate-700"
          >
            Open sing-box dashboard
          </button>
        </div>
      </section>

      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">System proxy</h2>
          <button
            onClick={refreshSystemProxyStatus}
            className="text-sm text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
          >
            Refresh
          </button>
        </div>
        <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
          <dt className="text-slate-500 dark:text-slate-400">Enabled</dt>
          <dd>{systemProxyStatus?.enabled ? "Yes" : "No"}</dd>
          <dt className="text-slate-500 dark:text-slate-400">HTTP proxy</dt>
          <dd>{systemProxyStatus?.httpProxy ?? "—"}</dd>
          <dt className="text-slate-500 dark:text-slate-400">HTTPS proxy</dt>
          <dd>{systemProxyStatus?.httpsProxy ?? "—"}</dd>
          <dt className="text-slate-500 dark:text-slate-400">SOCKS proxy</dt>
          <dd>{systemProxyStatus?.socksProxy ?? "—"}</dd>
          <dt className="text-slate-500 dark:text-slate-400">Bypass list</dt>
          <dd>
            {systemProxyStatus?.bypassList?.length
              ? systemProxyStatus.bypassList.join(", ")
              : "—"}
          </dd>
        </dl>
      </section>
    </div>
  );
}
