import { create } from "zustand";
import { ipc } from "./ipc";
import { appErrorMessage } from "./types";
import type {
  HelperStatus,
  PlatformInfo,
  ProxyStatus,
  ServerConfig,
  SystemProxyStatus,
  UserConfig,
} from "./types";

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

  configLoading: boolean;
  proxyBusy: boolean;
  helperBusy: boolean;
  subscriptionBusy: boolean;

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

  startProxy: (serverId: string) => Promise<void>;
  stopProxy: () => Promise<void>;
}

export const useAppStore = create<AppStore>((set, get) => ({
  config: null,
  proxyStatus: null,
  systemProxyStatus: null,
  platformInfo: null,
  helperStatus: null,

  configLoading: false,
  proxyBusy: false,
  helperBusy: false,
  subscriptionBusy: false,

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
}));
