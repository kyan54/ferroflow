import { create } from "zustand";
import { save, open } from "@tauri-apps/plugin-dialog";
import { ipc } from "./ipc";
import { appErrorMessage } from "./types";
import { newId } from "./lib/utils";
import { getT, normalizeLanguage, setCurrentLanguage } from "./i18n/current";
import type { Language } from "./i18n/dictionary";
import { applyTheme, normalizeTheme } from "./lib/theme";
import type { Theme } from "./lib/theme";
import {
  appRoutingRuleId,
  buildPresetRules,
  presetResourceRefs,
  ruleResourceId,
  PRESET_RULE_PREFIX,
  REGION_PRESETS,
} from "./lib/appRouting";
import type {
  CatalogEntry,
  ConnectionsSnapshot,
  HelperStatus,
  HistoryEntry,
  LogEntry,
  PlatformInfo,
  ProxyStatus,
  RoutingRule,
  RuleOutbound,
  RuleResourceCategory,
  ServerConfig,
  SystemProxyStatus,
  UnlockResult,
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
  /** Set by `dismissToast` just before removal so `ToastStack` can play its
   * exit animation instead of the DOM node vanishing instantly -- the toast
   * stays in `toasts` for one more animation frame's worth of time, then a
   * second pass actually drops it from the array. */
  leaving?: boolean;
}

/** Matches `.animate-toast-out`'s duration in index.css. */
const TOAST_EXIT_MS = 180;

let toastSeq = 0;

interface AppStore {
  config: UserConfig | null;
  language: Language;
  theme: Theme;
  proxyStatus: ProxyStatus | null;
  systemProxyStatus: SystemProxyStatus | null;
  platformInfo: PlatformInfo | null;
  helperStatus: HelperStatus | null;
  connectionsSnapshot: ConnectionsSnapshot | null;
  historyEntries: HistoryEntry[];
  logEntries: LogEntry[];
  ruleResourceCatalog: CatalogEntry[];
  unlockResults: UnlockResult[] | null;
  unlockError: string | null;
  /** `serverId -> last measured latency in ms, or null for a failed/timed
   * out probe` -- populated by `testServerLatency`/`testAllServerLatency`,
   * never cleared on its own (a stale reading is still useful context until
   * the next test). */
  latencyResults: Record<string, number | null>;

  configLoading: boolean;
  proxyBusy: boolean;
  helperBusy: boolean;
  subscriptionBusy: boolean;
  warpBusy: boolean;
  ruleResourceBusy: boolean;
  appRoutingBusy: boolean;
  regionPresetBusy: boolean;
  unlockBusy: boolean;
  /** Ids of servers currently being probed by `testServerLatency`, plus a
   * separate whole-batch flag for `testAllServerLatency` -- lets the
   * Servers page show a per-card spinner and disable "Test all" without
   * conflating the two. */
  latencyTestingIds: Set<string>;
  latencyTestingAll: boolean;

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
  /** Updates `language` state immediately (so the switch feels instant, no
   * round-trip wait) and persists it via `saveConfig` -- same pattern as
   * every other settings toggle in this store. */
  setLanguage: (language: Language) => Promise<void>;
  /** Same pattern as `setLanguage`: updates `theme` state (and the DOM
   * `data-theme` attribute via `applyTheme`) immediately, then persists via
   * `saveConfig`. */
  setTheme: (theme: Theme) => Promise<void>;
  addServer: (server: ServerConfig) => Promise<void>;
  deleteServer: (id: string) => Promise<void>;
  duplicateServer: (id: string) => Promise<void>;
  /** "Clone to self-built": same as `duplicateServer`, but the copy always
   * gets `source: "manual"` regardless of the original's -- see
   * `ServerSource`'s doc comment. Only shown in the UI for
   * `source === "subscription"` servers (a manual server's "Duplicate"
   * button already does the same thing). */
  cloneToSelfBuilt: (id: string) => Promise<void>;
  /** Generates this server's share-link (`vless://`/`trojan://`/`ss://`/
   * `vmess://`) via `subscription::generate_share_url` and copies it to the
   * clipboard -- `wireguard` servers have no share-link format, see
   * `hasShareLink` in `ServersView.tsx`, which hides the button for them. */
  copyShareUrl: (id: string) => Promise<void>;
  selectServer: (id: string | null) => Promise<void>;
  importSubscription: (url: string) => Promise<void>;
  importSubscriptionText: (text: string) => Promise<void>;
  importSubscriptionFile: (path: string) => Promise<void>;
  registerWarp: () => Promise<void>;

