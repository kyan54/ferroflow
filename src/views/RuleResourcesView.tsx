import { useEffect, useState } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { formatBytes } from "../lib/utils";
import { RULE_RESOURCE_CATEGORIES } from "../types";
import type { RuleResourceCategory, RuleResourceInfo } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Select, Toggle } from "../components/ui";

const UPDATE_INTERVAL_OPTIONS = [6, 12, 24, 72, 168];

function catalogKey(name: string, category: RuleResourceCategory): string {
  return `${category}:${name}`;
}

export function RuleResourcesView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const saveConfig = useAppStore((s) => s.saveConfig);
  const ruleResourceCatalog = useAppStore((s) => s.ruleResourceCatalog);
  const refreshRuleResourceCatalog = useAppStore((s) => s.refreshRuleResourceCatalog);
  const ruleResourceBusy = useAppStore((s) => s.ruleResourceBusy);
  const downloadRuleResource = useAppStore((s) => s.downloadRuleResource);
  const downloadCustomRuleResource = useAppStore((s) => s.downloadCustomRuleResource);
  const updateAllRuleResources = useAppStore((s) => s.updateAllRuleResources);
  const deleteRuleResource = useAppStore((s) => s.deleteRuleResource);

  const [accelPrefix, setAccelPrefix] = useState("");
  const [selectedCatalogKey, setSelectedCatalogKey] = useState("");
  const [customName, setCustomName] = useState("");
  const [customCategory, setCustomCategory] = useState<RuleResourceCategory>("geosite");
  const [customUrl, setCustomUrl] = useState("");
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  useEffect(() => {
    refreshRuleResourceCatalog();
  }, [refreshRuleResourceCatalog]);

  useEffect(() => {
    if (config) setAccelPrefix(config.githubAccelPrefix ?? "");
  }, [config?.githubAccelPrefix]);

  useEffect(() => {
    if (!selectedCatalogKey && ruleResourceCatalog.length > 0) {
      setSelectedCatalogKey(catalogKey(ruleResourceCatalog[0].name, ruleResourceCatalog[0].category));
    }
  }, [ruleResourceCatalog, selectedCatalogKey]);

  if (!config) {
    return <div className="mx-auto max-w-3xl p-6 text-sm text-fg-faint">{t.common.loading}</div>;
  }

  const resources = config.ruleResources;

  function handleSaveAccelPrefix() {
    if (!config) return;
    const trimmed = accelPrefix.trim();
    saveConfig({ ...config, githubAccelPrefix: trimmed.length > 0 ? trimmed : null });
  }

  function toggleAutoUpdate() {
    if (!config) return;
    saveConfig({ ...config, ruleResourceAutoUpdate: !config.ruleResourceAutoUpdate });
  }

  function handleIntervalChange(hours: number) {
    if (!config) return;
    saveConfig({ ...config, ruleResourceAutoUpdateIntervalHours: hours });
  }

  function handleDownloadFromCatalog() {
    const entry = ruleResourceCatalog.find((e) => catalogKey(e.name, e.category) === selectedCatalogKey);
    if (!entry) return;
    downloadRuleResource(entry.category, entry.name);
  }

  function handleDownloadCustom(e: React.FormEvent) {
    e.preventDefault();
    if (!customName.trim() || !customUrl.trim()) return;
    downloadCustomRuleResource(customName.trim(), customCategory, customUrl.trim());
    setCustomName("");
    setCustomUrl("");
  }

  function handleDelete(id: string) {
    if (pendingDeleteId === id) {
      deleteRuleResource(id);
      setPendingDeleteId(null);
    } else {
      setPendingDeleteId(id);
    }
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <h1 className="font-display text-xl font-semibold text-fg">{t.ruleResources.title}</h1>
      <p className="text-sm text-fg-faint">{t.ruleResources.description}</p>

      <Card>
        <CardHeader>
          <CardTitle>{t.ruleResources.accel.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <p className="text-sm text-fg-faint">{t.ruleResources.accel.explainer}</p>
          <div className="flex gap-2">
            <Input
              value={accelPrefix}
              onChange={(e) => setAccelPrefix(e.target.value)}
              placeholder="https://ghproxy.com/"
              className="flex-1"
            />
            <Button onClick={handleSaveAccelPrefix}>{t.ruleResources.accel.save}</Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.ruleResources.autoUpdate.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <Toggle
            checked={config.ruleResourceAutoUpdate}
            onChange={toggleAutoUpdate}
            label={t.ruleResources.autoUpdate.toggleLabel}
          />
          <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
            {t.ruleResources.autoUpdate.intervalLabel}
            <Select
              value={config.ruleResourceAutoUpdateIntervalHours}
              onChange={(e) => handleIntervalChange(Number(e.target.value))}
              disabled={!config.ruleResourceAutoUpdate}
              className="max-w-[220px]"
            >
              {UPDATE_INTERVAL_OPTIONS.map((hours) => (
                <option key={hours} value={hours}>
                  {t.ruleResources.autoUpdate.intervalOption(hours)}
                </option>
              ))}
            </Select>
          </label>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.ruleResources.catalog.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex items-end gap-2 pt-4">
          <label className="flex flex-1 flex-col gap-1 text-sm font-medium text-fg-dim">
            {t.ruleResources.catalog.resourceLabel}
            <Select value={selectedCatalogKey} onChange={(e) => setSelectedCatalogKey(e.target.value)}>
              {ruleResourceCatalog.map((entry) => (
                <option key={catalogKey(entry.name, entry.category)} value={catalogKey(entry.name, entry.category)}>
                  {entry.label} ({t.ruleResources.categoryLabels[entry.category]})
                </option>
              ))}
            </Select>
          </label>
          <Button busy={ruleResourceBusy} disabled={!selectedCatalogKey} onClick={handleDownloadFromCatalog}>
            {t.ruleResources.catalog.download}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.ruleResources.custom.title}</CardTitle>
        </CardHeader>
        <form onSubmit={handleDownloadCustom}>
          <CardContent className="flex flex-col gap-3 pt-4">
            <div className="grid grid-cols-2 gap-3">
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.ruleResources.custom.name}
                <Input
                  required
                  value={customName}
                  onChange={(e) => setCustomName(e.target.value)}
                  placeholder="category-porn"
                />
              </label>
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.ruleResources.custom.category}
                <Select
                  value={customCategory}
                  onChange={(e) => setCustomCategory(e.target.value as RuleResourceCategory)}
                >
                  {RULE_RESOURCE_CATEGORIES.map((c) => (
                    <option key={c} value={c}>
                      {t.ruleResources.categoryLabels[c]}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="col-span-2 flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.ruleResources.custom.url}
                <Input
                  required
                  value={customUrl}
                  onChange={(e) => setCustomUrl(e.target.value)}
                  placeholder="https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-porn.srs"
                />
              </label>
            </div>
            <div className="flex justify-end">
              <Button type="submit" busy={ruleResourceBusy}>
                {t.ruleResources.custom.download}
              </Button>
            </div>
          </CardContent>
        </form>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.ruleResources.downloaded.title}</CardTitle>
          <Button
            variant="ghost"
            size="sm"
            disabled={resources.length === 0}
            busy={ruleResourceBusy}
            onClick={() => updateAllRuleResources()}
          >
            {t.ruleResources.downloaded.updateAll}
          </Button>
        </CardHeader>
        <CardContent className="pt-4">
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-line text-fg-faint">
                  <th className="py-2 pr-3 font-medium">{t.ruleResources.downloaded.columnName}</th>
                  <th className="py-2 pr-3 font-medium">{t.ruleResources.downloaded.columnCategory}</th>
                  <th className="py-2 pr-3 font-medium">{t.ruleResources.downloaded.columnSource}</th>
                  <th className="py-2 pr-3 font-medium">{t.ruleResources.downloaded.columnSize}</th>
                  <th className="py-2 pr-3 font-medium">{t.ruleResources.downloaded.columnDownloaded}</th>
                  <th className="py-2 pr-3 font-medium"></th>
                </tr>
              </thead>
              <tbody>
                {resources.map((resource: RuleResourceInfo) => (
                  <tr key={resource.id} className="border-b border-line/60 last:border-0">
                    <td className="py-2 pr-3 font-mono text-fg">{resource.name}</td>
                    <td className="py-2 pr-3 text-fg-dim">
                      {t.ruleResources.categoryLabels[resource.category]}
                      {resource.isBuiltin ? "" : t.ruleResources.downloaded.customSuffix}
                    </td>
                    <td className="max-w-[220px] truncate py-2 pr-3 text-fg-faint" title={resource.sourceUrl}>
                      {resource.sourceUrl}
                    </td>
                    <td className="py-2 pr-3 font-mono text-fg-dim">{formatBytes(resource.sizeBytes)}</td>
                    <td className="py-2 pr-3 text-fg-dim">{new Date(resource.downloadedAt).toLocaleString()}</td>
                    <td className="py-2 pr-3 text-right">
                      <div className="flex justify-end gap-2">
                        <button
                          onClick={() => downloadRuleResource(resource.category, resource.name)}
                          className="text-xs text-fg-faint hover:text-fg"
                        >
                          {t.ruleResources.downloaded.redownload}
                        </button>
                        <button
                          onClick={() => handleDelete(resource.id)}
                          onBlur={() => setPendingDeleteId(null)}
                          className="text-xs text-fg-faint hover:text-err"
                        >
                          {pendingDeleteId === resource.id
                            ? t.ruleResources.downloaded.confirmDelete
                            : t.ruleResources.downloaded.delete}
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            {resources.length === 0 && (
              <p className="mt-3 text-sm text-fg-faint">{t.ruleResources.downloaded.empty}</p>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
