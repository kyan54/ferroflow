import { useEffect, useMemo, useRef, useState } from "react";
import type { SVGProps } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import type { Dictionary } from "../i18n/dictionary";
import { ServerForm } from "../components/ServerForm";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Input,
  Textarea,
  Select,
  Badge,
  SegmentedControl,
} from "../components/ui";
import { cn } from "../lib/utils";
import { PROTOCOLS } from "../types";
import type { Protocol, ServerConfig } from "../types";

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

function SearchIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <circle cx="11" cy="11" r="7" />
      <path d="M21 21l-4.3-4.3" />
    </svg>
  );
}

function BoltIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" {...props}>
      <path d="M13 2 3 14h7l-1 8 11-13h-7l0-7Z" />
    </svg>
  );
}

function DuplicateIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <rect x="9" y="9" width="12" height="12" rx="2" />
      <path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1" />
    </svg>
  );
}

function TrashIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M4 7h16M9 7V4h6v3M6 7l1 13a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-13" />
    </svg>
  );
}

function ChevronDownIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}

function PlusIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

const SHARE_LINK_HINT = (
  <>
    <code className="font-mono text-fg-dim">vless://</code>, <code className="font-mono text-fg-dim">trojan://</code>
    , <code className="font-mono text-fg-dim">ss://</code>, <code className="font-mono text-fg-dim">vmess://</code>
  </>
);

type ImportMode = "url" | "paste" | "file";

