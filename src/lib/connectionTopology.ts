// Aggregates the already-polled `ConnectionsSnapshot` (see
// `store.ts#refreshConnections`, driven by `DashboardView`'s 2s poll) into
// the node/link shape `ConnectionTopology` renders as a Sankey diagram.
// Pure and framework-free by design (no i18n, no React) -- no new backend
// command needed, since every field this reads (`chains`, `metadata`) is
// already on `ConnectionInfo`.
//
// Shape and semantics are ported from the reference FlowZ Electron app's
// own aggregation (`src/main/services/connections-aggregate.ts`), found in
// the sibling `FlowZ` checkout: device -> host (domain/IP) -> exit node,
// counted by *number of concurrently open connections*, not bytes
// transferred -- matching the small integer counts in the reference app's
// "连接拓扑" screenshot (e.g. "www.nodeseek.com — 4"). This also answers
// where the data comes from: the reference app's own `StatsService` deletes
// a connection from its tracking map the instant it closes
// (`connMap.delete(id)` on the `CLOSED` event), so its topology is *only
// ever* the current live snapshot too -- never persisted history. A diagram
// with 15+ distinct hosts is just what a real browsing session's
// concurrently-open connections (keep-alives, background sync, etc.) looks
// like; it doesn't require ferroflow's opt-in "connection history" feature
// at all.

import type { ConnectionInfo, ConnectionsSnapshot } from "../types";

export type TopologyNodeKind = "device" | "host" | "other" | "outbound";

/** What a rule targeting this node would need to match on -- `null` for
 * nodes a rule can't be built from (the device node, exit nodes, and the
 * aggregated "Other" bucket, which has no single value to target). */
export type TopologyRuleTarget = { kind: "domain"; value: string } | { kind: "ip"; value: string };

export interface TopologyNode {
  id: string;
  /** Either a literal display string (a real host name, IP, or outbound
   * tag) or one of the sentinel constants below -- `ConnectionTopology`
   * resolves sentinels to localized/dynamic text at render time, keeping
   * this module i18n-free. */
  label: string;
  /** Connection count flowing through this node. */
  value: number;
  kind: TopologyNodeKind;
  ruleTarget: TopologyRuleTarget | null;
}

export interface TopologyLink {
  sourceId: string;
  targetId: string;
  value: number;
}

export interface TopologyData {
  nodes: TopologyNode[];
  links: TopologyLink[];
}

export const EMPTY_TOPOLOGY: TopologyData = { nodes: [], links: [] };

/** Sentinel labels `ConnectionTopology` swaps for localized/dynamic text.
 * `\u0000`-prefixed so a real host/outbound-tag name can never collide with
 * one by coincidence (same trick as the reference app's
 * `TOPOLOGY_OTHERS_KEY`). */
export const DEVICE_LABEL = "\u0000device";
export const OTHERS_LABEL = "\u0000others";
export const EXIT_LABEL: Record<"proxy" | "direct" | "block", string> = {
  proxy: "\u0000exit:proxy",
  direct: "\u0000exit:direct",
  block: "\u0000exit:block",
};

/** Host column cap before the long tail folds into "Other" -- matches the
 * reference FlowZ app's own `TOPOLOGY_TOP_N`
 * (`main/services/connections-aggregate.ts`), not an arbitrary "top 8":
 * a busy browsing session routinely keeps well more than 8 concurrent named
 * connections open, and the reference app only lumps the genuine long tail
 * past 15 into "其他"/"Other". */
const TOP_N_HOSTS = 15;

const OTHER_HOST_ID = "host:__other__";

/** Outbound tag a connection is routed through. `chains` is "outermost
 * first" (see `ConnectionMetadata` doc comment in shared-types); ferroflow
 * never nests outbounds behind a selector today (see
 * `core-manager::config`'s `PROXY_OUTBOUND_TAG`/`DIRECT_OUTBOUND_TAG`/
 * `BLOCK_OUTBOUND_TAG`), so `chains[0]` and the final hop are the same tag.
 * Falls back to "direct" for the (currently unreachable) empty-chain case,
 * matching the reference app's own fallback. */
function exitTag(conn: ConnectionInfo): string {
  return conn.chains[0] || "direct";
}

