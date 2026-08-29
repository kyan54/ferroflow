import { useState } from "react";
import { useAppStore } from "../store";
import { APP_ROUTING_CATALOG, REGION_PRESETS, appRoutingRuleId } from "../lib/appRouting";
import type { RuleOutbound } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, SegmentedControl } from "../components/ui";
import type { SegmentedOption } from "../components/ui";

type AppRouteValue = "off" | RuleOutbound;

const ROUTE_OPTIONS: SegmentedOption<AppRouteValue>[] = [
  { value: "off", label: "Off" },
  { value: "proxy", label: "Proxy" },
  { value: "direct", label: "Direct" },
  { value: "block", label: "Block" },
];

export function AppRoutingView() {
  const config = useAppStore((s) => s.config);
  const appRoutingBusy = useAppStore((s) => s.appRoutingBusy);
  const regionPresetBusy = useAppStore((s) => s.regionPresetBusy);
  const setAppRoute = useAppStore((s) => s.setAppRoute);
  const applyRegionPreset = useAppStore((s) => s.applyRegionPreset);
  const [pendingPresetId, setPendingPresetId] = useState<string | null>(null);

  if (!config) {
    return <div className="mx-auto max-w-3xl p-6 text-sm text-fg-faint">Loading…</div>;
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
      <h1 className="font-display text-xl font-semibold text-fg">App routing</h1>
      <p className="text-sm text-fg-faint">
        One-click routing for well-known apps and services, and region-based presets for bulk rule
        changes -- both are just a friendlier way to manage the same routing rules as the "Rules"
        page, matched against downloaded GeoSite/GeoIP rule-sets instead of hand-typed domains.
      </p>

      <Card>
        <CardHeader>
          <CardTitle>Region presets</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <p className="text-sm text-fg-faint">
            Applying a preset replaces its own previously-applied rules and sets the fallback
            ("everything else") outbound -- your manual rules and app-routing toggles below are left
            alone, except for "Global proxy, no rules", which clears every rule. Click a preset once
            to arm it, click again to confirm.
          </p>
          {REGION_PRESETS.map((preset) => {
            const armed = pendingPresetId === preset.id;
            return (
              <div
                key={preset.id}
                className="flex items-center justify-between gap-3 rounded-md border border-line bg-surface-2 px-3 py-2.5"
              >
                <div className="min-w-0">
                  <p className="text-sm font-medium text-fg">{preset.label}</p>
                  <p className="mt-0.5 text-xs text-fg-faint">{preset.description}</p>
                </div>
                <Button
                  variant={armed ? "destructive" : "outline"}
                  size="sm"
                  busy={regionPresetBusy}
                  onClick={() => handlePresetClick(preset.id)}
                  onBlur={() => setPendingPresetId(null)}
                  className="shrink-0"
                >
                  {armed ? "Confirm?" : "Apply"}
                </Button>
              </div>
            );
          })}
        </CardContent>
      </Card>

      {APP_ROUTING_CATALOG.map((category) => (
        <Card key={category.id}>
          <CardHeader>
            <CardTitle>{category.label}</CardTitle>
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
                    aria-label={`Route ${app.label}`}
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
