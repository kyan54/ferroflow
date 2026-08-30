import { useMemo, useState } from "react";
import type { SVGProps } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { APP_ROUTING_CATALOG, REGION_PRESETS, appRoutingRuleId } from "../lib/appRouting";
import type { RuleOutbound } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Badge, SegmentedControl } from "../components/ui";
import type { SegmentedOption } from "../components/ui";

type AppRouteValue = "off" | RuleOutbound;

/** Only the destructive "Global proxy, no rules" preset renders here -- the
 * other, non-destructive region presets now live as a single-click pill row
 * on the Rules page (see `RulesView`'s `REGION_PILL_PRESETS`). This page
 * keeps the one preset that wipes every rule (including the app routes
 * below), since that's a deliberately confirm-guarded action distinct from
 * Rules' one-click pills. */
const DANGER_PRESETS = REGION_PRESETS.filter((preset) => preset.clearsAllRules);

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

export function AppRoutingView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const appRoutingBusy = useAppStore((s) => s.appRoutingBusy);
  const regionPresetBusy = useAppStore((s) => s.regionPresetBusy);
  const setAppRoute = useAppStore((s) => s.setAppRoute);
  const applyRegionPreset = useAppStore((s) => s.applyRegionPreset);
  const [pendingPresetId, setPendingPresetId] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  const ROUTE_OPTIONS: SegmentedOption<AppRouteValue>[] = [
    { value: "off", label: t.appRouting.routeOptions.off },
    { value: "proxy", label: t.appRouting.routeOptions.proxy },
    { value: "direct", label: t.appRouting.routeOptions.direct },
    { value: "block", label: t.appRouting.routeOptions.block },
  ];

  const rules = config?.rules ?? [];

  function currentValue(appId: string): AppRouteValue {
    const rule = rules.find((r) => r.id === appRoutingRuleId(appId));
    return rule ? rule.outbound : "off";
  }

  const totalApps = useMemo(
    () => APP_ROUTING_CATALOG.reduce((sum, category) => sum + category.apps.length, 0),
    [],
  );
  const routedApps = useMemo(
    () =>
      APP_ROUTING_CATALOG.reduce(
        (sum, category) => sum + category.apps.filter((app) => currentValue(app.id) !== "off").length,
        0,
      ),
    [rules],
  );

  const q = search.trim().toLowerCase();
  const visibleCategories = useMemo(
    () =>
      APP_ROUTING_CATALOG.map((category) => ({
        category,
        apps: q ? category.apps.filter((app) => app.label.toLowerCase().includes(q)) : category.apps,
      })).filter((entry) => entry.apps.length > 0),
    [q],
  );

  if (!config) {
    return <div className="mx-auto max-w-3xl p-6 text-sm text-fg-faint">{t.common.loading}</div>;
  }

  function handlePresetClick(presetId: string) {
    if (pendingPresetId === presetId) {
      applyRegionPreset(presetId);
      setPendingPresetId(null);
    } else {
      setPendingPresetId(presetId);
    }
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="font-display text-xl font-semibold text-fg">{t.appRouting.title}</h1>
          <p className="mt-0.5 text-xs text-fg-faint">{t.appRouting.subtitle(routedApps, totalApps)}</p>
        </div>
      </div>

      <p className="text-sm text-fg-faint">{t.appRouting.description}</p>

      <div className="relative">
        <SearchIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-faint" />
        <Input
          aria-label={t.appRouting.search.ariaLabel}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t.appRouting.search.placeholder}
          className="pl-9"
        />
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t.appRouting.presetsTitle}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <p className="text-sm text-fg-faint">{t.appRouting.presetsExplainer}</p>
          {DANGER_PRESETS.map((preset) => {
            const armed = pendingPresetId === preset.id;
            const presetText = t.appRouting.presets[preset.id as keyof typeof t.appRouting.presets];
            return (
              <div
                key={preset.id}
                className="flex items-center justify-between gap-3 rounded-md border border-line bg-surface-2 px-3 py-2.5"
              >
                <div className="min-w-0">
                  <p className="text-sm font-medium text-fg">{presetText.label}</p>
                  <p className="mt-0.5 text-xs text-fg-faint">{presetText.description}</p>
                </div>
                <Button
                  variant={armed ? "destructive" : "outline"}
                  size="sm"
                  busy={regionPresetBusy}
                  onClick={() => handlePresetClick(preset.id)}
                  onBlur={() => setPendingPresetId(null)}
                  className="shrink-0"
                >
                  {armed ? t.appRouting.presetConfirm : t.appRouting.presetApply}
                </Button>
              </div>
            );
          })}
        </CardContent>
      </Card>

      {visibleCategories.length === 0 ? (
        <Card>
          <CardContent className="text-sm text-fg-faint">{t.appRouting.noResults}</CardContent>
        </Card>
      ) : (
        visibleCategories.map(({ category, apps }) => {
          const routedInCategory = category.apps.filter((app) => currentValue(app.id) !== "off").length;
          return (
            <Card key={category.id}>
              <CardHeader>
                <CardTitle>{t.appRouting.categories[category.id as keyof typeof t.appRouting.categories]}</CardTitle>
                <Badge variant="secondary">
                  {t.appRouting.categoryRouted(routedInCategory, category.apps.length)}
                </Badge>
              </CardHeader>
              <CardContent className="flex flex-col divide-y divide-line pt-4">
                {apps.map((app) => (
                  <div
                    key={app.id}
                    className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2 py-2.5 first:pt-0 last:pb-0"
                  >
                    <p className="min-w-0 flex-1 truncate text-sm font-medium text-fg">{app.label}</p>
                    {/* Basis 300px like the original fixed design, but with
                        flex-shrink left at its default (1) instead of the
                        old shrink-0 -- it shrinks with the row instead of
                        forcing horizontal overflow at narrow widths or high
                        zoom (row also wraps as a last resort). */}
                    <div className="max-w-full basis-[300px] shrink">
                      <SegmentedControl
                        options={ROUTE_OPTIONS}
                        value={currentValue(app.id)}
                        disabled={appRoutingBusy}
                        aria-label={t.appRouting.routeAriaLabel(app.label)}
                        onChange={(value) =>
                          setAppRoute(app.id, app.label, app.geosite, value === "off" ? null : value)
                        }
                      />
                    </div>
                  </div>
                ))}
              </CardContent>
            </Card>
          );
        })
      )}
    </div>
  );
}