  addRule: (rule: RoutingRule) => Promise<void>;
  updateRule: (rule: RoutingRule) => Promise<void>;
  deleteRule: (id: string) => Promise<void>;
  /** Commits a full reordering of `config.rules` in one IPC round-trip (used
   * by `RulesView`'s "edit order" draft mode -- unlike a per-step move, the
   * caller batches every drag/move-to-edge change locally and only calls
   * this once, on "Save order"). Returns whether it succeeded so the view
   * can decide whether to exit the draft (kept open on failure, matching
   * the toast this already pushes rather than throwing). */
  commitRuleOrder: (orderedIds: string[]) => Promise<boolean>;

  /** Sets (or, with `outbound: null`, removes) the `RoutingRule` behind one
   * `AppRoutingView` app toggle -- downloads the backing GeoSite resource
   * first if it isn't already tracked in `config.ruleResources`. */
  setAppRoute: (appId: string, appLabel: string, geositeName: string, outbound: RuleOutbound | null) => Promise<void>;
  /** Applies a region preset (see `src/lib/appRouting.ts`): downloads any
   * resource its rules reference that isn't already tracked, replaces its
   * own previously-applied rules (or the whole `rules` list, for the
   * "clears all" preset), sets `defaultOutbound`, and saves. */
  applyRegionPreset: (presetId: string) => Promise<void>;
  /** Removes only the rules a region preset previously created (see
   * `PRESET_RULE_PREFIX`), leaving `defaultOutbound`, manual rules, and
   * `AppRoutingView` toggles untouched. Used for the Rules page's region
   * routing on/off switch -- deliberately *not* the same as applying the
   * "Global proxy, no rules" preset, which also wipes manual rules and is
   * left as a deliberate, confirm-guarded action on the App routing page. */
  clearRegionPreset: () => Promise<void>;

  startProxy: (serverId: string) => Promise<void>;
  stopProxy: () => Promise<void>;
  openDashboard: () => Promise<void>;
  checkUnlock: () => Promise<void>;
  testServerLatency: (serverId: string) => Promise<void>;
  testAllServerLatency: () => Promise<void>;

  refreshConnections: () => Promise<void>;
  closeConnection: (id: string) => Promise<void>;
  closeAllConnections: () => Promise<void>;

  refreshHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;

  refreshLogs: () => Promise<void>;
  clearLogs: () => Promise<void>;

  exportBackup: () => Promise<void>;
  importBackup: () => Promise<void>;
  exportDiagnostic: () => Promise<void>;

  refreshRuleResourceCatalog: () => Promise<void>;
  downloadRuleResource: (category: RuleResourceCategory, name: string) => Promise<void>;
  downloadCustomRuleResource: (name: string, category: RuleResourceCategory, url: string) => Promise<void>;
  updateAllRuleResources: () => Promise<void>;
  deleteRuleResource: (id: string) => Promise<void>;
}

