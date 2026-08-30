import { useEffect, useState } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { Card, CardHeader, CardTitle, CardContent, Button, Toggle, SegmentedControl } from "../components/ui";

export function SettingsView() {
  const { t, language } = useTranslation();
  const config = useAppStore((s) => s.config);
  const platformInfo = useAppStore((s) => s.platformInfo);
  const refreshPlatformInfo = useAppStore((s) => s.refreshPlatformInfo);
  const saveConfig = useAppStore((s) => s.saveConfig);
  const setLanguage = useAppStore((s) => s.setLanguage);
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
      <div className="mx-auto max-w-2xl p-6 text-sm text-fg-faint">{t.common.loading}</div>
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
      <h1 className="font-display text-xl font-semibold text-fg">{t.settings.title}</h1>

      <Card>
        <CardHeader>
          <CardTitle>{t.settings.language.title}</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          <SegmentedControl
            aria-label={t.settings.language.title}
            value={language}
            onChange={(lang) => setLanguage(lang)}
            options={[
              { value: "en", label: t.settings.language.english },
              { value: "zh", label: t.settings.language.chinese },
            ]}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.settings.platform.title}</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          {platformInfo ? (
            <dl className="grid grid-cols-2 gap-y-2 text-sm">
              <dt className="text-fg-faint">{t.settings.platform.os}</dt>
              <dd className="text-fg-dim">{platformInfo.platform}</dd>
              <dt className="text-fg-faint">{t.settings.platform.architecture}</dt>
              <dd className="text-fg-dim">{platformInfo.arch}</dd>
              <dt className="text-fg-faint">{t.settings.platform.osVersion}</dt>
              <dd className="text-fg-dim">{platformInfo.osVersion || "—"}</dd>
              <dt className="text-fg-faint">{t.settings.platform.runningAsAdmin}</dt>
              <dd className="text-fg-dim">{platformInfo.isAdmin ? t.common.yes : t.common.no}</dd>
            </dl>
          ) : (
            <p className="text-sm text-fg-faint">{t.common.loading}</p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.settings.behavior.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <Toggle
            checked={config.autoStart}
            onChange={() => toggle("autoStart")}
            label={t.settings.behavior.autoStart}
          />
          <Toggle
            checked={config.minimizeToTray}
            onChange={() => toggle("minimizeToTray")}
            label={t.settings.behavior.minimizeToTray}
          />
          <div>
            <Toggle
              checked={config.connectionHistoryEnabled}
              onChange={() => toggle("connectionHistoryEnabled")}
              label={t.settings.behavior.recordHistory}
            />
            <p className="mt-1 text-xs text-fg-faint">{t.settings.behavior.recordHistoryExplainer}</p>
          </div>

          <p className="mt-1 text-xs text-fg-faint">{t.settings.behavior.movedNote}</p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.settings.helper.title}</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          <p className="text-sm text-fg-faint">{t.settings.helper.explainer}</p>

          {helperStatus ? (
            <dl className="mt-4 grid grid-cols-2 gap-y-2 text-sm">
              <dt className="text-fg-faint">{t.settings.helper.status}</dt>
              <dd className="text-fg-dim">
                {helperStatus.ready ? t.settings.helper.ready : t.settings.helper.notInstalled}
              </dd>
              {helperStatus.version && (
                <>
                  <dt className="text-fg-faint">{t.settings.helper.version}</dt>
                  <dd className="text-fg-dim">{helperStatus.version}</dd>
                </>
              )}
            </dl>
          ) : (
            <p className="mt-4 text-sm text-fg-faint">{t.settings.helper.checking}</p>
          )}

          <div className="mt-4 flex gap-2">
            {!helperStatus?.ready ? (
              <Button busy={helperBusy} onClick={() => installHelper()}>
                {helperBusy ? t.settings.helper.installing : t.settings.helper.install}
              </Button>
            ) : (
              <Button
                variant="destructive"
                busy={helperBusy}
                onClick={handleUninstall}
                onBlur={() => setPendingUninstall(false)}
              >
                {helperBusy
                  ? t.settings.helper.removing
                  : pendingUninstall
                    ? t.settings.helper.confirmRemove
                    : t.settings.helper.remove}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.settings.backup.title}</CardTitle>
        </CardHeader>
        <CardContent className="pt-4">
          <p className="text-sm text-fg-faint">{t.settings.backup.explainer}</p>

          <div className="mt-4 flex flex-wrap gap-2">
            <Button onClick={() => exportBackup()}>{t.settings.backup.exportBackup}</Button>
            <Button variant="outline" onClick={() => importBackup()}>
              {t.settings.backup.importBackup}
            </Button>
            <Button onClick={() => exportDiagnostic()}>{t.settings.backup.exportDiagnostic}</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
