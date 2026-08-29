import { useEffect, useState } from "react";
import { useAppStore } from "../store";
import { formatBytes } from "../lib/utils";
import { RULE_RESOURCE_CATEGORIES } from "../types";
import type { RuleResourceCategory, RuleResourceInfo } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Select, Toggle } from "../components/ui";

const CATEGORY_LABELS: Record<RuleResourceCategory, string> = {
  geosite: "GeoSite",
  geoIp: "GeoIP",
};

const UPDATE_INTERVAL_OPTIONS = [6, 12, 24, 72, 168];

function catalogKey(name: string, category: RuleResourceCategory): string {
  return `${category}:${name}`;
}

export function RuleResourcesView() {
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
    return <div className="mx-auto max-w-3xl p-6 text-sm text-fg-faint">Loading…</div>;
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
      <h1 className="font-display text-xl font-semibold text-fg">Rule resources</h1>
      <p className="text-sm text-fg-faint">
        GeoIP/GeoSite rule-set files (sing-box's <code>.srs</code> binary format), downloaded once and
        referenced by name from a routing rule with match type "Rule set" instead of typing thousands
        of domains by hand.
      </p>

      <Card>
        <CardHeader>
          <CardTitle>GitHub acceleration</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <p className="text-sm text-fg-faint">
            Optional mirror prefix prepended in front of the real{" "}
            <code>raw.githubusercontent.com</code> download URL -- useful when that host is slow or
            blocked. Leave blank to fetch directly.
          </p>
          <div className="flex gap-2">
            <Input
              value={accelPrefix}
              onChange={(e) => setAccelPrefix(e.target.value)}
              placeholder="https://ghproxy.com/"
              className="flex-1"
            />
            <Button onClick={handleSaveAccelPrefix}>Save</Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Auto update</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <Toggle
            checked={config.ruleResourceAutoUpdate}
            onChange={toggleAutoUpdate}
            label="Periodically re-download tracked rule resources"
          />
          <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
            Interval
            <Select
              value={config.ruleResourceAutoUpdateIntervalHours}
              onChange={(e) => handleIntervalChange(Number(e.target.value))}
              disabled={!config.ruleResourceAutoUpdate}
              className="max-w-[220px]"
            >
              {UPDATE_INTERVAL_OPTIONS.map((hours) => (
                <option key={hours} value={hours}>
                  Every {hours < 24 ? `${hours}h` : `${hours / 24}d`}
                </option>
              ))}
            </Select>
          </label>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Add from catalog</CardTitle>
        </CardHeader>
        <CardContent className="flex items-end gap-2 pt-4">
          <label className="flex flex-1 flex-col gap-1 text-sm font-medium text-fg-dim">
            Resource
            <Select value={selectedCatalogKey} onChange={(e) => setSelectedCatalogKey(e.target.value)}>
              {ruleResourceCatalog.map((entry) => (
                <option key={catalogKey(entry.name, entry.category)} value={catalogKey(entry.name, entry.category)}>
                  {entry.label} ({CATEGORY_LABELS[entry.category]})
                </option>
              ))}
            </Select>
          </label>
          <Button busy={ruleResourceBusy} disabled={!selectedCatalogKey} onClick={handleDownloadFromCatalog}>
            Download
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Add custom</CardTitle>
        </CardHeader>
        <form onSubmit={handleDownloadCustom}>
          <CardContent className="flex flex-col gap-3 pt-4">
            <div className="grid grid-cols-2 gap-3">
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                Name
                <Input
                  required
                  value={customName}
                  onChange={(e) => setCustomName(e.target.value)}
                  placeholder="category-porn"
                />
              </label>
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                Category
                <Select
                  value={customCategory}
                  onChange={(e) => setCustomCategory(e.target.value as RuleResourceCategory)}
                >
                  {RULE_RESOURCE_CATEGORIES.map((c) => (
                    <option key={c} value={c}>
                      {CATEGORY_LABELS[c]}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="col-span-2 flex flex-col gap-1 text-sm font-medium text-fg-dim">
                URL
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
                Download
              </Button>
            </div>
          </CardContent>
        </form>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Downloaded resources</CardTitle>
          <Button
            variant="ghost"
            size="sm"
            disabled={resources.length === 0}
            busy={ruleResourceBusy}
            onClick={() => updateAllRuleResources()}
          >
            Update all
          </Button>
        </CardHeader>
        <CardContent className="pt-4">
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-line text-fg-faint">
                  <th className="py-2 pr-3 font-medium">Name</th>
                  <th className="py-2 pr-3 font-medium">Category</th>
                  <th className="py-2 pr-3 font-medium">Source</th>
                  <th className="py-2 pr-3 font-medium">Size</th>
                  <th className="py-2 pr-3 font-medium">Downloaded</th>
                  <th className="py-2 pr-3 font-medium"></th>
                </tr>
              </thead>
              <tbody>
                {resources.map((resource: RuleResourceInfo) => (
                  <tr key={resource.id} className="border-b border-line/60 last:border-0">
                    <td className="py-2 pr-3 font-mono text-fg">{resource.name}</td>
                    <td className="py-2 pr-3 text-fg-dim">
                      {CATEGORY_LABELS[resource.category]}
                      {resource.isBuiltin ? "" : " (custom)"}
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
                          Re-download
                        </button>
                        <button
                          onClick={() => handleDelete(resource.id)}
                          onBlur={() => setPendingDeleteId(null)}
                          className="text-xs text-fg-faint hover:text-err"
                        >
                          {pendingDeleteId === resource.id ? "Confirm?" : "Delete"}
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            {resources.length === 0 && (
              <p className="mt-3 text-sm text-fg-faint">
                No rule resources downloaded yet. Add one from the catalog above.
              </p>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
