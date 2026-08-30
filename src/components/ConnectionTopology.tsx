import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { SVGProps } from "react";
import { Card, CardHeader, CardTitle, CardContent, Input } from "./ui";
import { useTranslation } from "../i18n";
import { useAppStore } from "../store";
import {
  buildConnectionTopology,
  DEVICE_LABEL,
  OTHERS_LABEL,
  EXIT_LABEL,
  type TopologyNode,
  type TopologyNodeKind,
  type TopologyRuleTarget,
} from "../lib/connectionTopology";
import {
  computeTopologyLayout,
  collectLinkedIds,
  hitBox,
  NODE_WIDTH,
  type SankeyLayoutLink,
} from "../lib/sankeyLayout";
import type { RoutingRule, RuleOutbound } from "../types";
import type { Dictionary } from "../i18n";

const CANVAS_HEIGHT = 340;
const PADDING_X = 12;
/** The device and exit-node columns sit at the outer edges of the canvas,
 * so (unlike the host column, whose label can sit to the left over the
 * incoming ribbon -- there's always a device node behind it) they have
 * nothing behind them to absorb a label. Each gets a dedicated blank margin
 * sized for its longest label ("My device" on the left; exit-node names,
 * possibly a full server name, on the right). */
const LEFT_LABEL_RESERVE = 76;
const RIGHT_LABEL_RESERVE = 148;
const LABEL_MAX_CHARS = 22;

const NODE_FILL: Record<TopologyNodeKind, string> = {
  device: "fill-flow",
  host: "fill-ok",
  other: "fill-fg-faint",
  outbound: "fill-warn",
};

function truncateLabel(label: string, max = LABEL_MAX_CHARS): string {
  return label.length > max ? `${label.slice(0, max - 1)}…` : label;
}

/** Swaps a node's sentinel label (see `connectionTopology.ts`) for localized
 * or dynamic display text -- keeps that module i18n-free while still
 * letting the running server's actual name show up as the "proxy" exit
 * node, the way the reference FlowZ app's screenshot shows a named server
 * ("US_Direct") rather than a generic "Proxy" label. */
function displayLabel(label: string, t: Dictionary, serverName: string | null): string {
  if (label === DEVICE_LABEL) return t.dashboard.trafficFlow.myDevice;
  if (label === OTHERS_LABEL) return t.dashboard.trafficFlow.others;
  if (label === EXIT_LABEL.proxy) return serverName ?? t.dashboard.trafficFlow.exitLabels.proxy;
  if (label === EXIT_LABEL.direct) return t.dashboard.trafficFlow.exitLabels.direct;
  if (label === EXIT_LABEL.block) return t.dashboard.trafficFlow.exitLabels.block;
  return label;
}

function kindLabel(kind: TopologyNodeKind, t: Dictionary): string {
  if (kind === "device") return t.dashboard.trafficFlow.kindLabels.device;
  if (kind === "outbound") return t.dashboard.trafficFlow.kindLabels.outbound;
  return t.dashboard.trafficFlow.kindLabels.host;
}

function isIPv6(ip: string): boolean {
  return ip.includes(":");
}

/** Bare IP -> CIDR literal a `RuleMatchType.IpCidr` rule can use -- ferroflow
 * (like the sing-box config it generates, see `core-manager::config.rs`)
 * has no validation of its own on rule values, so an un-suffixed IP would
 * silently fail to match at the sing-box layer instead of erroring here. */
function toCidr(ip: string): string {
  return isIPv6(ip) ? `${ip}/128` : `${ip}/32`;
}

