// Mirrors crates/shared-types/src/lib.rs exactly (camelCase on the wire via
// serde rename_all). Keep in sync with that file until specta codegen lands.

export type Protocol = "vless" | "trojan" | "shadowsocks" | "vmess" | "wireguard";

export const PROTOCOLS: Protocol[] = ["vless", "trojan", "shadowsocks", "vmess", "wireguard"];

export interface TlsConfig {
  enabled: boolean;
  serverName?: string | null;
  insecure: boolean;
  realityPublicKey?: string | null;
  realityShortId?: string | null;
}

export interface ServerConfig {
  id: string;
  name: string;
  protocol: Protocol;
  /** For "wireguard", this is the peer's endpoint host. */
  address: string;
  /** For "wireguard", this is the peer's endpoint port. */
  port: number;

  uuid?: string | null;
  password?: string | null;
  encryption?: string | null;
  flow?: string | null;

  /** Not applicable to "wireguard" -- it has no TLS layer. */
  tls?: TlsConfig | null;

  /** Base64-encoded 32-byte private key. Only used by "wireguard". */
  wireguardPrivateKey?: string | null;
  /** Base64-encoded 32-byte peer public key. Only used by "wireguard". */
  wireguardPeerPublicKey?: string | null;
  /** Optional base64-encoded 32-byte pre-shared key. Only used by "wireguard". */
  wireguardPreSharedKey?: string | null;
  /** This client's local tunnel address in CIDR form (e.g. "10.0.0.2/32"). Only used by "wireguard". */
  wireguardLocalAddress?: string | null;
}

export type ProxyMode = "global" | "smart" | "direct";
export const PROXY_MODES: ProxyMode[] = ["global", "smart", "direct"];

export type ProxyModeType = "systemProxy" | "tun" | "manual";
export const PROXY_MODE_TYPES: ProxyModeType[] = ["systemProxy", "tun", "manual"];

export type ProxyErrorCode =
  | "helperUnavailable"
  | "coreStartFailed"
  | "portInUse"
  | "configInvalid"
  | "unknown";

export interface ProxyStatus {
  running: boolean;
  pid?: number | null;
  /** Unix millis, matches `Date.now()` on the TS side. */
  startTime?: number | null;
  uptimeSecs?: number | null;
  error?: string | null;
  errorCode?: ProxyErrorCode | null;
  currentServerId?: string | null;
}

export type RuleMatchType = "domain" | "domainSuffix" | "domainKeyword" | "ipCidr" | "processName";
export const RULE_MATCH_TYPES: RuleMatchType[] = [
  "domain",
  "domainSuffix",
  "domainKeyword",
  "ipCidr",
  "processName",
];

export type RuleOutbound = "proxy" | "direct" | "block";
export const RULE_OUTBOUNDS: RuleOutbound[] = ["proxy", "direct", "block"];

export interface RoutingRule {
  id: string;
  name: string;
  enabled: boolean;
  matchType: RuleMatchType;
  /** One or more raw match values (domains/suffixes/keywords/CIDRs/process names depending on `matchType`). */
  values: string[];
  outbound: RuleOutbound;
}

export interface UserConfig {
  servers: ServerConfig[];
  rules: RoutingRule[];
  selectedServerId?: string | null;
  proxyMode: ProxyMode;
  proxyModeType: ProxyModeType;
  autoStart: boolean;
  silentStart: boolean;
  autoConnect: boolean;
  minimizeToTray: boolean;
  language?: string | null;
}

export type HelperPlatform = "windows" | "macos" | "linux";

export interface HelperStatus {
  platform: HelperPlatform;
  installed: boolean;
  ready: boolean;
  version?: string | null;
  needsRepair: boolean;
}

export interface SystemProxyStatus {
  enabled: boolean;
  httpProxy?: string | null;
  httpsProxy?: string | null;
  socksProxy?: string | null;
  bypassList: string[];
}

export interface PlatformInfo {
  platform: HelperPlatform;
  arch: string;
  osVersion: string;
  isAdmin: boolean;
}

export interface ConnectionMetadata {
  network: string;
  /** May be empty -- fall back to `destinationIP`:`destinationPort` for display. */
  host: string;
  destinationIP: string;
  destinationPort: string;
}

export interface ConnectionInfo {
  id: string;
  metadata: ConnectionMetadata;
  upload: number;
  download: number;
  /** RFC3339 timestamp string, e.g. "2024-01-15T10:30:00.123456Z". */
  start: string;
  chains: string[];
  /** Name of the matched routing rule, or "" if it fell through to the default route. */
  rule: string;
}

export interface ConnectionsSnapshot {
  /** Cumulative bytes downloaded since sing-box started -- sing-box's own semantics. */
  downloadTotal: number;
  uploadTotal: number;
  connections: ConnectionInfo[];
}

/** Shape of every rejected Tauri command promise (`AppError` on the Rust side). */
export interface AppError {
  code: string;
  message: string;
}

export function isAppError(err: unknown): err is AppError {
  return (
    typeof err === "object" &&
    err !== null &&
    "code" in err &&
    "message" in err &&
    typeof (err as Record<string, unknown>).code === "string" &&
    typeof (err as Record<string, unknown>).message === "string"
  );
}

export function appErrorMessage(err: unknown): string {
  if (isAppError(err)) return `${err.message} (${err.code})`;
  if (err instanceof Error) return err.message;
  return String(err);
}
