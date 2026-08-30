import { useEffect } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import type { ConnectionMetadata, HistoryEntry } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button } from "../components/ui";

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

function destinationLabel(metadata: ConnectionMetadata): string {
  if (metadata.host) return metadata.host;
  return `${metadata.destinationIP}:${metadata.destinationPort}`;
}

export function ConnectionsView() {
  const { t } = useTranslation();
  const connectionsSnapshot = useAppStore((s) => s.connectionsSnapshot);
  const refreshConnections = useAppStore((s) => s.refreshConnections);
  const closeConnection = useAppStore((s) => s.closeConnection);
  const closeAllConnections = useAppStore((s) => s.closeAllConnections);

  const historyEntries = useAppStore((s) => s.historyEntries);
  const refreshHistory = useAppStore((s) => s.refreshHistory);
  const clearHistory = useAppStore((s) => s.clearHistory);

  useEffect(() => {
    refreshConnections();
    const interval = setInterval(refreshConnections, 2000);
    return () => clearInterval(interval);
  }, [refreshConnections]);

  // History is a look-back, not a live view -- fetched once on mount, with
  // a manual refresh button below rather than a poll.
  useEffect(() => {
    refreshHistory();
  }, [refreshHistory]);

  const connections = connectionsSnapshot?.connections ?? [];

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 p-6">
      <h1 className="font-display text-xl font-semibold text-fg">{t.connections.title}</h1>

      <Card>
        <CardHeader>
          <CardTitle>{t.connections.activeTitle}</CardTitle>
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="sm" onClick={refreshConnections}>
              {t.connections.refresh}
            </Button>
            <Button variant="destructive" size="sm" disabled={connections.length === 0} onClick={closeAllConnections}>
              {t.connections.closeAll}
            </Button>
          </div>
        </CardHeader>

        <CardContent className="pt-4">
          <dl className="grid grid-cols-2 gap-y-2 text-sm">
            <dt className="text-fg-faint">{t.connections.totalDownloaded}</dt>
            <dd className="font-mono text-dn">{formatBytes(connectionsSnapshot?.downloadTotal ?? 0)}</dd>
            <dt className="text-fg-faint">{t.connections.totalUploaded}</dt>
            <dd className="font-mono text-up">{formatBytes(connectionsSnapshot?.uploadTotal ?? 0)}</dd>
          </dl>

          <div className="mt-5 overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-line text-fg-faint">
                  <th className="py-2 pr-3 font-medium">{t.connections.columnDestination}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnNetwork}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnChain}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnRule}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnDownload}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnUpload}</th>
                  <th className="py-2 pr-3 font-medium"></th>
                </tr>
              </thead>
              <tbody>
                {connections.map((conn) => (
                  <tr key={conn.id} className="border-b border-line/60 last:border-0">
                    <td className="py-2 pr-3 font-mono text-fg">{destinationLabel(conn.metadata)}</td>
                    <td className="py-2 pr-3 uppercase text-fg-dim">{conn.metadata.network}</td>
                    <td className="py-2 pr-3 text-fg-dim">{conn.chains.join(" → ")}</td>
                    <td className="py-2 pr-3 text-fg-dim">{conn.rule || "—"}</td>
                    <td className="py-2 pr-3 font-mono text-dn">{formatBytes(conn.download)}</td>
                    <td className="py-2 pr-3 font-mono text-up">{formatBytes(conn.upload)}</td>
                    <td className="py-2 pr-3 text-right">
                      <button
                        onClick={() => closeConnection(conn.id)}
                        className="text-xs text-fg-faint hover:text-err"
                      >
                        {t.connections.close}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            {connections.length === 0 && (
              <p className="mt-3 text-sm text-fg-faint">{t.connections.emptyActive}</p>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.connections.historyTitle}</CardTitle>
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="sm" onClick={refreshHistory}>
              {t.connections.refresh}
            </Button>
            <Button variant="destructive" size="sm" disabled={historyEntries.length === 0} onClick={clearHistory}>
              {t.connections.clearHistory}
            </Button>
          </div>
        </CardHeader>

        <CardContent className="pt-4">
          <p className="-mt-2 mb-3 text-sm text-fg-faint">{t.connections.historyExplainer}</p>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-line text-fg-faint">
                  <th className="py-2 pr-3 font-medium">{t.connections.columnDestination}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnNetwork}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnChain}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnRule}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnDownload}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnUpload}</th>
                  <th className="py-2 pr-3 font-medium">{t.connections.columnEnded}</th>
                </tr>
              </thead>
              <tbody>
                {historyEntries.map((entry: HistoryEntry) => (
                  <tr key={entry.id} className="border-b border-line/60 last:border-0">
                    <td className="py-2 pr-3 font-mono text-fg">{destinationLabel(entry.metadata)}</td>
                    <td className="py-2 pr-3 uppercase text-fg-dim">{entry.metadata.network}</td>
                    <td className="py-2 pr-3 text-fg-dim">{entry.chains.join(" → ")}</td>
                    <td className="py-2 pr-3 text-fg-dim">{entry.rule || "—"}</td>
                    <td className="py-2 pr-3 font-mono text-dn">{formatBytes(entry.download)}</td>
                    <td className="py-2 pr-3 font-mono text-up">{formatBytes(entry.upload)}</td>
                    <td className="py-2 pr-3 text-fg-dim">{new Date(entry.end).toLocaleString()}</td>
                  </tr>
                ))}
              </tbody>
            </table>

            {historyEntries.length === 0 && (
              <p className="mt-3 text-sm text-fg-faint">{t.connections.emptyHistory}</p>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
