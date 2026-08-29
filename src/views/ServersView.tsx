import { useState } from "react";
import { useAppStore } from "../store";
import { ServerForm } from "../components/ServerForm";

function SubscriptionImportForm({ onDone }: { onDone: () => void }) {
  const importSubscription = useAppStore((s) => s.importSubscription);
  const subscriptionBusy = useAppStore((s) => s.subscriptionBusy);
  const [url, setUrl] = useState("");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!url.trim()) return;
    await importSubscription(url.trim());
    onDone();
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-4 rounded-xl bg-white p-5 shadow dark:bg-slate-800"
    >
      <h3 className="text-base font-semibold">Import from URL</h3>

      <label className="flex flex-col gap-1 text-sm">
        Subscription URL
        <input
          required
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://provider.example.com/subscribe/abc123"
          className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
        />
      </label>

      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onDone}
          className="rounded-md px-4 py-2 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={subscriptionBusy}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          Import
        </button>
      </div>
    </form>
  );
}

export function ServersView() {
  const config = useAppStore((s) => s.config);
  const deleteServer = useAppStore((s) => s.deleteServer);
  const registerWarp = useAppStore((s) => s.registerWarp);
  const warpBusy = useAppStore((s) => s.warpBusy);
  const [showForm, setShowForm] = useState(false);
  const [showImportForm, setShowImportForm] = useState(false);

  const servers = config?.servers ?? [];

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Servers</h2>
        {!showForm && !showImportForm && (
          <div className="flex gap-2">
            <button
              onClick={() => registerWarp()}
              disabled={warpBusy}
              className="rounded-md border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100 disabled:opacity-50 dark:border-slate-600 dark:text-slate-200 dark:hover:bg-slate-700"
            >
              {warpBusy ? "Registering…" : "Get Cloudflare WARP"}
            </button>
            <button
              onClick={() => setShowImportForm(true)}
              className="rounded-md border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100 dark:border-slate-600 dark:text-slate-200 dark:hover:bg-slate-700"
            >
              Import from URL
            </button>
            <button
              onClick={() => setShowForm(true)}
              className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
            >
              Add server
            </button>
          </div>
        )}
      </div>

      {showForm && <ServerForm onDone={() => setShowForm(false)} />}
      {showImportForm && <SubscriptionImportForm onDone={() => setShowImportForm(false)} />}

      {servers.length === 0 ? (
        <p className="rounded-xl bg-white p-5 text-sm text-slate-500 shadow dark:bg-slate-800 dark:text-slate-400">
          No servers yet. Add one to get started.
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {servers.map((server) => (
            <li
              key={server.id}
              className="flex items-center justify-between rounded-xl bg-white p-4 shadow dark:bg-slate-800"
            >
              <div>
                <p className="font-medium">{server.name}</p>
                <p className="text-sm text-slate-500 dark:text-slate-400">
                  {server.protocol} · {server.address}:{server.port}
                  {server.tls?.enabled ? " · TLS" : ""}
                </p>
              </div>
              <button
                onClick={() => deleteServer(server.id)}
                className="rounded-md px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
              >
                Delete
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