export const useAppStore = create<AppStore>((set, get) => ({
  config: null,
  language: "en",
  theme: "system",
  proxyStatus: null,
  systemProxyStatus: null,
  platformInfo: null,
  helperStatus: null,
  connectionsSnapshot: null,
  historyEntries: [],
  logEntries: [],
  ruleResourceCatalog: [],
  unlockResults: null,
  unlockError: null,
  latencyResults: {},

  configLoading: false,
  proxyBusy: false,
  helperBusy: false,
  subscriptionBusy: false,
  warpBusy: false,
  ruleResourceBusy: false,
  appRoutingBusy: false,
  regionPresetBusy: false,
  unlockBusy: false,
  latencyTestingIds: new Set(),
  latencyTestingAll: false,

  toasts: [],
  pushToast: (kind, message) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message }] }));
    if (kind !== "error") {
      setTimeout(() => get().dismissToast(id), 4000);
    }
  },
  dismissToast: (id) => {
    set((s) => ({ toasts: s.toasts.map((t) => (t.id === id ? { ...t, leaving: true } : t)) }));
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, TOAST_EXIT_MS);
  },

  refreshConfig: async () => {
    set({ configLoading: true });
    try {
      const config = await ipc.configGet();
      const language = normalizeLanguage(config.language);
      setCurrentLanguage(language);
      const theme = normalizeTheme(config.theme);
      applyTheme(theme);
      set({ config, language, theme });
    } catch (err) {
      get().pushToast("error", getT().toasts.loadConfigFailed(appErrorMessage(err)));
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
      get().pushToast("info", getT().toasts.systemProxyStatusUnavailable(appErrorMessage(err)));
    }
  },

  refreshPlatformInfo: async () => {
    try {
      const platformInfo = await ipc.platformInfo();
      set({ platformInfo });
    } catch (err) {
      get().pushToast("error", getT().toasts.loadPlatformInfoFailed(appErrorMessage(err)));
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
      get().pushToast("info", getT().toasts.helperStatusUnavailable(appErrorMessage(err)));
    }
  },

  installHelper: async () => {
    set({ helperBusy: true });
    try {
      const helperStatus = await ipc.helperInstall();
      set({ helperStatus });
      get().pushToast("success", getT().toasts.helperInstalled);
    } catch (err) {
      get().pushToast("error", getT().toasts.helperInstallFailed(appErrorMessage(err)));
    } finally {
      set({ helperBusy: false });
    }
  },

  uninstallHelper: async () => {
    set({ helperBusy: true });
    try {
      const helperStatus = await ipc.helperUninstall();
      set({ helperStatus });
      get().pushToast("success", getT().toasts.helperRemoved);
    } catch (err) {
      get().pushToast("error", getT().toasts.helperRemoveFailed(appErrorMessage(err)));
    } finally {
      set({ helperBusy: false });
    }
  },

  saveConfig: async (config) => {
    try {
      await ipc.configSave(config);
      set({ config });
      get().pushToast("success", getT().toasts.settingsSaved);
    } catch (err) {
      get().pushToast("error", getT().toasts.settingsSaveFailed(appErrorMessage(err)));
    }
  },

  setLanguage: async (language) => {
    set({ language });
    setCurrentLanguage(language);
    const current = get().config;
    if (!current) return;
    await get().saveConfig({ ...current, language });
  },

  setTheme: async (theme) => {
    set({ theme });
    applyTheme(theme);
    const current = get().config;
    if (!current) return;
    await get().saveConfig({ ...current, theme });
  },

  addServer: async (server) => {
    try {
      const config = await ipc.serversAdd(server);
      set({ config });
      get().pushToast("success", getT().toasts.serverAdded(server.name));
    } catch (err) {
      get().pushToast("error", getT().toasts.serverAddFailed(appErrorMessage(err)));
    }
  },

  deleteServer: async (id) => {
    try {
      const config = await ipc.serversDelete(id);
      set({ config });
      get().pushToast("success", getT().toasts.serverRemoved);
    } catch (err) {
      get().pushToast("error", getT().toasts.serverDeleteFailed(appErrorMessage(err)));
    }
  },

  duplicateServer: async (id) => {
    const original = get().config?.servers.find((s) => s.id === id);
    if (!original) return;
    const clone: ServerConfig = { ...original, id: newId(), name: `${original.name} (copy)` };
    try {
      const config = await ipc.serversAdd(clone);
      set({ config });
      get().pushToast("success", getT().toasts.serverDuplicated(original.name));
    } catch (err) {
      get().pushToast("error", getT().toasts.serverDuplicateFailed(appErrorMessage(err)));
    }
  },

  cloneToSelfBuilt: async (id) => {
    const original = get().config?.servers.find((s) => s.id === id);
    if (!original) return;
    const clone: ServerConfig = {
      ...original,
      id: newId(),
      name: getT().servers.cloneToSelfBuiltName(original.name),
      source: "manual",
    };
    try {
      const config = await ipc.serversAdd(clone);
      set({ config });
      get().pushToast("success", getT().toasts.serverCloned(original.name));
    } catch (err) {
      get().pushToast("error", getT().toasts.serverCloneFailed(appErrorMessage(err)));
    }
  },

  copyShareUrl: async (id) => {
    const server = get().config?.servers.find((s) => s.id === id);
    if (!server) return;
    try {
      const url = await ipc.subscriptionGenerateShareUrl(server);
      await navigator.clipboard.writeText(url);
      get().pushToast("success", getT().toasts.shareUrlCopied);
    } catch (err) {
      get().pushToast("error", getT().toasts.shareUrlCopyFailed(appErrorMessage(err)));
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
      get().pushToast("error", getT().toasts.serverSelectFailed(appErrorMessage(err)));
    }
  },

  importSubscription: async (url) => {
    set({ subscriptionBusy: true });
    const before = get().config?.servers?.length ?? 0;
    try {
      const config = await ipc.subscriptionImport(url);
      set({ config });
      const imported = config.servers.length - before;
      get().pushToast("success", getT().toasts.serversImported(imported));
    } catch (err) {
      get().pushToast("error", getT().toasts.subscriptionImportFailed(appErrorMessage(err)));
    } finally {
      set({ subscriptionBusy: false });
    }
  },

  importSubscriptionText: async (text) => {
    set({ subscriptionBusy: true });
    const before = get().config?.servers?.length ?? 0;
    try {
      const config = await ipc.subscriptionImportText(text);
      set({ config });
      const imported = config.servers.length - before;
      get().pushToast("success", getT().toasts.serversImported(imported));
    } catch (err) {
      get().pushToast("error", getT().toasts.serversImportFailed(appErrorMessage(err)));
    } finally {
      set({ subscriptionBusy: false });
    }
  },

  importSubscriptionFile: async (path) => {
    set({ subscriptionBusy: true });
    const before = get().config?.servers?.length ?? 0;
    try {
      const config = await ipc.subscriptionImportFile(path);
      set({ config });
      const imported = config.servers.length - before;
      get().pushToast("success", getT().toasts.serversImported(imported));
    } catch (err) {
      get().pushToast("error", getT().toasts.fileImportFailed(appErrorMessage(err)));
    } finally {
      set({ subscriptionBusy: false });
    }
  },

  registerWarp: async () => {
    set({ warpBusy: true });
    try {
      const config = await ipc.warpRegister();
      set({ config });
      get().pushToast("success", getT().toasts.warpRegistered);
    } catch (err) {
      get().pushToast("error", getT().toasts.warpRegisterFailed(appErrorMessage(err)));
    } finally {
      set({ warpBusy: false });
    }
  },

  addRule: async (rule) => {
    try {
      const config = await ipc.rulesAdd(rule);
      set({ config });
      get().pushToast("success", getT().toasts.ruleAdded(rule.name));
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleAddFailed(appErrorMessage(err)));
    }
  },

  updateRule: async (rule) => {
    try {
      const config = await ipc.rulesUpdate(rule);
      set({ config });
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleUpdateFailed(appErrorMessage(err)));
    }
  },

  deleteRule: async (id) => {
    try {
      const config = await ipc.rulesDelete(id);
      set({ config });
      get().pushToast("success", getT().toasts.ruleRemoved);
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleDeleteFailed(appErrorMessage(err)));
    }
  },

  commitRuleOrder: async (orderedIds) => {
    try {
      const config = await ipc.rulesReorder(orderedIds);
      set({ config });
      return true;
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleReorderFailed(appErrorMessage(err)));
      return false;
    }
  },

  setAppRoute: async (appId, appLabel, geositeName, outbound) => {
    const current = get().config;
    if (!current) return;
    const ruleId = appRoutingRuleId(appId);

    set({ appRoutingBusy: true });
    try {
      if (outbound === null) {
        if (current.rules.some((r) => r.id === ruleId)) {
          const config = await ipc.rulesDelete(ruleId);
          set({ config });
        }
        return;
      }

      let workingConfig = current;
      const resourceId = ruleResourceId("geosite", geositeName);
      if (!workingConfig.ruleResources.some((r) => r.id === resourceId)) {
        workingConfig = await ipc.ruleResourcesDownload("geosite", geositeName);
        set({ config: workingConfig });
      }

      const rule: RoutingRule = {
        id: ruleId,
        name: `App routing: ${appLabel}`,
        enabled: true,
        matchType: "ruleSet",
        values: [resourceId],
        outbound,
      };
      const config = workingConfig.rules.some((r) => r.id === ruleId)
        ? await ipc.rulesUpdate(rule)
        : await ipc.rulesAdd(rule);
      set({ config });
    } catch (err) {
      get().pushToast("error", getT().toasts.appRoutingUpdateFailed(appLabel, appErrorMessage(err)));
    } finally {
      set({ appRoutingBusy: false });
    }
  },

  applyRegionPreset: async (presetId) => {
    const current = get().config;
    if (!current) return;
    const preset = REGION_PRESETS.find((p) => p.id === presetId);
    if (!preset) return;

    set({ regionPresetBusy: true });
    try {
      let workingConfig = current;
      for (const ref of presetResourceRefs(preset)) {
        const resourceId = ruleResourceId(ref.category, ref.name);
        if (!workingConfig.ruleResources.some((r) => r.id === resourceId)) {
          workingConfig = await ipc.ruleResourcesDownload(ref.category, ref.name);
        }
      }

      const keptRules = preset.clearsAllRules
        ? []
        : workingConfig.rules.filter((r) => !r.id.startsWith(PRESET_RULE_PREFIX));

      const nextConfig: UserConfig = {
        ...workingConfig,
        rules: [...keptRules, ...buildPresetRules(preset)],
        defaultOutbound: preset.defaultOutbound,
      };

      await ipc.configSave(nextConfig);
      set({ config: nextConfig });
      get().pushToast("success", getT().toasts.presetApplied(preset.label));
    } catch (err) {
      get().pushToast("error", getT().toasts.presetApplyFailed(appErrorMessage(err)));
    } finally {
      set({ regionPresetBusy: false });
    }
  },

  clearRegionPreset: async () => {
    const current = get().config;
    if (!current) return;

    set({ regionPresetBusy: true });
    try {
      const nextConfig: UserConfig = {
        ...current,
        rules: current.rules.filter((r) => !r.id.startsWith(PRESET_RULE_PREFIX)),
      };
      await ipc.configSave(nextConfig);
      set({ config: nextConfig });
    } catch (err) {
      get().pushToast("error", getT().toasts.presetApplyFailed(appErrorMessage(err)));
    } finally {
      set({ regionPresetBusy: false });
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
      get().pushToast("error", getT().toasts.proxyStartFailed(appErrorMessage(err)));
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
      get().pushToast("error", getT().toasts.proxyStopFailed(appErrorMessage(err)));
    } finally {
      set({ proxyBusy: false });
    }
  },

  openDashboard: async () => {
    try {
      await ipc.dashboardOpen();
    } catch (err) {
      get().pushToast("error", getT().toasts.dashboardOpenFailed(appErrorMessage(err)));
    }
  },

  checkUnlock: async () => {
    set({ unlockBusy: true, unlockError: null });
    try {
      const unlockResults = await ipc.unlockCheck();
      set({ unlockResults });
    } catch (err) {
      // `proxy_not_running` is the expected/common case (no proxy running,
      // or TUN mode with no local port to probe through) -- shown inline on
      // the card itself rather than a disruptive toast, same treatment as
      // `refreshProxyStatus`/`refreshConnections`.
      set({ unlockError: appErrorMessage(err) });
    } finally {
      set({ unlockBusy: false });
    }
  },

  testServerLatency: async (serverId) => {
    set((s) => ({ latencyTestingIds: new Set(s.latencyTestingIds).add(serverId) }));
    try {
      const ms = await ipc.serverTestLatency(serverId);
      set((s) => ({ latencyResults: { ...s.latencyResults, [serverId]: ms } }));
    } catch (err) {
      get().pushToast("error", getT().toasts.latencyTestFailed(appErrorMessage(err)));
    } finally {
      set((s) => {
        const next = new Set(s.latencyTestingIds);
        next.delete(serverId);
        return { latencyTestingIds: next };
      });
    }
  },

  testAllServerLatency: async () => {
    set({ latencyTestingAll: true });
    try {
      const results = await ipc.serversTestLatencyAll();
      set((s) => ({ latencyResults: { ...s.latencyResults, ...results } }));
    } catch (err) {
      get().pushToast("error", getT().toasts.latencyTestFailed(appErrorMessage(err)));
    } finally {
      set({ latencyTestingAll: false });
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
      get().pushToast("error", getT().toasts.connectionCloseFailed(appErrorMessage(err)));
    }
  },

  closeAllConnections: async () => {
    try {
      await ipc.connectionsCloseAll();
      await get().refreshConnections();
    } catch (err) {
      get().pushToast("error", getT().toasts.connectionsCloseAllFailed(appErrorMessage(err)));
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
      get().pushToast("error", getT().toasts.historyLoadFailed(appErrorMessage(err)));
    }
  },

  clearHistory: async () => {
    try {
      await ipc.historyClear();
      set({ historyEntries: [] });
      get().pushToast("success", getT().toasts.historyCleared);
    } catch (err) {
      get().pushToast("error", getT().toasts.historyClearFailed(appErrorMessage(err)));
    }
  },

  refreshLogs: async () => {
    try {
      const logEntries = await ipc.logsGet();
      set({ logEntries });
    } catch (err) {
      get().pushToast("error", getT().toasts.logsLoadFailed(appErrorMessage(err)));
    }
  },

  clearLogs: async () => {
    try {
      await ipc.logsClear();
      set({ logEntries: [] });
      get().pushToast("success", getT().toasts.logsCleared);
    } catch (err) {
      get().pushToast("error", getT().toasts.logsClearFailed(appErrorMessage(err)));
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
      get().pushToast("success", getT().toasts.backupExported);
    } catch (err) {
      get().pushToast("error", getT().toasts.backupExportFailed(appErrorMessage(err)));
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
      get().pushToast("success", getT().toasts.backupImported);
    } catch (err) {
      get().pushToast("error", getT().toasts.backupImportFailed(appErrorMessage(err)));
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
      get().pushToast("success", getT().toasts.diagnosticExported);
    } catch (err) {
      get().pushToast("error", getT().toasts.diagnosticExportFailed(appErrorMessage(err)));
    }
  },

  refreshRuleResourceCatalog: async () => {
    try {
      const ruleResourceCatalog = await ipc.ruleResourcesCatalog();
      set({ ruleResourceCatalog });
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleResourceCatalogLoadFailed(appErrorMessage(err)));
    }
  },

  downloadRuleResource: async (category, name) => {
    set({ ruleResourceBusy: true });
    try {
      const config = await ipc.ruleResourcesDownload(category, name);
      set({ config });
      get().pushToast("success", getT().toasts.ruleResourceDownloaded(name));
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleResourceDownloadFailed(appErrorMessage(err)));
    } finally {
      set({ ruleResourceBusy: false });
    }
  },

  downloadCustomRuleResource: async (name, category, url) => {
    set({ ruleResourceBusy: true });
    try {
      const config = await ipc.ruleResourcesDownloadCustom(name, category, url);
      set({ config });
      get().pushToast("success", getT().toasts.ruleResourceDownloaded(name));
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleResourceDownloadFailed(appErrorMessage(err)));
    } finally {
      set({ ruleResourceBusy: false });
    }
  },

  updateAllRuleResources: async () => {
    set({ ruleResourceBusy: true });
    try {
      const config = await ipc.ruleResourcesUpdateAll();
      set({ config });
      get().pushToast("success", getT().toasts.ruleResourcesUpdated);
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleResourcesUpdateFailed(appErrorMessage(err)));
    } finally {
      set({ ruleResourceBusy: false });
    }
  },

  deleteRuleResource: async (id) => {
    try {
      const config = await ipc.ruleResourcesDelete(id);
      set({ config });
      get().pushToast("success", getT().toasts.ruleResourceRemoved);
    } catch (err) {
      get().pushToast("error", getT().toasts.ruleResourceDeleteFailed(appErrorMessage(err)));
    }
  },
}));