/** Host identity for one connection -- domain (SNI/Host header) when
 * sing-box resolved one, else the raw destination IP. Two different ids can
 * share the same underlying IP but different ports (e.g. two connections to
 * `1.2.3.4` on different ports) -- those still get separate nodes, same as
 * the reference app, since the id/label includes the port for
 * disambiguation; only the *rule target* drops it (a rule can't be scoped
 * to a port via `ipCidr`). Returns `null` when neither is available
 * (defensive -- sing-box fills `destinationIP` in practice). */
function hostIdentity(
  conn: ConnectionInfo,
): { id: string; label: string; ruleTarget: TopologyRuleTarget } | null {
  const { host, destinationIP, destinationPort } = conn.metadata;
  if (host) return { id: `host:${host}`, label: host, ruleTarget: { kind: "domain", value: host } };
  if (destinationIP) {
    const label = destinationPort ? `${destinationIP}:${destinationPort}` : destinationIP;
    return { id: `host:${label}`, label, ruleTarget: { kind: "ip", value: destinationIP } };
  }
  return null;
}

interface HostAgg {
  label: string;
  count: number;
  ruleTarget: TopologyRuleTarget;
  flows: Map<string, number>;
}

export function buildConnectionTopology(snapshot: ConnectionsSnapshot | null): TopologyData {
  const connections = snapshot?.connections ?? [];
  if (connections.length === 0) return EMPTY_TOPOLOGY;

  const hosts = new Map<string, HostAgg>();
  const exitTotals = new Map<string, number>();

  for (const conn of connections) {
    const tag = exitTag(conn);
    exitTotals.set(tag, (exitTotals.get(tag) ?? 0) + 1);

    const identity = hostIdentity(conn);
    if (!identity) continue; // no host node, but still counted into the exit total above
    let agg = hosts.get(identity.id);
    if (!agg) {
      agg = { label: identity.label, count: 0, ruleTarget: identity.ruleTarget, flows: new Map() };
      hosts.set(identity.id, agg);
    }
    agg.count += 1;
    agg.flows.set(tag, (agg.flows.get(tag) ?? 0) + 1);
  }

  if (hosts.size === 0) return EMPTY_TOPOLOGY;

  const sortedHosts = Array.from(hosts.entries()).sort((a, b) => b[1].count - a[1].count);
  const topHosts = sortedHosts.slice(0, TOP_N_HOSTS);
  const overflowHosts = sortedHosts.slice(TOP_N_HOSTS);

  const deviceTotal = sortedHosts.reduce((sum, [, h]) => sum + h.count, 0);

  const nodes: TopologyNode[] = [
    { id: "device", label: DEVICE_LABEL, value: deviceTotal, kind: "device", ruleTarget: null },
  ];

  for (const [id, h] of topHosts) {
    nodes.push({ id, label: h.label, value: h.count, kind: "host", ruleTarget: h.ruleTarget });
  }

  let overflowCount = 0;
  const overflowFlows = new Map<string, number>();
  for (const [, h] of overflowHosts) {
    overflowCount += h.count;
    for (const [tag, v] of h.flows) overflowFlows.set(tag, (overflowFlows.get(tag) ?? 0) + v);
  }
  if (overflowCount > 0) {
    nodes.push({ id: OTHER_HOST_ID, label: OTHERS_LABEL, value: overflowCount, kind: "other", ruleTarget: null });
  }

  const sortedExitTags = Array.from(exitTotals.keys()).sort(
    (a, b) => exitTotals.get(b)! - exitTotals.get(a)!,
  );
  for (const tag of sortedExitTags) {
    nodes.push({
      id: `exit:${tag}`,
      label: EXIT_LABEL[tag as keyof typeof EXIT_LABEL] ?? tag,
      value: exitTotals.get(tag)!,
      kind: "outbound",
      ruleTarget: null,
    });
  }

  const links: TopologyLink[] = [];
  for (const [id, h] of topHosts) {
    links.push({ sourceId: "device", targetId: id, value: h.count });
  }
  if (overflowCount > 0) {
    links.push({ sourceId: "device", targetId: OTHER_HOST_ID, value: overflowCount });
  }

  for (const [id, h] of topHosts) {
    for (const [tag, v] of h.flows) links.push({ sourceId: id, targetId: `exit:${tag}`, value: v });
  }
  if (overflowCount > 0) {
    for (const [tag, v] of overflowFlows) links.push({ sourceId: OTHER_HOST_ID, targetId: `exit:${tag}`, value: v });
  }

  return { nodes, links };
}
