import { useState } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { APP_ROUTING_CATALOG, REGION_PRESETS, appRoutingRuleId } from "../lib/appRouting";
import type { RuleOutbound } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, SegmentedControl } from "../components/ui";
import type { SegmentedOption } from "../components/ui";

type AppRouteValue = "off" | RuleOutbound;

export function AppRoutingView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const appRoutingBusy = useAppStore((s) => s.appRoutingBusy);
  const regionPresetBusy = useAppStore((s) => s.regionPresetBusy);
  const setAppRoute = useAppStore((s) => s.setAppRoute);
  const applyRegionPreset = useAppStore((s) => s.applyRegionPreset);
  const [pendingPresetId, setPendingPresetId] = useState<string | null>(null);

  const ROUTE_OPTIONS: SegmentedOption<AppRouteValue>[] = [
    { value: "off", label: t.appRouting.routeOptions.off },
    { value: "proxy", label: t.appRouting.routeOptions.proxy },
    { value: "direct", label: t.appRouting.routeOptions.direct },
    { value: "block", label: t.appRouting.routeOptions.block },
  ];

  if (!config) {
    return <div className="mx-auto max-w-3xl p-6 text-sm text-fg-faint">{t.common.loading}</div>;
  }

  const rules = config.rules;

  function currentValue(appId: string): AppRouteValue {
    const rule = rules.find((r) => r.id === appRoutingRuleId(appId));
    return rule ? rule.outbound : "off";
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
      <h1 className="font-display text-xl font-semibold text-fg">{t.appRouting.title}</h1>
      <p className="text-sm text-fg-faint">{t.appRouting.description}</p>

      <Card>
        <CardHeader>
          <CardTitle>{t.appRouting.presetsTitle}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <p className="text-sm text-fg-faint">{t.appRouting.presetsExplainer}</p>
          {REGION_PRESETS.map((preset) => {
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

      {APP_ROUTING_CATALOG.map((category) => (
        <Card key={category.id}>
          <CardHeader>
            <CardTitle>{t.appRouting.categories[category.id as keyof typeof t.appRouting.categories]}</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 pt-4">
            {category.apps.map((app) => (
              <div key={app.id} className="flex items-center justify-between gap-3">
                <p className="text-sm font-medium text-fg">{app.label}</p>
                <div className="w-[300px] shrink-0">
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
      ))}
    </div>
  );
}
