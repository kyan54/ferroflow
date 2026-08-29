import { useEffect } from "react";
import { useAppStore } from "../store";
import type { ConnectionMetadata, HistoryEntry } from "../types";

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
      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Connections</h2>
          <div className="flex items-center gap-3">
            <button
              onClick={refreshConnections}
              className="text-sm text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
            >
              Refresh
            </button>
            <button
              onClick={closeAllConnections}
              disabled={connections.length === 0}
              className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
            >
              Close all
            </button>
          </div>
        </div>

        <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
          <dt className="text-slate-500 dark:text-slate-400">Total downloaded</dt>
          <dd>{formatBytes(connectionsSnapshot?.downloadTotal ?? 0)}</dd>
          <dt className="text-slate-500 dark:text-slate-400">Total uploaded</dt>
          <dd>{formatBytes(connectionsSnapshot?.uploadTotal ?? 0)}</dd>
        </dl>

        <div className="mt-5 overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-slate-200 text-slate-500 dark:border-slate-700 dark:text-slate-400">
                <th className="py-2 pr-3 font-medium">Destination</th>
                <th className="py-2 pr-3 font-medium">Network</th>
                <th className="py-2 pr-3 font-medium">Chain</th>
                <th className="py-2 pr-3 font-medium">Rule</th>
                <th className="py-2 pr-3 font-medium">Download</th>
                <th className="py-2 pr-3 font-medium">Upload</th>
                <th className="py-2 pr-3 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {connections.map((conn) => (
                <tr
                  key={conn.id}
                  className="border-b border-slate-100 last:border-0 dark:border-slate-800"
                >
                  <td className="py-2 pr-3">{destinationLabel(conn.metadata)}</td>
                  <td className="py-2 pr-3 uppercase">{conn.metadata.network}</td>
                  <td className="py-2 pr-3">{conn.chains.join(" → ")}</td>
                  <td className="py-2 pr-3">{conn.rule || "—"}</td>
                  <td className="py-2 pr-3">{formatBytes(conn.download)}</td>
                  <td className="py-2 pr-3">{formatBytes(conn.upload)}</td>
                  <td className="py-2 pr-3 text-right">
                    <button
                      onClick={() => closeConnection(conn.id)}
                      className="text-xs text-slate-500 hover:text-red-600 dark:text-slate-400 dark:hover:text-red-400"
                    >
                      Close
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {connections.length === 0 && (
            <p className="mt-3 text-sm text-slate-500 dark:text-slate-400">
              No active connections.
            </p>
          )}
        </div>
      </section>

      <section className="rounded-xl bg-white p-5 shadow dark:bg-slate-800">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">History</h2>
          <div className="flex items-center gap-3">
            <button
              onClick={refreshHistory}
              className="text-sm text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
            >
              Refresh
            </button>
            <button
              onClick={clearHistory}
              disabled={historyEntries.length === 0}
              className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
            >
              Clear history
            </button>
          </div>
        </div>

        <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
          A look-back at connections that have finished, not a live view -- use Refresh to pull the
          latest. Only recorded when "Record connection history" is turned on in Settings.
        </p>

        <div className="mt-4 overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-slate-200 text-slate-500 dark:border-slate-700 dark:text-slate-400">
                <th className="py-2 pr-3 font-medium">Destination</th>
                <th className="py-2 pr-3 font-medium">Network</th>
                <th className="py-2 pr-3 font-medium">Chain</th>
                <th className="py-2 pr-3 font-medium">Rule</th>
                <th className="py-2 pr-3 font-medium">Download</th>
                <th className="py-2 pr-3 font-medium">Upload</th>
                <th className="py-2 pr-3 font-medium">Ended</th>
              </tr>
            </thead>
            <tbody>
              {historyEntries.map((entry: HistoryEntry) => (
                <tr
                  key={entry.id}
                  className="border-b border-slate-100 last:border-0 dark:border-slate-800"
                >
                  <td className="py-2 pr-3">{destinationLabel(entry.metadata)}</td>
                  <td className="py-2 pr-3 uppercase">{entry.metadata.network}</td>
                  <td className="py-2 pr-3">{entry.chains.join(" → ")}</td>
                  <td className="py-2 pr-3">{entry.rule || "—"}</td>
                  <td className="py-2 pr-3">{formatBytes(entry.download)}</td>
                  <td className="py-2 pr-3">{formatBytes(entry.upload)}</td>
                  <td className="py-2 pr-3">{new Date(entry.end).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>

          {historyEntries.length === 0 && (
            <p className="mt-3 text-sm text-slate-500 dark:text-slate-400">
              No connection history yet.
            </p>
          )}
        </div>
      </section>
    </div>
  );
}
