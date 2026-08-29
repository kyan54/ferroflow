import { useState } from "react";
import { useAppStore } from "../store";
import { ServerForm } from "../components/ServerForm";

export function ServersView() {
  const config = useAppStore((s) => s.config);
  const deleteServer = useAppStore((s) => s.deleteServer);
  const [showForm, setShowForm] = useState(false);

  const servers = config?.servers ?? [];

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Servers</h2>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
          >
            Add server
          </button>
        )}
      </div>

      {showForm && <ServerForm onDone={() => setShowForm(false)} />}

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
