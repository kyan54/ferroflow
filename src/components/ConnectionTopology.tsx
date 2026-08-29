import { useEffect, useMemo, useRef, useState } from "react";
import type { SVGProps } from "react";
import { Card, CardHeader, CardTitle, CardContent } from "./ui";
import { formatBytes } from "../lib/utils";
import { buildConnectionTopology, type TopologyNodeKind } from "../lib/connectionTopology";
import { computeSankeyLayout, NODE_WIDTH } from "../lib/sankeyLayout";
import type { ConnectionsSnapshot } from "../types";

const CANVAS_HEIGHT = 220;
const PADDING_X = 12;
/** The device and destination columns sit at the outer edges of the canvas,
 * so (unlike the middle exit column, whose label can sit to the left over
 * the incoming ribbon -- there's always a device node behind it) they have
 * nothing behind them to absorb a label. Each gets a dedicated blank margin
 * sized for its longest label ("Your device" on the left; domains/IPs,
 * generally the longest labels in the diagram, on the right). */
const LEFT_LABEL_RESERVE = 76;
const RIGHT_LABEL_RESERVE = 128;

/** Solid node fill + translucent ribbon fill per node kind, as literal
 * Tailwind class strings (Tailwind's content scan needs the full class name
 * present in source, not assembled via template interpolation) -- same
 * `Record<Kind, string>` shape as `Badge.tsx`'s `VARIANT_CLASSES`. Proxy
 * reuses the brand "flow" hue already used for it elsewhere (Badge's
 * `default` variant, `RulesView`'s `OUTBOUND_BADGE.proxy`); Direct reuses
 * the neutral "secondary" hue (`OUTBOUND_BADGE.direct`); Block reuses the
 * destructive hue (`OUTBOUND_BADGE.block`) -- so this diagram's colors read
 * as the same proxy/direct/block vocabulary as the rest of the app instead
 * of inventing a new palette. */
const NODE_FILL: Record<TopologyNodeKind, string> = {
  device: "fill-fg-faint",
  proxy: "fill-flow",
  direct: "fill-fg-dim",
  block: "fill-err",
  destination: "fill-fg-dim",
  other: "fill-fg-faint",
};

const LINK_FILL: Record<TopologyNodeKind, string> = {
  device: "fill-fg-faint/30",
  proxy: "fill-flow/35",
  direct: "fill-fg-dim/25",
  block: "fill-err/35",
  destination: "fill-fg-dim/25",
  other: "fill-fg-faint/25",
};

function truncateLabel(label: string, max = 18): string {
  return label.length > max ? `${label.slice(0, max - 1)}…` : label;
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

export interface ConnectionTopologyProps {
  snapshot: ConnectionsSnapshot | null;
  running: boolean;
}

/** Sankey-style visualization of where live traffic is going: device -> exit
 * node (Proxy/Direct/Blocked) -> destination host, ribbon width proportional
 * to bytes transferred. Purely derived from the `ConnectionsSnapshot`
 * `DashboardView` already polls every 2s (see `refreshConnections`) --
 * no dedicated backend command, this just re-shapes data already on screen
 * elsewhere (`ConnectionsView`'s table). */
export function ConnectionTopology({ snapshot, running }: ConnectionTopologyProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setWidth(el.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const topology = useMemo(() => buildConnectionTopology(snapshot), [snapshot]);

  const layout = useMemo(() => {
    if (width === 0 || topology.nodes.length === 0) return null;
    const hasDestinations = topology.nodes.some((n) => n.kind === "destination" || n.kind === "other");
    const columnX = hasDestinations
      ? [LEFT_LABEL_RESERVE, width * 0.46, width - PADDING_X - RIGHT_LABEL_RESERVE]
      : [LEFT_LABEL_RESERVE, width - PADDING_X - RIGHT_LABEL_RESERVE];
    const column: Record<string, number> = {};
    topology.nodes.forEach((n) => {
      if (n.kind === "device") column[n.id] = 0;
      else if (n.kind === "destination" || n.kind === "other") column[n.id] = columnX.length - 1;
      else column[n.id] = 1;
    });
    return computeSankeyLayout(
      topology.nodes.map((n) => ({ id: n.id, value: n.value, column: column[n.id] })),
      topology.links,
      columnX,
      CANVAS_HEIGHT,
    );
  }, [topology, width]);

  const nodeMeta = useMemo(() => new Map(topology.nodes.map((n) => [n.id, n])), [topology.nodes]);
  const linkKind = useMemo(() => new Map(topology.links.map((l) => [`${l.sourceId} ${l.targetId}`, l.kind])), [
    topology.links,
  ]);
  const lastColumnX = layout ? Math.max(...layout.nodes.map((n) => n.column)) : 0;

  const emptyMessage = !running
    ? "Start the proxy to see live traffic flow."
    : (snapshot?.connections.length ?? 0) === 0
      ? "No active connections."
      : "Connections are open but idle — waiting for traffic.";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Traffic flow</CardTitle>
      </CardHeader>
      <CardContent className="pt-4">
        <div ref={containerRef} className="w-full" style={{ height: CANVAS_HEIGHT }}>
          {!layout ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-fg-faint">
              <NetworkIcon className="h-7 w-7 opacity-50" />
              <span>{emptyMessage}</span>
            </div>
          ) : (
            <svg width="100%" height={CANVAS_HEIGHT} viewBox={`0 0 ${width} ${CANVAS_HEIGHT}`}>
              {layout.links.map((link, i) => {
                const kind = linkKind.get(`${link.sourceId} ${link.targetId}`) ?? "direct";
                return (
                  <path key={`${link.sourceId}-${link.targetId}-${i}`} d={link.path} className={LINK_FILL[kind]} />
                );
              })}
              {layout.nodes.map((node) => {
                const meta = nodeMeta.get(node.id);
                if (!meta) return null;
                // Every column except the last labels to the left of its
                // bar: for the device column that lands in `LEFT_LABEL_
                // RESERVE`'s blank margin, for the exit column it lands
                // over the (translucent) incoming ribbon instead -- both
                // read fine since the ribbon fill is low-opacity. Only the
                // last column flips sides, into its own reserved margin.
                const isLastColumn = node.column === lastColumnX;
                const labelX = isLastColumn ? NODE_WIDTH + 8 : -8;
                const labelAnchor = isLastColumn ? "start" : "end";
                const valueX = isLastColumn ? -6 : NODE_WIDTH + 6;
                const valueAnchor = isLastColumn ? "end" : "start";
                return (
                  <g key={node.id} transform={`translate(${node.x}, ${node.y})`}>
                    <title>{`${meta.label} · ${formatBytes(meta.value)}`}</title>
                    <rect width={NODE_WIDTH} height={node.height} rx={1.5} className={NODE_FILL[meta.kind]} />
                    <text
                      x={labelX}
                      y={node.height / 2}
                      dy=".32em"
                      textAnchor={labelAnchor}
                      className="select-none text-[11px] font-medium fill-fg"
                    >
                      {truncateLabel(meta.label)}
                    </text>
                    {node.height >= 12 && (
                      <text
                        x={valueX}
                        y={node.height / 2}
                        dy=".32em"
                        textAnchor={valueAnchor}
                        className="select-none text-[9px] fill-fg-faint"
                      >
                        {formatBytes(meta.value)}
                      </text>
                    )}
                  </g>
                );
              })}
            </svg>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
