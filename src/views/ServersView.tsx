import { useState } from "react";
import { useAppStore } from "../store";
import { ServerForm } from "../components/ServerForm";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Badge } from "../components/ui";
import type { BadgeVariant } from "../components/ui";
import type { Protocol } from "../types";

const PROTOCOL_BADGE: Record<Protocol, BadgeVariant> = {
  vless: "vless",
  vmess: "vmess",
  trojan: "trojan",
  shadowsocks: "shadowsocks",
  wireguard: "wireguard",
};

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
    <Card>
      <form onSubmit={handleSubmit}>
        <CardHeader>
          <CardTitle>Import from subscription URL</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 pt-4">
          <p className="text-sm text-fg-faint">
            Fetches the URL and imports every server it contains. Supports a base64-encoded body or
            plain text, one share link per line ( <code className="font-mono text-fg-dim">vless://</code>,{" "}
            <code className="font-mono text-fg-dim">trojan://</code>,{" "}
            <code className="font-mono text-fg-dim">ss://</code>,{" "}
            <code className="font-mono text-fg-dim">vmess://</code> ). Importing the same URL twice
            appends duplicates — there's no dedupe yet.
          </p>
          <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
            Subscription URL
            <Input
              required
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://provider.example.com/subscribe/abc123"
            />
          </label>

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onDone}>
              Cancel
            </Button>
            <Button type="submit" busy={subscriptionBusy}>
              Import
            </Button>
          </div>
        </CardContent>
      </form>
    </Card>
  );
}

export function ServersView() {
  const config = useAppStore((s) => s.config);
  const deleteServer = useAppStore((s) => s.deleteServer);
  const registerWarp = useAppStore((s) => s.registerWarp);
  const warpBusy = useAppStore((s) => s.warpBusy);
  const [showForm, setShowForm] = useState(false);
  const [showImportForm, setShowImportForm] = useState(false);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  const servers = config?.servers ?? [];

  function handleDelete(id: string) {
    if (pendingDeleteId === id) {
      deleteServer(id);
      setPendingDeleteId(null);
    } else {
      setPendingDeleteId(id);
    }
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h1 className="font-display text-xl font-semibold text-fg">Servers</h1>
        {!showForm && !showImportForm && (
          <div className="flex gap-2">
            <Button variant="outline" busy={warpBusy} onClick={() => registerWarp()}>
              {warpBusy ? "Registering…" : "Get Cloudflare WARP"}
            </Button>
            <Button variant="outline" onClick={() => setShowImportForm(true)}>
              Import from URL
            </Button>
            <Button onClick={() => setShowForm(true)}>Add server</Button>
          </div>
        )}
      </div>

      {showForm && <ServerForm onDone={() => setShowForm(false)} />}
      {showImportForm && <SubscriptionImportForm onDone={() => setShowImportForm(false)} />}

      {servers.length === 0 ? (
        <Card>
          <CardContent className="text-sm text-fg-faint">
            No servers yet. Add one to get started, or import from a subscription URL.
          </CardContent>
        </Card>
      ) : (
        <ul className="flex flex-col gap-2">
          {servers.map((server) => (
            <li key={server.id}>
              <Card>
                <CardContent className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="font-medium text-fg">{server.name}</p>
                      <Badge variant={PROTOCOL_BADGE[server.protocol]}>{server.protocol}</Badge>
                      {server.tls?.enabled && <Badge variant="secondary">TLS</Badge>}
                    </div>
                    <p className="mt-0.5 truncate font-mono text-sm text-fg-faint">
                      {server.address}:{server.port}
                    </p>
                  </div>
                  <Button
                    variant={pendingDeleteId === server.id ? "destructive" : "ghost"}
                    size="sm"
                    onClick={() => handleDelete(server.id)}
                    onBlur={() => setPendingDeleteId(null)}
                  >
                    {pendingDeleteId === server.id ? "Confirm delete?" : "Delete"}
                  </Button>
                </CardContent>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
