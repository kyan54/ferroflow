import { useEffect } from "react";
import type { SVGProps } from "react";
import { useAppStore } from "../store";
import { cn, formatBytes } from "../lib/utils";
import { PROXY_MODES, PROXY_MODE_TYPES } from "../types";
import type { ProxyMode, ProxyModeType } from "../types";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Badge,
  Spinner,
  SegmentedControl,
} from "../components/ui";
import { ConnectionTopology } from "../components/ConnectionTopology";
import { UnlockStatusCard } from "../components/UnlockStatusCard";

const TAKEOVER_LABELS: Record<ProxyModeType, string> = {
  systemProxy: "System proxy",
  tun: "TUN",
  manual: "Manual",
};

const ROUTING_LABELS: Record<ProxyMode, string> = {
  global: "Global",
  smart: "Smart routing",
  direct: "Direct",
};

/** Whole minutes only -- matches the reference app's header summary
 * ("运行时间: 598 分钟"), which is coarser than the HH:MM:SS the old
 * "Proxy status" card used to show. */
function formatUptimeMinutes(secs: number | null | undefined): string | null {
  if (secs == null) return null;
  return `${Math.floor(secs / 60)}m`;
}

function icon(props: SVGProps<SVGSVGElement>) {
  return {
    viewBox: "0 0 24 24",
    fill: "none" as const,
    stroke: "currentColor",
    strokeWidth: 1.9,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    ...props,
  };
}

function ChevronDownIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}

function PlayIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" {...props}>
      <path d="M7 5.5v13a1 1 0 0 0 1.53.85l10.5-6.5a1 1 0 0 0 0-1.7l-10.5-6.5A1 1 0 0 0 7 5.5Z" />
    </svg>
  );
}

function StopIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" {...props}>
      <rect x="6" y="6" width="12" height="12" rx="2" />
    </svg>
  );
}

function ExternalLinkIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M14 4h6v6M20 4l-9 9M9 5H6a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2v-3" />
    </svg>
  );
}