function newRuleId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `rule-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Whether `rules` already has a domain-family/`ipCidr` condition containing
 * this exact value -- a minimal "already covered" guard (exact-value match,
 * not full domainSuffix-subsumes-subdomain reasoning) so right-clicking the
 * same domain twice doesn't pile up duplicate rules. */
function alreadyRuled(rules: RoutingRule[], matchType: RoutingRule["matchType"], value: string): boolean {
  const v = value.trim().toLowerCase();
  return rules.some((r) => r.matchType === matchType && r.values.some((x) => x.trim().toLowerCase() === v));
}

function NetworkIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      <rect x="3" y="16" width="6" height="5" rx="1" />
      <rect x="15" y="16" width="6" height="5" rx="1" />
      <rect x="9" y="3" width="6" height="5" rx="1" />
      <path d="M12 8v4m0 0H6v4m6-4h6v4" />
    </svg>
  );
}

function SearchIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round" {...props}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  );
}

interface ContextMenuState {
  x: number;
  y: number;
  nodeId: string;
  label: string;
  ruleTarget: TopologyRuleTarget;
}

/** Sankey-style visualization of where live traffic is going: device -> host
 * (domain/IP) -> exit node, ribbon width proportional to connection count.
 * Ported from the reference FlowZ Electron app's own "连接拓扑" page (see
 * `connectionTopology.ts`'s doc comment for the full data-source rationale)
 * -- purely derived from the `connectionsSnapshot` `DashboardView` already
 * polls every 2s, no dedicated backend command. Self-sufficient via the
 * store (like the reference component) rather than prop-drilled, so it can
 * look up the running server's name and existing rules for the right-click
 * "add rule" menu without `DashboardView` having to plumb them through. */
export function ConnectionTopology() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const proxyStatus = useAppStore((s) => s.proxyStatus);
  const connectionsSnapshot = useAppStore((s) => s.connectionsSnapshot);
  const addRule = useAppStore((s) => s.addRule);

  const running = proxyStatus?.running ?? false;
  const runningServerName = useMemo(
    () => config?.servers.find((s) => s.id === proxyStatus?.currentServerId)?.name ?? null,
    [config, proxyStatus],
  );
  const rules = config?.rules ?? [];

  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [search, setSearch] = useState("");
  const [hovered, setHovered] = useState<{ type: "node" | "link"; id: string } | null>(null);
  const [mousePos, setMousePos] = useState({ x: 0, y: 0 });
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [tooltipSize, setTooltipSize] = useState({ w: 0, h: 0 });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setWidth(el.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const topology = useMemo(() => buildConnectionTopology(connectionsSnapshot), [connectionsSnapshot]);

  // Resolve sentinel labels once here so downstream layout/search/tooltip
  // code only ever deals in real display strings.
  const nodeMeta = useMemo(() => {
    const map = new Map<string, TopologyNode & { displayLabel: string }>();
    for (const n of topology.nodes) {
      map.set(n.id, { ...n, displayLabel: displayLabel(n.label, t, runningServerName) });
    }
    return map;
  }, [topology, t, runningServerName]);

  const layout = useMemo(() => {
    if (width === 0 || topology.nodes.length === 0) return null;
    const columnOf: Record<TopologyNodeKind, 0 | 1 | 2> = { device: 0, host: 1, other: 1, outbound: 2 };
    const columnX: [number, number, number] = [
      LEFT_LABEL_RESERVE,
      width * 0.46,
      width - PADDING_X - RIGHT_LABEL_RESERVE,
    ];
    return computeTopologyLayout(
      topology.nodes.map((n) => ({ id: n.id, value: n.value, column: columnOf[n.kind] })),
      topology.links,
      columnX,
      CANVAS_HEIGHT,
    );
  }, [topology, width]);

  const lastColumn = layout ? Math.max(...layout.nodes.map((n) => n.column)) : 0;

  // Case-insensitive substring match against display labels -- device
  // excluded (it's the anchor, not a search target), matching the
  // reference app's own `matchNodeIds`.
  const searching = search.trim().length > 0;
  const searchMatches = useMemo(() => {
    if (!searching || !layout) return [];
    const q = search.trim().toLowerCase();
    return layout.nodes.filter((n) => n.column !== 0 && (nodeMeta.get(n.id)?.displayLabel ?? "").toLowerCase().includes(q)).map((n) => n.id);
  }, [layout, nodeMeta, search, searching]);

  const highlightedIds = useMemo(() => {
    if (!layout) return new Set<string>();
    let focus: string[] = [];
    if (hovered) {
      if (hovered.type === "node") {
        focus = [hovered.id];
      } else {
        const link = layout.links[Number(hovered.id.split("-")[1])];
        if (link) focus = [link.sourceId, link.targetId];
      }
    } else if (searching) {
      focus = searchMatches;
    }
    return collectLinkedIds(layout.links, focus);
  }, [layout, hovered, searching, searchMatches]);

  const dimming = hovered !== null || searching;
  const nodeOpacity = (id: string) => (!dimming ? 1 : highlightedIds.has(id) ? 1 : 0.12);
  const linkOpacity = (i: number) => (!dimming ? 0.4 : highlightedIds.has(`link-${i}`) ? 0.85 : 0.05);

  useLayoutEffect(() => {
    if (!hovered || !tooltipRef.current) return;
    const r = tooltipRef.current.getBoundingClientRect();
    setTooltipSize((prev) => (prev.w === r.width && prev.h === r.height ? prev : { w: r.width, h: r.height }));
  }, [hovered]);

  const tooltipPos = useMemo(() => {
    const OFFSET = 12;
    const PAD = 6;
    const b = containerRef.current?.getBoundingClientRect();
    const maxLeft = b ? b.width - PAD - tooltipSize.w : mousePos.x;
    const maxTop = b ? b.height - PAD - tooltipSize.h : mousePos.y;
    const flipX = mousePos.x + OFFSET + tooltipSize.w > (b?.width ?? Infinity) - PAD;
    const flipY = mousePos.y + OFFSET + tooltipSize.h > (b?.height ?? Infinity) - PAD;
    const left = flipX ? mousePos.x - OFFSET - tooltipSize.w : mousePos.x + OFFSET;
    const top = flipY ? mousePos.y - OFFSET - tooltipSize.h : mousePos.y + OFFSET;
    return { left: Math.min(Math.max(PAD, left), Math.max(PAD, maxLeft)), top: Math.min(Math.max(PAD, top), Math.max(PAD, maxTop)) };
  }, [mousePos, tooltipSize]);

  function handleMouseMove(e: React.MouseEvent) {
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setMousePos({ x: e.clientX - rect.left, y: e.clientY - rect.top });
  }

  function tooltipContent() {
    if (!hovered || !layout) return null;
    if (hovered.type === "node") {
      const meta = nodeMeta.get(hovered.id);
      if (!meta) return null;
      return (
        <div className="rounded-md border border-line bg-surface-2 px-3 py-2 text-xs shadow-lg">
          <div className="mb-1 max-w-[200px] truncate font-semibold text-fg">{meta.displayLabel}</div>
          <div className="text-fg-faint">
            {t.dashboard.trafficFlow.tooltip.type}: {kindLabel(meta.kind, t)}
          </div>
          <div className="text-fg-faint">{t.dashboard.trafficFlow.tooltip.connections(meta.value)}</div>
        </div>
      );
    }
    const link = layout.links[Number(hovered.id.split("-")[1])];
    if (!link) return null;
    const sourceLabel = nodeMeta.get(link.sourceId)?.displayLabel ?? link.sourceId;
    const targetLabel = nodeMeta.get(link.targetId)?.displayLabel ?? link.targetId;
    return (
      <div className="rounded-md border border-line bg-surface-2 px-3 py-2 text-xs shadow-lg">
        <div className="mb-1 flex items-center gap-1 font-semibold text-fg">
          <span className="max-w-[100px] truncate">{sourceLabel}</span>
          <span className="text-fg-faint">→</span>
          <span className="max-w-[100px] truncate">{targetLabel}</span>
        </div>
        <div className="text-fg-faint">{t.dashboard.trafficFlow.tooltip.connections(link.value)}</div>
      </div>
    );
  }

  function handleNodeContextMenu(e: React.MouseEvent, node: TopologyNode & { displayLabel: string }) {
    if (node.kind !== "host" || !node.ruleTarget) return;
    e.preventDefault();
    e.stopPropagation();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setHovered(null);
    setContextMenu({
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
      nodeId: node.id,
      label: node.displayLabel,
      ruleTarget: node.ruleTarget,
    });
  }

  useEffect(() => {
    if (!contextMenu) return;
    function onDocMouseDown(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setContextMenu(null);
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setContextMenu(null);
    }
    document.addEventListener("mousedown", onDocMouseDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onDocMouseDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [contextMenu]);

  const menuPos = useMemo(() => {
    if (!contextMenu) return { left: 0, top: 0 };
    const b = containerRef.current?.getBoundingClientRect();
    const menuW = menuRef.current?.offsetWidth ?? 200;
    const menuH = menuRef.current?.offsetHeight ?? 160;
    const maxLeft = b ? Math.max(0, b.width - menuW) : contextMenu.x;
    const maxTop = b ? Math.max(0, b.height - menuH) : contextMenu.y;
    return { left: Math.min(contextMenu.x, maxLeft), top: Math.min(contextMenu.y, maxTop) };
  }, [contextMenu]);

  async function addDomainRule(target: TopologyRuleTarget, label: string, outbound: RuleOutbound) {
    setContextMenu(null);
    const matchType = target.kind === "domain" ? "domainSuffix" : "ipCidr";
    const value = target.kind === "domain" ? target.value : toCidr(target.value);
    if (alreadyRuled(rules, matchType, value)) {
      useAppStore.getState().pushToast("info", t.toasts.domainAlreadyInRule(target.value));
      return;
    }
    const rule: RoutingRule = {
      id: newRuleId(),
      name: label,
      enabled: true,
      matchType,
      values: [value],
      outbound,
    };
    await addRule(rule);
  }

  const emptyMessage = !running ? t.dashboard.trafficFlow.emptyNotRunning : t.dashboard.trafficFlow.emptyNoConnections;

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-col gap-0.5">
          <CardTitle>{t.dashboard.trafficFlow.title}</CardTitle>
          <p className="text-xs text-fg-faint">{t.dashboard.trafficFlow.hint}</p>
        </div>
        <div className="relative w-44 shrink-0 sm:w-56">
          <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-faint" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t.dashboard.trafficFlow.searchPlaceholder}
            aria-label={t.dashboard.trafficFlow.searchAriaLabel}
            className="pl-8 text-xs"
          />
        </div>
      </CardHeader>
      <CardContent className="pt-4">
        <div
          ref={containerRef}
          className="relative w-full cursor-default"
          style={{ height: CANVAS_HEIGHT }}
          onMouseMove={handleMouseMove}
          onMouseLeave={() => setHovered(null)}
        >
          {!layout ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-fg-faint">
              <NetworkIcon className="h-7 w-7 opacity-50" />
              <span>{emptyMessage}</span>
            </div>
          ) : (
            <>
              {searching && searchMatches.length === 0 && (
                <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center">
                  <span className="rounded-md bg-surface-2/90 px-3 py-1.5 text-xs text-fg-faint">
                    {t.dashboard.trafficFlow.searchNoMatch}
                  </span>
                </div>
              )}

              {hovered && !contextMenu && (
                <div ref={tooltipRef} className="pointer-events-none absolute z-20" style={tooltipPos}>
                  {tooltipContent()}
                </div>
              )}

              {contextMenu && (
                <div
                  ref={menuRef}
                  className="absolute z-30 w-[200px] rounded-lg border border-line bg-surface shadow-lg"
                  style={menuPos}
                >
                  <div className="border-b border-line px-3 py-2">
                    <p className="max-w-[176px] truncate text-sm font-medium text-fg" title={contextMenu.label}>
                      {contextMenu.label}
                    </p>
                    <p className="text-xs text-fg-faint">{t.dashboard.trafficFlow.contextMenu.addRuleHint}</p>
                  </div>
                  <div className="py-1">
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-fg hover:bg-surface-2"
                      onClick={() => void addDomainRule(contextMenu.ruleTarget, contextMenu.label, "proxy")}
                    >
                      <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-flow" />
                      {t.dashboard.trafficFlow.contextMenu.proxy}
                    </button>
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-fg hover:bg-surface-2"
                      onClick={() => void addDomainRule(contextMenu.ruleTarget, contextMenu.label, "direct")}
                    >
                      <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-fg-faint" />
                      {t.dashboard.trafficFlow.contextMenu.direct}
                    </button>
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-err hover:bg-surface-2"
                      onClick={() => void addDomainRule(contextMenu.ruleTarget, contextMenu.label, "block")}
                    >
                      <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-err" />
                      {t.dashboard.trafficFlow.contextMenu.block}
                    </button>
                  </div>
                </div>
              )}

              <svg width="100%" height={CANVAS_HEIGHT} viewBox={`0 0 ${width} ${CANVAS_HEIGHT}`}>
                <defs>
                  <linearGradient id="topology-gradient-in" gradientUnits="userSpaceOnUse" x1="0" x2={width * 0.46} y1="0" y2="0">
                    <stop offset="0%" stopColor="hsl(var(--flow))" stopOpacity="0.45" />
                    <stop offset="100%" stopColor="hsl(var(--ok))" stopOpacity="0.45" />
                  </linearGradient>
                  <linearGradient id="topology-gradient-out" gradientUnits="userSpaceOnUse" x1={width * 0.46} x2={width} y1="0" y2="0">
                    <stop offset="0%" stopColor="hsl(var(--ok))" stopOpacity="0.45" />
                    <stop offset="100%" stopColor="hsl(var(--warn))" stopOpacity="0.45" />
                  </linearGradient>
                </defs>

                {layout.links.map((link: SankeyLayoutLink, i) => (
                  <path
                    key={`${link.sourceId}-${link.targetId}-${i}`}
                    d={link.path}
                    fill={link.stage === "in" ? "url(#topology-gradient-in)" : "url(#topology-gradient-out)"}
                    opacity={linkOpacity(i)}
                    className="transition-opacity duration-200"
                    onMouseEnter={() => setHovered({ type: "link", id: `link-${i}` })}
                  />
                ))}

                {layout.nodes.map((node) => {
                  const meta = nodeMeta.get(node.id);
                  if (!meta) return null;
                  const isLastColumn = node.column === lastColumn;
                  const labelX = isLastColumn ? NODE_WIDTH + 8 : -8;
                  const labelAnchor = isLastColumn ? "start" : "end";
                  const valueX = isLastColumn ? -6 : NODE_WIDTH + 6;
                  const valueAnchor = isLastColumn ? "end" : "start";
                  const box = hitBox(node, !isLastColumn);
                  const ruleable = meta.kind === "host";
                  return (
                    <g
                      key={node.id}
                      transform={`translate(${node.x}, ${node.y})`}
                      opacity={nodeOpacity(node.id)}
                      className="transition-opacity duration-200"
                      style={ruleable ? { cursor: "context-menu" } : undefined}
                      onMouseEnter={() => setHovered({ type: "node", id: node.id })}
                      onContextMenu={(e) => handleNodeContextMenu(e, meta)}
                    >
                      <rect width={NODE_WIDTH} height={node.height} rx={1.5} className={NODE_FILL[meta.kind]} />
                      <text
                        x={labelX}
                        y={node.height / 2}
                        dy=".32em"
                        textAnchor={labelAnchor}
                        className="pointer-events-none select-none text-[11px] font-medium fill-fg"
                      >
                        {truncateLabel(meta.displayLabel)}
                      </text>
                      <text
                        x={valueX}
                        y={node.height / 2}
                        dy=".32em"
                        textAnchor={valueAnchor}
                        className="pointer-events-none select-none text-[9px] fill-fg-faint"
                      >
                        {meta.value}
                      </text>
                      <rect x={box.x} y={box.y} width={box.width} height={box.height} fill="transparent" />
                    </g>
                  );
                })}
              </svg>
            </>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
