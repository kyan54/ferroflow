// Full shape of one language's translation dictionary. `en.ts` and `zh.ts`
// each declare a `const` typed against this interface, so a key present in
// one but missing (or wrongly typed, e.g. string vs. function) in the other
// is a compile error rather than a silent runtime fallback.

import type { ProxyMode, ProxyModeType, UnlockStatus, RuleMatchType, RuleOutbound } from "../types";

export type Language = "en" | "zh";

export interface Dictionary {
  common: {
    cancel: string;
    save: string;
    delete: string;
    confirmDelete: string;
    confirmQuestion: string;
    refresh: string;
    loading: string;
    download: string;
    yes: string;
    no: string;
    dismiss: string;
  };

  nav: {
    appName: string;
    dashboard: string;
    servers: string;
    routingSection: string;
    rules: string;
    appRouting: string;
    ruleResources: string;
    diagnosticsSection: string;
    connections: string;
    logs: string;
    settings: string;
  };

  dashboard: {
    title: string;
    uptime: (label: string) => string;
    exitNode: {
      title: string;
      empty: string;
      selectPlaceholder: string;
      selectAriaLabel: string;
      openDashboardTooltip: string;
      stop: string;
      start: string;
      selectServerFirst: string;
    };
    takeoverMode: {
      title: string;
      ariaLabel: string;
      labels: Record<ProxyModeType, string>;
      tunWarning: string;
    };
    routingStrategy: {
      title: string;
      ariaLabel: string;
      labels: Record<ProxyMode, string>;
    };
    trafficFlow: {
      title: string;
      emptyNotRunning: string;
      emptyNoConnections: string;
      emptyIdle: string;
    };
    unlock: {
      title: string;
      checkButton: string;
      notRunning: string;
      explainer: string;
      statusLabels: Record<UnlockStatus, string>;
    };
    systemProxy: {
      title: string;
      enabled: string;
      disabled: string;
      refresh: string;
      httpProxy: string;
      httpsProxy: string;
      socksProxy: string;
      bypassList: string;
    };
    connected: string;
    disconnected: string;
    downloadTotal: (bytes: string) => string;
    uploadTotal: (bytes: string) => string;
  };

  servers: {
    title: string;
    getWarp: string;
    registeringWarp: string;
    import: string;
    addServer: string;
    empty: string;
    duplicate: string;
    delete: string;
    confirmDelete: string;
    tlsBadge: string;
    importForm: {
      title: string;
      modeUrl: string;
      modePaste: string;
      modeFile: string;
      urlExplainer: string;
      urlLabel: string;
      pasteExplainer: string;
      pasteLabel: string;
      fileExplainer: string;
      chooseFile: string;
      cancel: string;
      submit: string;
    };
  };

  serverForm: {
    title: string;
    name: string;
    protocol: string;
    address: string;
    port: string;
    uuid: string;
    password: string;
    cipher: string;
    encryption: string;
    flow: string;
    wireguardPrivateKey: string;
    wireguardPeerPublicKey: string;
    wireguardPreSharedKey: string;
    wireguardLocalAddress: string;
    tlsLegend: string;
    tlsEnabled: string;
    tlsServerName: string;
    tlsInsecure: string;
    tlsRealityPublicKey: string;
    tlsRealityShortId: string;
    cancel: string;
    submit: string;
  };

  rules: {
    title: string;
    addRule: string;
    description: string;
    empty: string;
    enabledAriaLabel: string;
    moveUp: string;
    moveDown: string;
    delete: string;
    confirmDelete: string;
  };

  ruleForm: {
    title: string;
    name: string;
    matchType: string;
    matchTypeLabels: Record<RuleMatchType, string>;
    ruleSetResources: string;
    noRuleResources: string;
    values: string;
    outbound: string;
    outboundLabels: Record<RuleOutbound, string>;
    enabled: string;
    cancel: string;
    submit: string;
  };

  appRouting: {
    title: string;
    description: string;
    presetsTitle: string;
    presetsExplainer: string;
    presetApply: string;
    presetConfirm: string;
    routeAriaLabel: (appLabel: string) => string;
    routeOptions: {
      off: string;
      proxy: string;
      direct: string;
      block: string;
    };
    categories: {
      streaming: string;
      social: string;
      ai: string;
      gaming: string;
      devtools: string;
    };
    presets: {
      "cn-direct": { label: string; description: string };
      "streaming-proxy": { label: string; description: string };
      "ads-cn-direct": { label: string; description: string };
      "global-proxy": { label: string; description: string };
    };
  };

  connections: {
    title: string;
    activeTitle: string;
    refresh: string;
    closeAll: string;
    totalDownloaded: string;
    totalUploaded: string;
    columnDestination: string;
    columnNetwork: string;
    columnChain: string;
    columnRule: string;
    columnDownload: string;
    columnUpload: string;
    columnEnded: string;
    close: string;
    emptyActive: string;
    historyTitle: string;
    clearHistory: string;
    historyExplainer: string;
    emptyHistory: string;
  };