export function DashboardView() {
  const config = useAppStore((s) => s.config);
  const proxyStatus = useAppStore((s) => s.proxyStatus);
  const systemProxyStatus = useAppStore((s) => s.systemProxyStatus);
  const proxyBusy = useAppStore((s) => s.proxyBusy);
  const helperStatus = useAppStore((s) => s.helperStatus);
  const connectionsSnapshot = useAppStore((s) => s.connectionsSnapshot);
  const refreshProxyStatus = useAppStore((s) => s.refreshProxyStatus);
  const refreshSystemProxyStatus = useAppStore((s) => s.refreshSystemProxyStatus);
  const refreshHelperStatus = useAppStore((s) => s.refreshHelperStatus);
  const refreshConnections = useAppStore((s) => s.refreshConnections);
  const selectServer = useAppStore((s) => s.selectServer);
  const startProxy = useAppStore((s) => s.startProxy);
  const stopProxy = useAppStore((s) => s.stopProxy);
  const openDashboard = useAppStore((s) => s.openDashboard);
  const saveConfig = useAppStore((s) => s.saveConfig);
  const unlockResults = useAppStore((s) => s.unlockResults);
  const unlockBusy = useAppStore((s) => s.unlockBusy);
  const unlockError = useAppStore((s) => s.unlockError);
  const checkUnlock = useAppStore((s) => s.checkUnlock);

  useEffect(() => {
    refreshProxyStatus();
    refreshSystemProxyStatus();
    refreshHelperStatus();
    const interval = setInterval(refreshProxyStatus, 2000);
    return () => clearInterval(interval);
  }, [refreshProxyStatus, refreshSystemProxyStatus, refreshHelperStatus]);

  // Bottom status bar needs live traffic totals -- ConnectionsView polls
  // connectionsSnapshot the same way, but it unmounts when navigating away
  // (App.tsx renders views with `&&`, not a keep-alive router), so the
  // Dashboard needs its own poll while it's the active view.
  useEffect(() => {
    refreshConnections();
    const interval = setInterval(refreshConnections, 2000);
    return () => clearInterval(interval);
  }, [refreshConnections]);

  const servers = config?.servers ?? [];
  const selectedServerId = config?.selectedServerId ?? "";
  const selectedServer = servers.find((s) => s.id === selectedServerId);
  const running = proxyStatus?.running ?? false;
  const canStart = !running && !!selectedServerId && !proxyBusy;
  const canStop = running && !proxyBusy;

  const proxyModeType = config?.proxyModeType ?? "systemProxy";
  const proxyMode = config?.proxyMode ?? "smart";
  const uptimeLabel = running ? formatUptimeMinutes(proxyStatus?.uptimeSecs) : null;

  const summaryParts = [
    TAKEOVER_LABELS[proxyModeType],
    ROUTING_LABELS[proxyMode],
    uptimeLabel ? `Uptime: ${uptimeLabel}` : null,
  ].filter((part): part is string => !!part);

  const runningServer = servers.find((s) => s.id === proxyStatus?.currentServerId);
  const connectionStateLabel = running
    ? [
        "Connected",
        runningServer?.name ?? proxyStatus?.currentServerId ?? null,
        runningServer ? `${runningServer.address}:${runningServer.port}` : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "Disconnected";

  function setProxyModeType(modeType: ProxyModeType) {
    if (!config) return;
    saveConfig({ ...config, proxyModeType: modeType });
  }

  function setProxyMode(mode: ProxyMode) {
    if (!config) return;
    saveConfig({ ...config, proxyMode: mode });
  }

  return (
    <div className="flex h-full flex-col">
      <div className="mx-auto flex w-full max-w-2xl flex-1 flex-col gap-6 p-6">
        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
          <h1 className="font-display text-xl font-semibold text-fg">Dashboard</h1>
          {summaryParts.length > 0 && (
            <p className="truncate text-xs text-fg-faint">{summaryParts.join(" · ")}</p>
          )}
        </div>

        <Card>
          <CardHeader>
            <CardTitle>Exit node</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 pt-4">
            {servers.length === 0 ? (
              <p className="text-sm text-fg-faint">
                No servers configured yet — add one in the Servers tab.
              </p>
            ) : (
              <div className="flex items-center gap-2">
                {/* One cohesive "current server" control: the visible dot +
                    badge + name + address + chevron are purely decorative,
                    and a borderless native <select> is stretched invisibly
                    over them so the whole row opens the picker on click --
                    the simplification the brief allows in place of a real
                    popover, without looking like a separate form field. */}
                <div className="relative flex min-w-0 flex-1 items-center gap-2.5 rounded-lg border border-line bg-surface-2 px-3 py-2">
                  <span className="relative flex h-2.5 w-2.5 shrink-0">
                    {running && (
                      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-ok opacity-60" />
                    )}
                    <span
                      className={cn(
                        "relative inline-flex h-2.5 w-2.5 rounded-full",
                        running ? "bg-ok" : "bg-fg-faint",
                      )}
                    />
                  </span>

                  {selectedServer ? (
                    <>
                      <Badge variant={selectedServer.protocol} className="shrink-0">
                        {selectedServer.protocol}
                      </Badge>
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-semibold text-fg">
                          {selectedServer.name}
                        </div>
                        <div className="truncate text-xs text-fg-faint">
                          {selectedServer.address}:{selectedServer.port}
                        </div>
                      </div>
                    </>
                  ) : (
                    <span className="flex-1 text-sm text-fg-faint">Select a server…</span>
                  )}

                  <ChevronDownIcon className="h-4 w-4 shrink-0 text-fg-faint" />

                  <select
                    aria-label="Exit node"
                    value={selectedServerId}
                    disabled={running}
                    onChange={(e) => selectServer(e.target.value || null)}
                    className="absolute inset-0 h-full w-full cursor-pointer opacity-0 disabled:cursor-not-allowed"
                  >
                    <option value="">Select a server…</option>
                    {servers.map((server) => (
                      <option key={server.id} value={server.id}>
                        {server.name} ({server.protocol})
                      </option>
                    ))}
                  </select>
                </div>

                <Button
                  variant="ghost"
                  size="icon"
                  disabled={!running}
                  title="Open sing-box dashboard"
                  aria-label="Open sing-box dashboard"
                  onClick={openDashboard}
                >
                  <ExternalLinkIcon className="h-4 w-4" />
                </Button>

                {running ? (
                  <button
                    type="button"
                    onClick={stopProxy}
                    disabled={!canStop}
                    title="Stop"
                    aria-label="Stop"
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-err text-white transition-colors hover:brightness-95 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {proxyBusy ? <Spinner className="h-4 w-4" /> : <StopIcon className="h-4 w-4" />}
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => selectedServerId && startProxy(selectedServerId)}
                    disabled={!canStart}
                    title={!selectedServerId ? "Select a server first" : "Start"}
                    aria-label="Start"
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-flow text-white transition-colors hover:brightness-95 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {proxyBusy ? <Spinner className="h-4 w-4" /> : <PlayIcon className="h-4 w-4" />}
                  </button>
                )}
              </div>
            )}

            {proxyStatus?.error && (
              <p className="rounded-md border border-err/30 bg-err-weak px-3 py-2 text-sm text-err">
                {proxyStatus.error}
                {proxyStatus.errorCode ? ` (${proxyStatus.errorCode})` : ""}
              </p>
            )}
          </CardContent>
        </Card>

        <div className="grid grid-cols-2 gap-4">
          <Card>
            <CardHeader>
              <CardTitle>Takeover mode</CardTitle>
            </CardHeader>
            <CardContent className="pt-4">
              <SegmentedControl
                aria-label="Takeover mode"
                value={proxyModeType}
                onChange={setProxyModeType}
                options={PROXY_MODE_TYPES.map((modeType) => ({
                  value: modeType,
                  label: TAKEOVER_LABELS[modeType],
                }))}
              />
              {proxyModeType === "tun" && !helperStatus?.ready && (
                <p className="mt-3 text-xs text-warn">
                  TUN mode needs the privileged helper installed (Settings → Privileged helper) —
                  starting the proxy without it will fail.
                </p>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Routing strategy</CardTitle>
            </CardHeader>
            <CardContent className="pt-4">
              <SegmentedControl
                aria-label="Routing strategy"
                value={proxyMode}
                onChange={setProxyMode}
                options={PROXY_MODES.map((mode) => ({
                  value: mode,
                  label: ROUTING_LABELS[mode],
                }))}
              />
            </CardContent>
          </Card>
        </div>

        <ConnectionTopology snapshot={connectionsSnapshot} running={running} />

        <UnlockStatusCard
          results={unlockResults}
          busy={unlockBusy}
          error={unlockError}
          running={running}
          onCheck={checkUnlock}
        />

        {/* Ferroflow-specific diagnostics with no equivalent slot in FlowZ's
            home layout and nowhere else in the app that surfaces it -- kept
            here rather than dropped, just demoted below the structure that
            now mirrors the reference app. */}
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

      {/* Bottom status bar -- pinned to the bottom of this view's own
          scroll area via `sticky`, not a window-wide fixed bar (App.tsx
          switches views by conditional render, not a router, so a global
          footer would need plumbing through every view for little gain). */}
      <div className="sticky bottom-0 flex shrink-0 items-center justify-between gap-4 border-t border-line bg-surface/95 px-6 py-2.5 text-xs backdrop-blur">
        <span className={cn("font-medium", running ? "text-ok" : "text-fg-faint")}>
          {connectionStateLabel}
        </span>
        <span className="flex items-center gap-3 font-mono text-fg-dim">
          <span className="text-dn">
            ↓ {formatBytes(connectionsSnapshot?.downloadTotal ?? 0)} total
          </span>
          <span className="text-up">
            ↑ {formatBytes(connectionsSnapshot?.uploadTotal ?? 0)} total
          </span>
        </span>
      </div>
    </div>
  );
}