function SubscriptionImportForm({
  initialMode = "url",
  onDone,
}: {
  initialMode?: ImportMode;
  onDone: () => void;
}) {
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
  const [mode, setMode] = useState<ImportMode>(initialMode);
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

/** Rough latency-bucket color, matching the existing warn/ok/err token
 * convention -- purely a visual hint, not a precision measurement. */
function latencyColorClass(ms: number): string {
  if (ms <= 150) return "text-ok";
  if (ms <= 400) return "text-warn";
  return "text-err";
}

/** Security detail line -- only what's genuinely derivable from
 * `ServerConfig` (TLS enabled + Reality present). No transport field
 * (`network`/`ws-opts`/etc.) exists on `ServerConfig` at all -- see
 * docs/ipc-contract.md's "Subscription import" section -- so this
 * deliberately doesn't fabricate a "tcp ·" prefix the reference screenshot
 * shows. */
function securityDetail(server: ServerConfig, t: Dictionary): string {
  if (server.protocol === "wireguard") return "WireGuard";
  if (!server.tls?.enabled) return t.servers.card.noTls;
  if (server.tls.realityPublicKey) return t.servers.card.tlsReality;
  return t.servers.card.tlsPlain;
}

function ServerCard({
  server,
  isSelected,
  isRunning,
  latencyMs,
  isTesting,
  pendingDelete,
  onTest,
  onDuplicate,
  onDelete,
  onDeleteBlur,
}: {
  server: ServerConfig;
  isSelected: boolean;
  isRunning: boolean;
  latencyMs: number | null | undefined;
  isTesting: boolean;
  pendingDelete: boolean;
  onTest: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onDeleteBlur: () => void;
}) {
  const { t } = useTranslation();

  return (
    <Card className={cn("flex flex-col gap-3 p-4", isSelected && "border-flow ring-1 ring-flow")}>
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn("h-2 w-2 shrink-0 rounded-full", isRunning ? "bg-ok" : "bg-fg-faint")}
            aria-hidden="true"
          />
          <p className="truncate font-semibold text-fg">{server.name}</p>
        </div>
        {latencyMs !== undefined && (
          <span
            className={cn(
              "shrink-0 font-mono text-xs font-medium",
              latencyMs === null ? "text-fg-faint" : latencyColorClass(latencyMs),
            )}
          >
            {latencyMs === null ? t.servers.card.latencyTimeout : `${latencyMs}ms`}
          </span>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <Badge variant={server.protocol}>{server.protocol}</Badge>
        {isSelected && <Badge variant="default">{t.servers.card.current}</Badge>}
      </div>

      <div className="min-w-0">
        <p className="truncate font-mono text-xs text-fg-faint">
          {server.address}:{server.port}
        </p>
        <p className="mt-0.5 text-xs text-fg-faint">{securityDetail(server, t)}</p>
      </div>

      <div className="mt-auto flex items-center justify-end gap-1 border-t border-line pt-2">
        <Button
          variant="ghost"
          size="icon"
          busy={isTesting}
          title={t.servers.card.testLatency}
          aria-label={t.servers.card.testLatency}
          onClick={onTest}
        >
          {!isTesting && <BoltIcon className="h-4 w-4" />}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title={t.servers.duplicate}
          aria-label={t.servers.duplicate}
          onClick={onDuplicate}
        >
          <DuplicateIcon className="h-4 w-4" />
        </Button>
        <Button
          variant={pendingDelete ? "destructive" : "ghost"}
          size="icon"
          title={pendingDelete ? t.servers.confirmDelete : t.servers.delete}
          aria-label={pendingDelete ? t.servers.confirmDelete : t.servers.delete}
          onClick={onDelete}
          onBlur={onDeleteBlur}
        >
          <TrashIcon className="h-4 w-4" />
        </Button>
      </div>
    </Card>
  );
}

type SortBy = "name" | "latency";
type ProtocolFilter = Protocol | "all";

export function ServersView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const proxyStatus = useAppStore((s) => s.proxyStatus);
  const deleteServer = useAppStore((s) => s.deleteServer);
  const duplicateServer = useAppStore((s) => s.duplicateServer);
  const registerWarp = useAppStore((s) => s.registerWarp);
  const warpBusy = useAppStore((s) => s.warpBusy);
  const latencyResults = useAppStore((s) => s.latencyResults);
  const latencyTestingIds = useAppStore((s) => s.latencyTestingIds);
  const latencyTestingAll = useAppStore((s) => s.latencyTestingAll);
  const testServerLatency = useAppStore((s) => s.testServerLatency);
  const testAllServerLatency = useAppStore((s) => s.testAllServerLatency);

  const [showForm, setShowForm] = useState(false);
  const [showImportForm, setShowImportForm] = useState(false);
  const [importInitialMode, setImportInitialMode] = useState<ImportMode>("url");
  const [showAddMenu, setShowAddMenu] = useState(false);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [protocolFilter, setProtocolFilter] = useState<ProtocolFilter>("all");
  const [sortBy, setSortBy] = useState<SortBy>("name");

  const addMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showAddMenu) return;
    function handleClick(e: MouseEvent) {
      if (addMenuRef.current && !addMenuRef.current.contains(e.target as Node)) {
        setShowAddMenu(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [showAddMenu]);

  const allServers = config?.servers ?? [];
  const selectedServerId = config?.selectedServerId ?? null;

  const servers = useMemo(() => {
    let list = allServers;
    if (protocolFilter !== "all") {
      list = list.filter((s) => s.protocol === protocolFilter);
    }
    const q = search.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (s) => s.name.toLowerCase().includes(q) || s.address.toLowerCase().includes(q),
      );
    }
    list = [...list];
    if (sortBy === "name") {
      list.sort((a, b) => a.name.localeCompare(b.name));
    } else {
      list.sort((a, b) => {
        const am = latencyResults[a.id];
        const bm = latencyResults[b.id];
        if (am == null && bm == null) return a.name.localeCompare(b.name);
        if (am == null) return 1;
        if (bm == null) return -1;
        return am - bm;
      });
    }
    return list;
  }, [allServers, protocolFilter, search, sortBy, latencyResults]);

  function handleDelete(id: string) {
    if (pendingDeleteId === id) {
      deleteServer(id);
      setPendingDeleteId(null);
    } else {
      setPendingDeleteId(id);
    }
  }

  function openManualAdd() {
    setShowAddMenu(false);
    setShowForm(true);
  }

  function openManualImport() {
    setShowAddMenu(false);
    setImportInitialMode("paste");
    setShowImportForm(true);
  }

  function openAddSubscription() {
    setShowAddMenu(false);
    setImportInitialMode("url");
    setShowImportForm(true);
  }

  function handleWarp() {
    setShowAddMenu(false);
    registerWarp();
  }

  const showingForm = showForm || showImportForm;

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="font-display text-xl font-semibold text-fg">{t.servers.title}</h1>
          <p className="mt-0.5 text-xs text-fg-faint">{t.servers.subtitle(allServers.length)}</p>
        </div>

        {!showingForm && (
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" busy={latencyTestingAll} onClick={testAllServerLatency}>
              {!latencyTestingAll && <BoltIcon className="h-3.5 w-3.5" />}
              {latencyTestingAll ? t.servers.testingAll : t.servers.testAll}
            </Button>

            <div className="relative" ref={addMenuRef}>
              <Button size="sm" onClick={() => setShowAddMenu((v) => !v)}>
                <PlusIcon className="h-3.5 w-3.5" />
                {t.servers.addMenu.button}
                <ChevronDownIcon className="h-3.5 w-3.5" />
              </Button>
              {showAddMenu && (
                <div
                  role="menu"
                  className="absolute right-0 top-full z-20 mt-1 w-56 overflow-hidden rounded-lg border border-line bg-surface py-1 shadow-lg"
                >
                  <button
                    type="button"
                    role="menuitem"
                    onClick={openManualAdd}
                    className="flex w-full items-center px-3 py-2 text-left text-sm text-fg-dim transition-colors hover:bg-surface-2 hover:text-fg"
                  >
                    {t.servers.addMenu.manualAdd}
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={openManualImport}
                    className="flex w-full items-center px-3 py-2 text-left text-sm text-fg-dim transition-colors hover:bg-surface-2 hover:text-fg"
                  >
                    {t.servers.addMenu.manualImport}
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={openAddSubscription}
                    className="flex w-full items-center px-3 py-2 text-left text-sm text-fg-dim transition-colors hover:bg-surface-2 hover:text-fg"
                  >
                    {t.servers.addMenu.addSubscription}
                  </button>
                  <div className="my-1 border-t border-line" />
                  <button
                    type="button"
                    role="menuitem"
                    disabled={warpBusy}
                    onClick={handleWarp}
                    className="flex w-full items-center px-3 py-2 text-left text-sm text-fg-dim transition-colors hover:bg-surface-2 hover:text-fg disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {warpBusy ? t.servers.addMenu.registeringWarp : t.servers.addMenu.getWarp}
                  </button>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {showForm && <ServerForm onDone={() => setShowForm(false)} />}
      {showImportForm && (
        <SubscriptionImportForm initialMode={importInitialMode} onDone={() => setShowImportForm(false)} />
      )}

      {!showingForm && allServers.length > 0 && (
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative flex-1">
            <SearchIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-faint" />
            <Input
              aria-label={t.servers.search.ariaLabel}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t.servers.search.placeholder}
              className="pl-9"
            />
          </div>
          <Select
            aria-label={t.servers.filter.ariaLabel}
            value={protocolFilter}
            onChange={(e) => setProtocolFilter(e.target.value as ProtocolFilter)}
            className="sm:w-40"
          >
            <option value="all">{t.servers.filter.all}</option>
            {PROTOCOLS.map((p) => (
              <option key={p} value={p} className="capitalize">
                {p}
              </option>
            ))}
          </Select>
          <Select
            aria-label={t.servers.sort.ariaLabel}
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as SortBy)}
            className="sm:w-52"
          >
            <option value="name">{t.servers.sort.nameAsc}</option>
            <option value="latency">{t.servers.sort.latencyAsc}</option>
          </Select>
        </div>
      )}

      {!showingForm &&
        (allServers.length === 0 ? (
          <Card>
            <CardContent className="text-sm text-fg-faint">{t.servers.empty}</CardContent>
          </Card>
        ) : servers.length === 0 ? (
          <Card>
            <CardContent className="text-sm text-fg-faint">{t.servers.noResults}</CardContent>
          </Card>
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {servers.map((server) => (
              <ServerCard
                key={server.id}
                server={server}
                isSelected={server.id === selectedServerId}
                isRunning={!!proxyStatus?.running && proxyStatus.currentServerId === server.id}
                latencyMs={latencyResults[server.id]}
                isTesting={latencyTestingIds.has(server.id)}
                pendingDelete={pendingDeleteId === server.id}
                onTest={() => testServerLatency(server.id)}
                onDuplicate={() => duplicateServer(server.id)}
                onDelete={() => handleDelete(server.id)}
                onDeleteBlur={() => setPendingDeleteId(null)}
              />
            ))}
          </div>
        ))}
    </div>
  );
}
