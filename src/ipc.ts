// Thin wrapper around @tauri-apps/api/core's invoke() so call sites don't
// repeat command-name strings / argument boilerplate. See docs/ipc-contract.md
// for the authoritative command list — this file must stay in lockstep with
// src-tauri/src/commands/*.rs.

import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionsSnapshot,
  HelperStatus,
  PlatformInfo,
  ProxyStatus,
  RoutingRule,
  ServerConfig,
  SystemProxyStatus,
  UserConfig,
} from "./types";

export const ipc = {
  configGet: () => invoke<UserConfig>("config_get"),
  configSave: (config: UserConfig) => invoke<void>("config_save", { config }),

  serversAdd: (server: ServerConfig) => invoke<UserConfig>("servers_add", { server }),
  serversDelete: (id: string) => invoke<UserConfig>("servers_delete", { id }),

  rulesAdd: (rule: RoutingRule) => invoke<UserConfig>("rules_add", { rule }),
  rulesUpdate: (rule: RoutingRule) => invoke<UserConfig>("rules_update", { rule }),
  rulesDelete: (id: string) => invoke<UserConfig>("rules_delete", { id }),
  rulesReorder: (orderedIds: string[]) => invoke<UserConfig>("rules_reorder", { orderedIds }),

  proxyStart: (serverId: string) => invoke<ProxyStatus>("proxy_start", { serverId }),
  proxyStop: () => invoke<ProxyStatus>("proxy_stop"),
  proxyStatus: () => invoke<ProxyStatus>("proxy_status"),

  connectionsList: () => invoke<ConnectionsSnapshot>("connections_list"),
  connectionsClose: (id: string) => invoke<void>("connections_close", { id }),
  connectionsCloseAll: () => invoke<void>("connections_close_all"),

  subscriptionImport: (url: string) => invoke<UserConfig>("subscription_import", { url }),

  systemProxyStatus: () => invoke<SystemProxyStatus>("system_proxy_status"),
  platformInfo: () => invoke<PlatformInfo>("platform_info"),

  helperGetStatus: () => invoke<HelperStatus>("helper_get_status"),
  helperInstall: () => invoke<HelperStatus>("helper_install"),
  helperUninstall: () => invoke<HelperStatus>("helper_uninstall"),
};
