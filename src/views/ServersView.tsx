import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { ServerForm } from "../components/ServerForm";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Input,
  Textarea,
  Badge,
  SegmentedControl,
} from "../components/ui";
import type { BadgeVariant } from "../components/ui";
import type { Protocol } from "../types";

const PROTOCOL_BADGE: Record<Protocol, BadgeVariant> = {
  vless: "vless",
  vmess: "vmess",
  trojan: "trojan",
  shadowsocks: "shadowsocks",
  wireguard: "wireguard",
};

const SHARE_LINK_HINT = (
  <>
    <code className="font-mono text-fg-dim">vless://</code>, <code className="font-mono text-fg-dim">trojan://</code>
    , <code className="font-mono text-fg-dim">ss://</code>, <code className="font-mono text-fg-dim">vmess://</code>
  </>
);

type ImportMode = "url" | "paste" | "file";

function SubscriptionImportForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const importSubscription = useAppStore((s) => s.importSubscription);
  const importSubscriptionText = useAppStore((s) => s.importSubscriptionText);
  const importSubscriptionFile = useAppStore((s) => s.importSubscriptionFile);
  const subscriptionBusy = useAppStore((s) => s.subscriptionBusy);
  const IMPORT_MODE_OPTIONS: { value: ImportMode; label: string }[] = [
    { value: "url", label: t.servers.importForm.modeUrl },
    { value: "paste", label: t.servers.importForm.modePaste },
    { value: "file", label: t.servers.importForm.modeFile },
  ];
  const [mode, setMode] = useState<ImportMode>("url");
  const [url, setUrl] = useState("");
  const [text, setText] = useState("");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (mode === "url") {
      if (!url.trim()) return;
      await importSubscription(url.trim());
      onDone();
    } else if (mode === "paste") {
      if (!text.trim()) return;
      await importSubscriptionText(text);
      onDone();
    }
  }

  async function handleChooseFile() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Server list", extensions: ["txt", "yaml", "yml"] }],
    });
    if (!selected) return;
    const path = Array.isArray(selected) ? selected[0] : selected;
    await importSubscriptionFile(path);
    onDone();
  }

  return (
    <Card>
      <form onSubmit={handleSubmit}>
        <CardHeader>
          <CardTitle>{t.servers.importForm.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 pt-4">
          <SegmentedControl
            aria-label={t.servers.importForm.modeUrl}
            options={IMPORT_MODE_OPTIONS}
            value={mode}
            onChange={setMode}
          />

          {mode === "url" && (
            <>
              <p className="text-sm text-fg-faint">
                {t.servers.importForm.urlExplainer} ( {SHARE_LINK_HINT} )
              </p>
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.servers.importForm.urlLabel}
                <Input
                  required
                  type="url"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://provider.example.com/subscribe/abc123"
                />
              </label>
            </>
          )}

          {mode === "paste" && (
            <>
              <p className="text-sm text-fg-faint">
                {t.servers.importForm.pasteExplainer} ( {SHARE_LINK_HINT} )
              </p>
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.servers.importForm.pasteLabel}
                <Textarea
                  required
                  rows={6}
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  placeholder="vless://...&#10;trojan://...&#10;ss://..."
                  className="font-mono text-xs"
                />
              </label>
            </>
          )}

          {mode === "file" && (
            <>
              <p className="text-sm text-fg-faint">{t.servers.importForm.fileExplainer}</p>
              <div className="flex justify-end">
                <Button type="button" onClick={handleChooseFile} busy={subscriptionBusy}>
                  {t.servers.importForm.chooseFile}
                </Button>
              </div>
            </>
          )}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onDone}>
              {t.servers.importForm.cancel}
            </Button>
            {mode !== "file" && (
              <Button type="submit" busy={subscriptionBusy}>
                {t.servers.importForm.submit}
              </Button>
            )}
          </div>
        </CardContent>
      </form>
    </Card>
  );
}

export function ServersView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const deleteServer = useAppStore((s) => s.deleteServer);
  const duplicateServer = useAppStore((s) => s.duplicateServer);
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
        <h1 className="font-display text-xl font-semibold text-fg">{t.servers.title}</h1>
        {!showForm && !showImportForm && (
          <div className="flex gap-2">
            <Button variant="outline" busy={warpBusy} onClick={() => registerWarp()}>
              {warpBusy ? t.servers.registeringWarp : t.servers.getWarp}
            </Button>
            <Button variant="outline" onClick={() => setShowImportForm(true)}>
              {t.servers.import}
            </Button>
            <Button onClick={() => setShowForm(true)}>{t.servers.addServer}</Button>
          </div>
        )}
      </div>

      {showForm && <ServerForm onDone={() => setShowForm(false)} />}
      {showImportForm && <SubscriptionImportForm onDone={() => setShowImportForm(false)} />}

      {servers.length === 0 ? (
        <Card>
          <CardContent className="text-sm text-fg-faint">{t.servers.empty}</CardContent>
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
                      {server.tls?.enabled && <Badge variant="secondary">{t.servers.tlsBadge}</Badge>}
                    </div>
                    <p className="mt-0.5 truncate font-mono text-sm text-fg-faint">
                      {server.address}:{server.port}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button variant="ghost" size="sm" onClick={() => duplicateServer(server.id)}>
                      {t.servers.duplicate}
                    </Button>
                    <Button
                      variant={pendingDeleteId === server.id ? "destructive" : "ghost"}
                      size="sm"
                      onClick={() => handleDelete(server.id)}
                      onBlur={() => setPendingDeleteId(null)}
                    >
                      {pendingDeleteId === server.id ? t.servers.confirmDelete : t.servers.delete}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
