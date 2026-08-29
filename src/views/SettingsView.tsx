import { useEffect, useState } from "react";
import { useAppStore } from "../store";
import { Card, CardHeader, CardTitle, CardContent, Button, Toggle } from "../components/ui";

export function SettingsView() {
  const config = useAppStore((s) => s.config);
  const platformInfo = useAppStore((s) => s.platformInfo);
  const refreshPlatformInfo = useAppStore((s) => s.refreshPlatformInfo);
  const saveConfig = useAppStore((s) => s.saveConfig);
  const helperStatus = useAppStore((s) => s.helperStatus);
  const helperBusy = useAppStore((s) => s.helperBusy);
  const refreshHelperStatus = useAppStore((s) => s.refreshHelperStatus);
  const installHelper = useAppStore((s) => s.installHelper);
  const uninstallHelper = useAppStore((s) => s.uninstallHelper);
  const exportBackup = useAppStore((s) => s.exportBackup);
  const importBackup = useAppStore((s) => s.importBackup);
  const exportDiagnostic = useAppStore((s) => s.exportDiagnostic);

  const [pendingUninstall, setPendingUninstall] = useState(false);

  useEffect(() => {
    refreshPlatformInfo();
    refreshHelperStatus();
  }, [refreshPlatformInfo, refreshHelperStatus]);

  if (!config) {
    return (
      <div className="mx-auto max-w-2xl p-6 text-sm text-fg-faint">Loading…</div>
    );
  }

  function toggle(key: "autoStart" | "minimizeToTray" | "connectionHistoryEnabled") {
    if (!config) return;
    saveConfig({ ...config, [key]: !config[key] });
  }

  function handleUninstall() {
    if (pendingUninstall) {
      uninstallHelper();
      setPendingUninstall(false);
    } else {
      setPendingUninstall(true);
    }
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-6 p-6">
      <h1 className="font-display text-xl font-semibold text-fg">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Platform</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          {platformInfo ? (
            <dl className="grid grid-cols-2 gap-y-2 text-sm">
              <dt className="text-fg-faint">OS</dt>
              <dd className="text-fg-dim">{platformInfo.platform}</dd>
              <dt className="text-fg-faint">Architecture</dt>
              <dd className="text-fg-dim">{platformInfo.arch}</dd>
              <dt className="text-fg-faint">OS version</dt>
              <dd className="text-fg-dim">{platformInfo.osVersion || "—"}</dd>
              <dt className="text-fg-faint">Running as admin</dt>
              <dd className="text-fg-dim">{platformInfo.isAdmin ? "Yes" : "No"}</dd>
            </dl>
          ) : (
            <p className="text-sm text-fg-faint">Loading…</p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Behavior</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <Toggle
            checked={config.autoStart}
            onChange={() => toggle("autoStart")}
            label="Start automatically on login"
          />
          <Toggle
            checked={config.minimizeToTray}
            onChange={() => toggle("minimizeToTray")}
            label="Minimize to tray on close"
          />
          <div>
            <Toggle
              checked={config.connectionHistoryEnabled}
              onChange={() => toggle("connectionHistoryEnabled")}
              label="Record connection history"
            />
            <p className="mt-1 text-xs text-fg-faint">
              Off by default. Only applies the next time the proxy starts -- toggling this while
              already connected does not retroactively record the current session. Recorded
              locally as plain, unencrypted JSON, capped at the most recent 1000 finished
              connections.
            </p>
          </div>

          <p className="mt-1 text-xs text-fg-faint">
            Takeover mode and routing strategy moved to the Dashboard — see the "Takeover mode"
            and "Routing strategy" cards there.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Privileged helper</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          <p className="text-sm text-fg-faint">
            Required for TUN mode. Installed once with a single admin prompt; after that, starting
            and stopping the proxy never prompts again.
          </p>

          {helperStatus ? (
            <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
              <dt className="text-fg-faint">Status</dt>
              <dd className="text-fg-dim">{helperStatus.ready ? "Installed and running" : "Not installed"}</dd>
              {helperStatus.version && (
                <>
                  <dt className="text-fg-faint">Version</dt>
                  <dd className="text-fg-dim">{helperStatus.version}</dd>
                </>
              )}
            </dl>
          ) : (
            <p className="mt-4 text-sm text-fg-faint">Checking…</p>
          )}

          <div className="mt-4 flex gap-2">
            {!helperStatus?.ready ? (
              <Button busy={helperBusy} onClick={() => installHelper()}>
                {helperBusy ? "Installing…" : "Install helper"}
              </Button>
            ) : (
              <Button
                variant="destructive"
                busy={helperBusy}
                onClick={handleUninstall}
                onBlur={() => setPendingUninstall(false)}
              >
                {helperBusy ? "Removing…" : pendingUninstall ? "Confirm remove?" : "Remove helper"}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Backup & diagnostics</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          <p className="text-sm text-fg-faint">
            Back up your servers, rules, and settings to a file you can move to another machine, or
            export a redacted diagnostic report safe to paste into a bug report.
          </p>

          <div className="mt-4 flex flex-wrap gap-2">
            <Button onClick={() => exportBackup()}>Export backup</Button>
            <Button variant="outline" onClick={() => importBackup()}>
              Import backup
            </Button>
            <Button onClick={() => exportDiagnostic()}>Export diagnostic report</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
