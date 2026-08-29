import { create } from "zustand";
import { save, open } from "@tauri-apps/plugin-dialog";
import { ipc } from "./ipc";
import { appErrorMessage } from "./types";
import type {
  ConnectionsSnapshot,
  HelperStatus,
  HistoryEntry,
  PlatformInfo,
  ProxyStatus,
  RoutingRule,
  ServerConfig,
  SystemProxyStatus,
  UserConfig,
} from "./types";

const EMPTY_CONNECTIONS_SNAPSHOT: ConnectionsSnapshot = {
  downloadTotal: 0,
  uploadTotal: 0,
  connections: [],
};

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  message: string;
}

let toastSeq = 0;

interface AppStore {
  config: UserConfig | null;
  proxyStatus: ProxyStatus | null;
  systemProxyStatus: SystemProxyStatus | null;
  platformInfo: PlatformInfo | null;
  helperStatus: HelperStatus | null;
  connectionsSnapshot: ConnectionsSnapshot | null;
  historyEntries: HistoryEntry[];

  configLoading: boolean;
  proxyBusy: boolean;
  helperBusy: boolean;
  subscriptionBusy: boolean;
  warpBusy: boolean;

  toasts: Toast[];
  pushToast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;

  refreshConfig: () => Promise<void>;
  refreshProxyStatus: () => Promise<void>;
  refreshSystemProxyStatus: () => Promise<void>;
  refreshPlatformInfo: () => Promise<void>;
  refreshHelperStatus: () => Promise<void>;
  installHelper: () => Promise<void>;
  uninstallHelper: () => Promise<void>;

  saveConfig: (config: UserConfig) => Promise<void>;
  addServer: (server: ServerConfig) => Promise<void>;
  deleteServer: (id: string) => Promise<void>;
  selectServer: (id: string | null) => Promise<void>;
  importSubscription: (url: string) => Promise<void>;
  registerWarp: () => Promise<void>;

  addRule: (rule: RoutingRule) => Promise<void>;
  updateRule: (rule: RoutingRule) => Promise<void>;
  deleteRule: (id: string) => Promise<void>;
  moveRuleUp: (id: string) => Promise<void>;
  moveRuleDown: (id: string) => Promise<void>;

  startProxy: (serverId: string) => Promise<void>;
  stopProxy: () => Promise<void>;
  openDashboard: () => Promise<void>;

  refreshConnections: () => Promise<void>;
  closeConnection: (id: string) => Promise<void>;
  closeAllConnections: () => Promise<void>;

  refreshHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;

  exportBackup: () => Promise<void>;
  importBackup: () => Promise<void>;
  exportDiagnostic: () => Promise<void>;
}

