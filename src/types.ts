// Mirrors crates/shared-types/src/lib.rs exactly (camelCase on the wire via
// serde rename_all). Keep in sync with that file until specta codegen lands.

export type Protocol = "vless" | "trojan" | "shadowsocks" | "vmess";

export const PROTOCOLS: Protocol[] = ["vless", "trojan", "shadowsocks", "vmess"];

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
  address: string;
  port: number;

  uuid?: string | null;
  password?: string | null;
  encryption?: string | null;
  flow?: string | null;

  tls?: TlsConfig | null;
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

export interface UserConfig {
  servers: ServerConfig[];
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