  logs: {
    title: string;
    cardTitle: string;
    refresh: string;
    copyAll: string;
    clear: string;
    explainer: (count: number) => string;
    empty: string;
  };

  ruleResources: {
    title: string;
    description: string;
    categoryLabels: { geosite: string; geoIp: string };
    accel: {
      title: string;
      explainer: string;
      save: string;
    };
    autoUpdate: {
      title: string;
      toggleLabel: string;
      intervalLabel: string;
      intervalOption: (hours: number) => string;
    };
    catalog: {
      title: string;
      resourceLabel: string;
      download: string;
    };
    custom: {
      title: string;
      name: string;
      category: string;
      url: string;
      download: string;
    };
    downloaded: {
      title: string;
      updateAll: string;
      columnName: string;
      columnCategory: string;
      columnSource: string;
      columnSize: string;
      columnDownloaded: string;
      customSuffix: string;
      redownload: string;
      delete: string;
      confirmDelete: string;
      empty: string;
    };
  };

  settings: {
    title: string;
    platform: {
      title: string;
      os: string;
      architecture: string;
      osVersion: string;
      runningAsAdmin: string;
    };
    language: {
      title: string;
      english: string;
      chinese: string;
    };
    theme: {
      title: string;
      system: string;
      light: string;
      dark: string;
    };
    behavior: {
      title: string;
      autoStart: string;
      minimizeToTray: string;
      recordHistory: string;
      recordHistoryExplainer: string;
      movedNote: string;
    };
    helper: {
      title: string;
      explainer: string;
      status: string;
      ready: string;
      notInstalled: string;
      version: string;
      checking: string;
      install: string;
      installing: string;
      remove: string;
      removing: string;
      confirmRemove: string;
    };
    backup: {
      title: string;
      explainer: string;
      exportBackup: string;
      importBackup: string;
      exportDiagnostic: string;
    };
  };

  toasts: {
    loadConfigFailed: (msg: string) => string;
    loadPlatformInfoFailed: (msg: string) => string;
    helperStatusUnavailable: (msg: string) => string;
    systemProxyStatusUnavailable: (msg: string) => string;
    helperInstalled: string;
    helperInstallFailed: (msg: string) => string;
    helperRemoved: string;
    helperRemoveFailed: (msg: string) => string;
    settingsSaved: string;
    settingsSaveFailed: (msg: string) => string;
    serverAdded: (name: string) => string;
    serverAddFailed: (msg: string) => string;
    serverRemoved: string;
    serverDeleteFailed: (msg: string) => string;
    serverDuplicated: (name: string) => string;
    serverDuplicateFailed: (msg: string) => string;
    serverSelectFailed: (msg: string) => string;
    serversImported: (n: number) => string;
    subscriptionImportFailed: (msg: string) => string;
    serversImportFailed: (msg: string) => string;
    fileImportFailed: (msg: string) => string;
    warpRegistered: string;
    warpRegisterFailed: (msg: string) => string;
    ruleAdded: (name: string) => string;
    ruleAddFailed: (msg: string) => string;
    ruleUpdateFailed: (msg: string) => string;
    ruleRemoved: string;
    ruleDeleteFailed: (msg: string) => string;
    ruleReorderFailed: (msg: string) => string;
    appRoutingUpdateFailed: (appLabel: string, msg: string) => string;
    presetApplied: (label: string) => string;
    presetApplyFailed: (msg: string) => string;
    proxyStartFailed: (msg: string) => string;
    proxyStopFailed: (msg: string) => string;
    dashboardOpenFailed: (msg: string) => string;
    connectionCloseFailed: (msg: string) => string;
    connectionsCloseAllFailed: (msg: string) => string;
    historyLoadFailed: (msg: string) => string;
    historyCleared: string;
    historyClearFailed: (msg: string) => string;
    logsLoadFailed: (msg: string) => string;
    logsCleared: string;
    logsClearFailed: (msg: string) => string;
    backupExported: string;
    backupExportFailed: (msg: string) => string;
    backupImported: string;
    backupImportFailed: (msg: string) => string;
    diagnosticExported: string;
    diagnosticExportFailed: (msg: string) => string;
    ruleResourceCatalogLoadFailed: (msg: string) => string;
    ruleResourceDownloaded: (name: string) => string;
    ruleResourceDownloadFailed: (msg: string) => string;
    ruleResourcesUpdated: string;
    ruleResourcesUpdateFailed: (msg: string) => string;
    ruleResourceRemoved: string;
    ruleResourceDeleteFailed: (msg: string) => string;
    logsCopied: string;
    logsCopyFailed: (msg: string) => string;
  };
}