export const useAppStore = create<AppStore>((set, get) => ({
  config: null,
  proxyStatus: null,
  systemProxyStatus: null,
  platformInfo: null,
  helperStatus: null,
  connectionsSnapshot: null,
  historyEntries: [],

  configLoading: false,
  proxyBusy: false,
  helperBusy: false,
  subscriptionBusy: false,
  warpBusy: false,

  toasts: [],
  pushToast: (kind, message) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message }] }));
    if (kind !== "error") {
      setTimeout(() => get().dismissToast(id), 4000);
    }
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  refreshConfig: async () => {
    set({ configLoading: true });
    try {
      const config = await ipc.configGet();
      set({ config });
    } catch (err) {
      get().pushToast("error", `Failed to load config: ${appErrorMessage(err)}`);
    } finally {
      set({ configLoading: false });
    }
  },

  refreshProxyStatus: async () => {
    try {
      const proxyStatus = await ipc.proxyStatus();
      set({ proxyStatus });
    } catch (err) {
      // Common during MVP bring-up (core-manager not wired yet) — surface it
      // as status text rather than a disruptive toast.
      set({
        proxyStatus: {
          running: false,
          error: appErrorMessage(err),
          errorCode: "unknown",
        },
      });
    }
  },

  refreshSystemProxyStatus: async () => {
    try {
      const systemProxyStatus = await ipc.systemProxyStatus();
      set({ systemProxyStatus });
    } catch (err) {
      set({
        systemProxyStatus: {
          enabled: false,
          bypassList: [],
        },
      });
      get().pushToast("info", `System proxy status unavailable: ${appErrorMessage(err)}`);
    }
  },

  refreshPlatformInfo: async () => {
    try {
      const platformInfo = await ipc.platformInfo();
      set({ platformInfo });
    } catch (err) {
      get().pushToast("error", `Failed to load platform info: ${appErrorMessage(err)}`);
    }
  },

  refreshHelperStatus: async () => {
    try {
      const helperStatus = await ipc.helperGetStatus();
      set({ helperStatus });
    } catch (err) {
      // Not-installed is reported by the command as Ok(installed:false),
      // so a thrown error here means something else went wrong (e.g. the
      // command itself isn't wired up on this platform) — surface it
      // quietly rather than a disruptive toast, matching refreshProxyStatus.
      set({ helperStatus: null });
      get().pushToast("info", `Helper status unavailable: ${appErrorMessage(err)}`);
    }
  },

  installHelper: async () => {
    set({ helperBusy: true });
    try {
      const helperStatus = await ipc.helperInstall();
      set({ helperStatus });
      get().pushToast("success", "Privileged helper installed");
    } catch (err) {
      get().pushToast("error", `Failed to install helper: ${appErrorMessage(err)}`);
    } finally {
      set({ helperBusy: false });
    }
  },

  uninstallHelper: async () => {
    set({ helperBusy: true });
    try {
      const helperStatus = await ipc.helperUninstall();
      set({ helperStatus });
      get().pushToast("success", "Privileged helper removed");
    } catch (err) {
      get().pushToast("error", `Failed to remove helper: ${appErrorMessage(err)}`);
    } finally {
      set({ helperBusy: false });
    }
  },

  saveConfig: async (config) => {
    try {
      await ipc.configSave(config);
      set({ config });
      get().pushToast("success", "Settings saved");
    } catch (err) {
      get().pushToast("error", `Failed to save settings: ${appErrorMessage(err)}`);
    }
  },

  addServer: async (server) => {
    try {
      const config = await ipc.serversAdd(server);
      set({ config });
      get().pushToast("success", `Added server "${server.name}"`);
    } catch (err) {
      get().pushToast("error", `Failed to add server: ${appErrorMessage(err)}`);
    }
  },

  deleteServer: async (id) => {
    try {
      const config = await ipc.serversDelete(id);
      set({ config });
      get().pushToast("success", "Server removed");
    } catch (err) {
      get().pushToast("error", `Failed to delete server: ${appErrorMessage(err)}`);
    }
  },

  selectServer: async (id) => {
    const current = get().config;
    if (!current) return;
    const next = { ...current, selectedServerId: id };
    try {
      await ipc.configSave(next);
      set({ config: next });
    } catch (err) {
      get().pushToast("error", `Failed to select server: ${appErrorMessage(err)}`);
    }
  },

  importSubscription: async (url) => {
    set({ subscriptionBusy: true });
    const before = get().config?.servers.length ?? 0;
    try {
      const config = await ipc.subscriptionImport(url);
      set({ config });
      const imported = config.servers.length - before;
      get().pushToast("success", `Imported ${imported} server${imported === 1 ? "" : "s"}`);
    } catch (err) {
      get().pushToast("error", `Failed to import subscription: ${appErrorMessage(err)}`);
    } finally {
      set({ subscriptionBusy: false });
    }
  },

  registerWarp: async () => {
    set({ warpBusy: true });
    try {
      const config = await ipc.warpRegister();
      set({ config });
      get().pushToast("success", "Registered Cloudflare WARP");
    } catch (err) {
      get().pushToast("error", `Failed to register Cloudflare WARP: ${appErrorMessage(err)}`);
    } finally {
      set({ warpBusy: false });
    }
  },

  addRule: async (rule) => {
    try {
      const config = await ipc.rulesAdd(rule);
      set({ config });
      get().pushToast("success", `Added rule "${rule.name}"`);
    } catch (err) {
      get().pushToast("error", `Failed to add rule: ${appErrorMessage(err)}`);
    }
  },

  updateRule: async (rule) => {
    try {
      const config = await ipc.rulesUpdate(rule);
      set({ config });
    } catch (err) {
      get().pushToast("error", `Failed to update rule: ${appErrorMessage(err)}`);
    }
  },

  deleteRule: async (id) => {
    try {
      const config = await ipc.rulesDelete(id);
      set({ config });
      get().pushToast("success", "Rule removed");
    } catch (err) {
      get().pushToast("error", `Failed to delete rule: ${appErrorMessage(err)}`);
    }
  },

  moveRuleUp: async (id) => {
    const rules = get().config?.rules ?? [];
    const index = rules.findIndex((r) => r.id === id);
    if (index <= 0) return;
    const ids = rules.map((r) => r.id);
    [ids[index - 1], ids[index]] = [ids[index], ids[index - 1]];
    try {
      const config = await ipc.rulesReorder(ids);
      set({ config });
    } catch (err) {
      get().pushToast("error", `Failed to reorder rules: ${appErrorMessage(err)}`);
    }
  },

  moveRuleDown: async (id) => {
    const rules = get().config?.rules ?? [];
    const index = rules.findIndex((r) => r.id === id);
    if (index === -1 || index >= rules.length - 1) return;
    const ids = rules.map((r) => r.id);
    [ids[index], ids[index + 1]] = [ids[index + 1], ids[index]];
    try {
      const config = await ipc.rulesReorder(ids);
      set({ config });
    } catch (err) {
      get().pushToast("error", `Failed to reorder rules: ${appErrorMessage(err)}`);
    }
  },

  startProxy: async (serverId) => {
    set({ proxyBusy: true });
    try {
      const proxyStatus = await ipc.proxyStart(serverId);
      set({ proxyStatus });
      if (proxyStatus.error) {
        get().pushToast("error", proxyStatus.error);
      }
    } catch (err) {
      get().pushToast("error", `Failed to start proxy: ${appErrorMessage(err)}`);
    } finally {
      set({ proxyBusy: false });
    }
  },

  stopProxy: async () => {
    set({ proxyBusy: true });
    try {
      const proxyStatus = await ipc.proxyStop();
      set({ proxyStatus });
      if (proxyStatus.error) {
        get().pushToast("error", proxyStatus.error);
      }
    } catch (err) {
      get().pushToast("error", `Failed to stop proxy: ${appErrorMessage(err)}`);
    } finally {
      set({ proxyBusy: false });
    }
  },

  openDashboard: async () => {
    try {
      await ipc.dashboardOpen();
    } catch (err) {
      get().pushToast("error", `Failed to open sing-box dashboard: ${appErrorMessage(err)}`);
    }
  },

  refreshConnections: async () => {
    try {
      const connectionsSnapshot = await ipc.connectionsList();
      set({ connectionsSnapshot });
    } catch {
      // Routine whenever the proxy isn't running (proxy_not_running) --
      // same treatment as refreshProxyStatus: fall back to an empty
      // snapshot quietly rather than spamming a toast on every 2s poll.
      set({ connectionsSnapshot: EMPTY_CONNECTIONS_SNAPSHOT });
    }
  },

  closeConnection: async (id) => {
    try {
      await ipc.connectionsClose(id);
      await get().refreshConnections();
    } catch (err) {
      get().pushToast("error", `Failed to close connection: ${appErrorMessage(err)}`);
    }
  },

  closeAllConnections: async () => {
    try {
      await ipc.connectionsCloseAll();
      await get().refreshConnections();
    } catch (err) {
      get().pushToast("error", `Failed to close all connections: ${appErrorMessage(err)}`);
    }
  },

  refreshHistory: async () => {
    try {
      const historyEntries = await ipc.historyList();
      set({ historyEntries });
    } catch (err) {
      // Unlike `refreshConnections`, there's no "expected" failure mode here
      // (a missing/never-enabled history file is `Ok([])`, not an error) --
      // a thrown error means something genuinely went wrong reading the
      // file, worth a toast.
      get().pushToast("error", `Failed to load connection history: ${appErrorMessage(err)}`);
    }
  },

  clearHistory: async () => {
    try {
      await ipc.historyClear();
      set({ historyEntries: [] });
      get().pushToast("success", "Connection history cleared");
    } catch (err) {
      get().pushToast("error", `Failed to clear connection history: ${appErrorMessage(err)}`);
    }
  },

  exportBackup: async () => {
    const path = await save({
      defaultPath: "ferroflow-backup.json",
      filters: [{ name: "Ferroflow backup", extensions: ["json"] }],
    });
    if (!path) return;
    try {
      await ipc.backupExport(path);
      get().pushToast("success", "Backup exported");
    } catch (err) {
      get().pushToast("error", `Failed to export backup: ${appErrorMessage(err)}`);
    }
  },

  importBackup: async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Ferroflow backup", extensions: ["json"] }],
    });
    if (!selected) return;
    const path = Array.isArray(selected) ? selected[0] : selected;
    try {
      const config = await ipc.backupImport(path);
      set({ config });
      get().pushToast("success", "Backup imported");
    } catch (err) {
      get().pushToast("error", `Failed to import backup: ${appErrorMessage(err)}`);
    }
  },

  exportDiagnostic: async () => {
    const path = await save({
      defaultPath: "ferroflow-diagnostic.md",
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!path) return;
    try {
      await ipc.diagnosticExport(path);
      get().pushToast("success", "Diagnostic report exported");
    } catch (err) {
      get().pushToast("error", `Failed to export diagnostic report: ${appErrorMessage(err)}`);
    }
  },
}));
