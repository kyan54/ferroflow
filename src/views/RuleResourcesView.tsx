import { useEffect, useMemo, useRef, useState } from "react";
import type { SVGProps } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { formatBytes } from "../lib/utils";
import { RULE_RESOURCE_CATEGORIES } from "../types";
import type { RuleResourceCategory, RuleResourceInfo } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Select, Toggle, Badge } from "../components/ui";

const UPDATE_INTERVAL_OPTIONS = [6, 12, 24, 72, 168];

function catalogKey(name: string, category: RuleResourceCategory): string {
  return `${category}:${name}`;
}

function icon(props: SVGProps<SVGSVGElement>) {
  return {
    viewBox: "0 0 24 24",
    fill: "none" as const,
    stroke: "currentColor",
    strokeWidth: 1.9,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    ...props,
  };
}

function SearchIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <circle cx="11" cy="11" r="7" />
      <path d="M21 21l-4.3-4.3" />
    </svg>
  );
}

function ChevronDownIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}

function PlusIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

function RefreshIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M21 12a9 9 0 1 1-3-6.7" />
      <path d="M21 3v6h-6" />
    </svg>
  );
}

function TrashIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M4 7h16M9 7V4h6v3M6 7l1 13a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-13" />
    </svg>
  );
}

type AddMode = "catalog" | "custom";

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
  const [showAddMenu, setShowAddMenu] = useState(false);
  const [addMode, setAddMode] = useState<AddMode | null>(null);
  const [tableSearch, setTableSearch] = useState("");

  const addMenuRef = useRef<HTMLDivElement>(null);

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

  useEffect(() => {
    if (!showAddMenu) return;
    function handleClick(e: MouseEvent) {
      if (addMenuRef.current && !addMenuRef.current.contains(e.target as Node)) {
        setShowAddMenu(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [showAddMenu]);

  const resources = config?.ruleResources ?? [];

  const totalBytes = useMemo(() => resources.reduce((sum, r) => sum + r.sizeBytes, 0), [resources]);

  const q = tableSearch.trim().toLowerCase();
  const visibleResources = useMemo(
    () => (q ? resources.filter((r) => r.name.toLowerCase().includes(q)) : resources),
    [resources, q],
  );

  if (!config) {
    return <div className="mx-auto max-w-3xl p-6 text-sm text-fg-faint">{t.common.loading}</div>;
  }

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

  function handleDownloadFromCatalog(e: React.FormEvent) {
    e.preventDefault();
    const entry = ruleResourceCatalog.find((entry) => catalogKey(entry.name, entry.category) === selectedCatalogKey);
    if (!entry) return;
    downloadRuleResource(entry.category, entry.name);
    setAddMode(null);
  }

  function handleDownloadCustom(e: React.FormEvent) {
    e.preventDefault();
    if (!customName.trim() || !customUrl.trim()) return;
    downloadCustomRuleResource(customName.trim(), customCategory, customUrl.trim());
    setCustomName("");
    setCustomUrl("");
    setAddMode(null);
  }

  function handleDelete(id: string) {
    if (pendingDeleteId === id) {
      deleteRuleResource(id);
      setPendingDeleteId(null);
    } else {
      setPendingDeleteId(id);
    }
  }

  function openAddCatalog() {
    setShowAddMenu(false);
    setAddMode("catalog");
  }

  function openAddCustom() {
    setShowAddMenu(false);
    setAddMode("custom");
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="font-display text-xl font-semibold text-fg">{t.ruleResources.title}</h1>
          <p className="mt-0.5 text-xs text-fg-faint">
            {t.ruleResources.subtitle(resources.length, formatBytes(totalBytes))}
          </p>
        </div>

        {addMode === null && (
          <div className="relative" ref={addMenuRef}>
            <Button size="sm" onClick={() => setShowAddMenu((v) => !v)}>
              <PlusIcon className="h-3.5 w-3.5" />
              {t.ruleResources.addMenu.button}
              <ChevronDownIcon className="h-3.5 w-3.5" />
            </Button>
            {showAddMenu && (
              <div
                role="menu"
                className="absolute right-0 top-full z-20 mt-1 w-56 overflow-hidden rounded-lg border border-line bg-surface py-1 shadow-lg animate-dropdown-in"
              >
                <button
                  type="button"
                  role="menuitem"
                  onClick={openAddCatalog}
                  className="flex w-full items-center px-3 py-2 text-left text-sm text-fg-dim transition-colors hover:bg-surface-2 hover:text-fg"
                >
                  {t.ruleResources.catalog.title}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={openAddCustom}
                  className="flex w-full items-center px-3 py-2 text-left text-sm text-fg-dim transition-colors hover:bg-surface-2 hover:text-fg"
                >
                  {t.ruleResources.custom.title}
                </button>
              </div>
            )}
          </div>
        )}
      </div>

      <p className="text-sm text-fg-faint">{t.ruleResources.description}</p>

      {addMode === "catalog" && (
        <Card className="animate-fade-in-up">
          <form onSubmit={handleDownloadFromCatalog}>
            <CardHeader>
              <CardTitle>{t.ruleResources.catalog.title}</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-3 pt-4">
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.ruleResources.catalog.resourceLabel}
                <Select value={selectedCatalogKey} onChange={(e) => setSelectedCatalogKey(e.target.value)}>
                  {ruleResourceCatalog.map((entry) => (
                    <option
                      key={catalogKey(entry.name, entry.category)}
                      value={catalogKey(entry.name, entry.category)}
                    >
                      {entry.label} ({t.ruleResources.categoryLabels[entry.category]})
                    </option>
                  ))}
                </Select>
              </label>
              <div className="flex justify-end gap-2">
                <Button type="button" variant="ghost" onClick={() => setAddMode(null)}>
                  {t.ruleResources.addMenu.cancel}
                </Button>
                <Button type="submit" busy={ruleResourceBusy} disabled={!selectedCatalogKey}>
                  {t.ruleResources.catalog.download}
                </Button>
              </div>
            </CardContent>
          </form>
        </Card>
      )}

      {addMode === "custom" && (
        <Card className="animate-fade-in-up">
          <form onSubmit={handleDownloadCustom}>
            <CardHeader>
              <CardTitle>{t.ruleResources.custom.title}</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-3 pt-4">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
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
              <div className="flex justify-end gap-2">
                <Button type="button" variant="ghost" onClick={() => setAddMode(null)}>
                  {t.ruleResources.addMenu.cancel}
                </Button>
                <Button type="submit" busy={ruleResourceBusy}>
                  {t.ruleResources.custom.download}
                </Button>
              </div>
            </CardContent>
          </form>
        </Card>
      )}

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
          <CardTitle>{t.ruleResources.downloaded.title}</CardTitle>
          <Button
            variant="ghost"
            size="sm"
            disabled={resources.length === 0}
            busy={ruleResourceBusy}
            onClick={() => updateAllRuleResources()}
          >
            <RefreshIcon className="h-3.5 w-3.5" />
            {t.ruleResources.downloaded.updateAll}
          </Button>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          {resources.length > 0 && (
            <div className="relative">
              <SearchIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-faint" />
              <Input
                aria-label={t.ruleResources.downloaded.searchAriaLabel}
                value={tableSearch}
                onChange={(e) => setTableSearch(e.target.value)}
                placeholder={t.ruleResources.downloaded.searchPlaceholder}
                className="pl-9"
              />
            </div>
          )}

          {resources.length === 0 ? (
            <p className="text-sm text-fg-faint">{t.ruleResources.downloaded.empty}</p>
          ) : visibleResources.length === 0 ? (
            <p className="text-sm text-fg-faint">{t.ruleResources.downloaded.noResults}</p>
          ) : (
            <div className="-mx-5 overflow-x-auto animate-fade-in">
              {/* No fixed min-width -- see RulesView's rule-list table for
                  the same reasoning: column width hints plus the source-url
                  column's truncation let the table shrink with the card
                  instead of forcing a horizontal scrollbar at normal sizes. */}
              <table className="w-full border-collapse text-sm">
                <thead>
                  <tr className="border-b border-line text-left text-xs font-medium uppercase tracking-wide text-fg-faint">
                    <th className="py-2 pl-5 pr-2 font-medium">{t.ruleResources.downloaded.columnName}</th>
                    <th className="w-24 py-2 px-2 font-medium">{t.ruleResources.downloaded.columnCategory}</th>
                    <th className="py-2 px-2 font-medium">{t.ruleResources.downloaded.columnSource}</th>
                    <th className="w-24 py-2 px-2 text-right font-medium">
                      {t.ruleResources.downloaded.columnSize}
                    </th>
                    <th className="w-36 py-2 px-2 font-medium">{t.ruleResources.downloaded.columnDownloaded}</th>
                    <th className="w-24 py-2 pl-2 pr-5 text-right font-medium">
                      {t.ruleResources.downloaded.columnActions}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {visibleResources.map((resource: RuleResourceInfo) => (
                    <tr key={resource.id} className="border-b border-line last:border-0">
                      <td className="py-3 pl-5 pr-2">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="font-mono text-fg">{resource.name}</span>
                          {!resource.isBuiltin && (
                            <Badge variant="outline">{t.ruleResources.downloaded.customBadge}</Badge>
                          )}
                        </div>
                      </td>
                      <td className="py-3 px-2">
                        <Badge variant={resource.category}>{t.ruleResources.categoryLabels[resource.category]}</Badge>
                      </td>
                      <td className="max-w-[220px] truncate py-3 px-2 text-fg-faint" title={resource.sourceUrl}>
                        {resource.sourceUrl}
                      </td>
                      <td className="py-3 px-2 text-right font-mono text-fg-dim">
                        {formatBytes(resource.sizeBytes)}
                      </td>
                      <td className="py-3 px-2 text-fg-dim">{new Date(resource.downloadedAt).toLocaleString()}</td>
                      <td className="py-3 pl-2 pr-5">
                        <div className="flex items-center justify-end gap-0.5">
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => downloadRuleResource(resource.category, resource.name)}
                            title={t.ruleResources.downloaded.redownload}
                            aria-label={t.ruleResources.downloaded.redownload}
                          >
                            <RefreshIcon className="h-4 w-4" />
                          </Button>
                          <Button
                            variant={pendingDeleteId === resource.id ? "destructive" : "ghost"}
                            size="icon"
                            onClick={() => handleDelete(resource.id)}
                            onBlur={() => setPendingDeleteId(null)}
                            title={
                              pendingDeleteId === resource.id
                                ? t.ruleResources.downloaded.confirmDelete
                                : t.ruleResources.downloaded.delete
                            }
                            aria-label={
                              pendingDeleteId === resource.id
                                ? t.ruleResources.downloaded.confirmDelete
                                : t.ruleResources.downloaded.delete
                            }
                          >
                            <TrashIcon className="h-4 w-4" />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
