// Aggregates the already-polled `ConnectionsSnapshot` (see
// `store.ts#refreshConnections`, driven by `DashboardView`'s 2s poll) into
// the node/link shape `ConnectionTopology` renders as a Sankey diagram.
// Pure and framework-free by design -- no new backend command needed, since
// every field this reads (`chains`, `metadata`, `download`/`upload`) is
// already on `ConnectionInfo`.

import type { ConnectionInfo, ConnectionsSnapshot } from "../types";

export type TopologyNodeKind = "device" | "proxy" | "direct" | "block" | "destination" | "other";

export interface TopologyNode {
  id: string;
  label: string;
  /** Total bytes (download + upload) flowing through this node. */
  value: number;
  kind: TopologyNodeKind;
}

export interface TopologyLink {
  sourceId: string;
  targetId: string;
  value: number;
  /** Which color a link renders as -- always its exit-node side, so a
   * destination fed by both Proxy and Direct still shows two distinctly
   * colored ribbons converging on it. */
  kind: Exclude<TopologyNodeKind, "device" | "destination" | "other">;
}

export interface TopologyData {
  nodes: TopologyNode[];
  links: TopologyLink[];
}

export const EMPTY_TOPOLOGY: TopologyData = { nodes: [], links: [] };

/** Cap on destination nodes before the long tail gets folded into "Other" --
 * keeps the diagram legible with hundreds of raw connections. */
const MAX_DESTINATIONS = 8;

const OTHER_DESTINATION_ID = "dest:__other__";

const EXIT_LABELS: Record<string, string> = {
  proxy: "Proxy",
  direct: "Direct",
  block: "Blocked",
};

function exitKind(tag: string): Exclude<TopologyNodeKind, "device" | "destination" | "other"> {
  if (tag === "proxy") return "proxy";
  if (tag === "block") return "block";
  return "direct";
}

/** Outbound tag a connection actually exited through. `chains` is "outermost
 * first" (see `ConnectionMetadata` doc comment in shared-types); ferroflow
 * never nests outbounds behind a selector today (see
 * `core-manager::config`'s `PROXY_OUTBOUND_TAG`/`DIRECT_OUTBOUND_TAG`/
 * `BLOCK_OUTBOUND_TAG`), so `chains[0]` and the final hop are the same tag.
 * Falls back to "direct" for the (currently unreachable) empty-chain case,
 * matching the reference Electron app's aggregation. */
function exitTag(conn: ConnectionInfo): string {
  return conn.chains[0] || "direct";
}

function exitLabel(tag: string): string {
  return EXIT_LABELS[tag] ?? tag.charAt(0).toUpperCase() + tag.slice(1);
}

/** Display name for a connection's destination -- domain when sing-box
 * resolved one (SNI/Host header), else the raw IP:port. Mirrors
 * `ConnectionsView`'s `destinationLabel`. */
function destinationName(conn: ConnectionInfo): string {
  const { host, destinationIP, destinationPort } = conn.metadata;
  if (host) return host;
  if (destinationIP) return `${destinationIP}:${destinationPort}`;
  return "Unknown";
}

export function buildConnectionTopology(snapshot: ConnectionsSnapshot | null): TopologyData {
  const connections = snapshot?.connections ?? [];

  const exitTotals = new Map<string, number>();
  const destTotals = new Map<string, number>();
  const deviceToExit = new Map<string, number>();
  // Keyed by "<exitTag> <destName>" -- space can't appear in either half
  // (tags are identifiers, dest names are hostnames/IPs), so it's a safe
  // separator without needing a dedicated tuple type.
  const exitToDest = new Map<string, number>();
  let deviceTotal = 0;

  for (const conn of connections) {
    const bytes = conn.download + conn.upload;
    // A just-opened connection with no bytes yet would render as an
    // invisible sliver anyway; excluding it also means a proxy that's
    // running but genuinely idle naturally aggregates to an empty
    // topology, which `ConnectionTopology` treats as the idle empty state.
    if (bytes <= 0) continue;

    const tag = exitTag(conn);
    const dest = destinationName(conn);

    deviceTotal += bytes;
    exitTotals.set(tag, (exitTotals.get(tag) ?? 0) + bytes);
    destTotals.set(dest, (destTotals.get(dest) ?? 0) + bytes);
    deviceToExit.set(tag, (deviceToExit.get(tag) ?? 0) + bytes);
    const key = `${tag} ${dest}`;
    exitToDest.set(key, (exitToDest.get(key) ?? 0) + bytes);
  }

  if (deviceTotal === 0) return EMPTY_TOPOLOGY;

  const sortedDests = Array.from(destTotals.entries()).sort((a, b) => b[1] - a[1]);
  const topDests = sortedDests.slice(0, MAX_DESTINATIONS);
  const overflowDests = sortedDests.slice(MAX_DESTINATIONS);
  const overflowNames = new Set(overflowDests.map(([name]) => name));
  const overflowTotal = overflowDests.reduce((sum, [, value]) => sum + value, 0);

  const nodes: TopologyNode[] = [
    { id: "device", label: "Your device", value: deviceTotal, kind: "device" },
  ];

  const sortedExitTags = Array.from(exitTotals.keys()).sort(
    (a, b) => exitTotals.get(b)! - exitTotals.get(a)!,
  );
  for (const tag of sortedExitTags) {
    nodes.push({
      id: `exit:${tag}`,
      label: exitLabel(tag),
      value: exitTotals.get(tag)!,
      kind: exitKind(tag),
    });
  }

  for (const [name, value] of topDests) {
    nodes.push({ id: `dest:${name}`, label: name, value, kind: "destination" });
  }
  if (overflowTotal > 0) {
    nodes.push({
      id: OTHER_DESTINATION_ID,
      label: `Other (${overflowDests.length})`,
      value: overflowTotal,
      kind: "other",
    });
  }

  const links: TopologyLink[] = [];
  for (const [tag, value] of deviceToExit) {
    links.push({ sourceId: "device", targetId: `exit:${tag}`, value, kind: exitKind(tag) });
  }

  const foldedExitToDest = new Map<string, number>();
  for (const [key, value] of exitToDest) {
    const [tag, dest] = key.split(" ");
    const destId = overflowNames.has(dest) ? OTHER_DESTINATION_ID : `dest:${dest}`;
    const foldedKey = `${tag} ${destId}`;
    foldedExitToDest.set(foldedKey, (foldedExitToDest.get(foldedKey) ?? 0) + value);
  }
  for (const [key, value] of foldedExitToDest) {
    const [tag, destId] = key.split(" ");
    links.push({ sourceId: `exit:${tag}`, targetId: destId, value, kind: exitKind(tag) });
  }

  return { nodes, links };
}
