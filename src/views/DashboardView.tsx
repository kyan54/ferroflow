import { useEffect } from "react";
import { useAppStore } from "../store";
import { Card, CardHeader, CardTitle, CardContent, Button, Select, Badge } from "../components/ui";

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
      <h1 className="font-display text-xl font-semibold text-fg">Dashboard</h1>

      <Card>
        <CardHeader>
          <CardTitle>Proxy status</CardTitle>
          <Button variant="ghost" size="sm" onClick={refreshProxyStatus}>
            Refresh
          </Button>
        </CardHeader>
        <CardContent className="pt-4">
          <div className="flex items-center gap-3">
            <span className={`relative flex h-2.5 w-2.5 ${running ? "" : ""}`}>
              {running && (
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-ok opacity-60" />
              )}
              <span
                className={`relative inline-flex h-2.5 w-2.5 rounded-full ${
                  running ? "bg-ok" : "bg-fg-faint"
                }`}
              />
            </span>
            <span className="font-medium text-fg">{running ? "Running" : "Stopped"}</span>
            {proxyBusy && <span className="text-xs text-fg-faint">working…</span>}
          </div>

          <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
            <dt className="text-fg-faint">PID</dt>
            <dd className="font-mono text-fg-dim">{proxyStatus?.pid ?? "—"}</dd>
            <dt className="text-fg-faint">Uptime</dt>
            <dd className="font-mono text-fg-dim">{formatUptime(proxyStatus?.uptimeSecs)}</dd>
            <dt className="text-fg-faint">Active server</dt>
            <dd className="text-fg-dim">
              {servers.find((s) => s.id === proxyStatus?.currentServerId)?.name ??
                proxyStatus?.currentServerId ??
                "—"}
            </dd>
          </dl>

          {proxyStatus?.error && (
            <p className="mt-4 rounded-md border border-err/30 bg-err-weak px-3 py-2 text-sm text-err">
              {proxyStatus.error}
              {proxyStatus.errorCode ? ` (${proxyStatus.errorCode})` : ""}
            </p>
          )}

          <div className="mt-5 flex items-center gap-3">
            <Select
              value={selectedServerId}
              onChange={(e) => selectServer(e.target.value || null)}
              disabled={running}
              className="flex-1"
            >
              <option value="">Select a server…</option>
              {servers.map((server) => (
                <option key={server.id} value={server.id}>
                  {server.name} ({server.protocol})
                </option>
              ))}
            </Select>

            {running ? (
              <Button variant="destructive" busy={proxyBusy} disabled={!canStop} onClick={stopProxy}>
                Stop
              </Button>
            ) : (
              <Button
                busy={proxyBusy}
                disabled={!canStart}
                title={!selectedServerId ? "Select a server first" : undefined}
                onClick={() => selectedServerId && startProxy(selectedServerId)}
              >
                Start
              </Button>
            )}
          </div>

          {servers.length === 0 && (
            <p className="mt-3 text-sm text-fg-faint">
              No servers configured yet — add one in the Servers tab.
            </p>
          )}

          <div className="mt-4 border-t border-line pt-4">
            <Button
              variant="outline"
              disabled={!running}
              title={!running ? "Start the proxy first" : undefined}
              onClick={openDashboard}
            >
              Open sing-box dashboard
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>System proxy</CardTitle>
          <div className="flex items-center gap-2">
            <Badge variant={systemProxyStatus?.enabled ? "success" : "secondary"}>
              {systemProxyStatus?.enabled ? "Enabled" : "Disabled"}
            </Badge>
            <Button variant="ghost" size="sm" onClick={refreshSystemProxyStatus}>
              Refresh
            </Button>
          </div>
        </CardHeader>
        <CardContent className="pt-4">
          <dl className="grid grid-cols-2 gap-y-2 text-sm">
            <dt className="text-fg-faint">HTTP proxy</dt>
            <dd className="font-mono text-fg-dim">{systemProxyStatus?.httpProxy ?? "—"}</dd>
            <dt className="text-fg-faint">HTTPS proxy</dt>
            <dd className="font-mono text-fg-dim">{systemProxyStatus?.httpsProxy ?? "—"}</dd>
            <dt className="text-fg-faint">SOCKS proxy</dt>
            <dd className="font-mono text-fg-dim">{systemProxyStatus?.socksProxy ?? "—"}</dd>
            <dt className="text-fg-faint">Bypass list</dt>
            <dd className="text-fg-dim">
              {systemProxyStatus?.bypassList?.length
                ? systemProxyStatus.bypassList.join(", ")
                : "—"}
            </dd>
          </dl>
        </CardContent>
      </Card>
    </div>
  );
}
