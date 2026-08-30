import { useMemo, useState } from "react";
import type { SVGProps } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { RuleForm } from "../components/RuleForm";
import { REGION_PRESETS, PRESET_RULE_PREFIX } from "../lib/appRouting";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  Button,
  Toggle,
  Badge,
  SegmentedControl,
} from "../components/ui";
import type { BadgeVariant, SegmentedOption } from "../components/ui";
import { cn } from "../lib/utils";
import type { RoutingRule, RuleOutbound } from "../types";

const OUTBOUND_BADGE: Record<RuleOutbound, BadgeVariant> = {
  proxy: "default",
  direct: "secondary",
  block: "destructive",
};

/** Region presets a user can pick with a single, non-destructive click on
 * this page's pill row -- excludes "Global proxy, no rules", which (per its
 * own `clearsAllRules` flag) wipes manual rules and app-routing toggles too.
 * That one stays a deliberate, confirm-guarded action on the App routing
 * page rather than a single-click pill here. */
const REGION_PILL_PRESETS = REGION_PRESETS.filter((preset) => !preset.clearsAllRules);

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

function PlusIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

function PencilIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
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

function ArrowUpIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 19V5M5 12l7-7 7 7" />
    </svg>
  );
}

function ArrowDownIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 5v14M5 12l7 7 7-7" />
    </svg>
  );
}

/** Parses the `presetId` out of one `PRESET_RULE_PREFIX`-tagged rule id
 * (`"preset:<presetId>:<index>"` -- see `buildPresetRules`). */
function presetIdFromRuleId(ruleId: string): string {
  const rest = ruleId.slice(PRESET_RULE_PREFIX.length);
  const lastColon = rest.lastIndexOf(":");
  return lastColon === -1 ? rest : rest.slice(0, lastColon);
}

export function RulesView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const deleteRule = useAppStore((s) => s.deleteRule);
  const updateRule = useAppStore((s) => s.updateRule);
  const moveRuleUp = useAppStore((s) => s.moveRuleUp);
  const moveRuleDown = useAppStore((s) => s.moveRuleDown);
  const regionPresetBusy = useAppStore((s) => s.regionPresetBusy);
  const applyRegionPreset = useAppStore((s) => s.applyRegionPreset);
  const clearRegionPreset = useAppStore((s) => s.clearRegionPreset);

  const [activeForm, setActiveForm] = useState<"new" | RoutingRule | null>(null);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  const rules = config?.rules ?? [];

  const activePresetId = useMemo(() => {
    const presetRule = rules.find((r) => r.id.startsWith(PRESET_RULE_PREFIX));
    return presetRule ? presetIdFromRuleId(presetRule.id) : null;
  }, [rules]);

  const regionOn = activePresetId != null && REGION_PILL_PRESETS.some((p) => p.id === activePresetId);

  const regionOptions: SegmentedOption<string>[] = REGION_PILL_PRESETS.map((preset) => ({
    value: preset.id,
    label: t.appRouting.presets[preset.id as keyof typeof t.appRouting.presets].label,
  }));

  function handleRegionToggle(checked: boolean) {
    if (checked) {
      applyRegionPreset(REGION_PILL_PRESETS[0].id);
    } else {
      clearRegionPreset();
    }
  }

  function handleDelete(id: string) {
    if (pendingDeleteId === id) {
      deleteRule(id);
      setPendingDeleteId(null);
    } else {
      setPendingDeleteId(id);
    }
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <h1 className="font-display text-xl font-semibold text-fg">{t.rules.title}</h1>
        <div className="flex flex-col items-end gap-2">
          <p className="max-w-sm text-right text-xs text-fg-faint">{t.rules.subtitle}</p>
          {activeForm === null && (
            <Button size="sm" onClick={() => setActiveForm("new")}>
              <PlusIcon className="h-3.5 w-3.5" />
              {t.rules.addRule}
            </Button>
          )}
        </div>
      </div>

      <p className="text-sm text-fg-faint">{t.rules.smartRoutingNote}</p>

      {activeForm !== null && (
        <RuleForm
          initialRule={activeForm === "new" ? undefined : activeForm}
          onDone={() => setActiveForm(null)}
        />
      )}

      <Card>
        <CardHeader>
          <CardTitle>{t.rules.regionCard.title}</CardTitle>
          <Toggle
            checked={regionOn}
            onChange={handleRegionToggle}
            disabled={regionPresetBusy}
            aria-label={t.rules.regionCard.toggleAriaLabel}
          />
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <div>
            <p className="text-sm font-medium text-fg">{t.rules.regionCard.regionLabel}</p>
            <p className="mt-0.5 text-xs text-fg-faint">{t.rules.regionCard.regionExplainer}</p>
          </div>
          <SegmentedControl
            options={regionOptions}
            value={activePresetId ?? ""}
            onChange={(id) => applyRegionPreset(id)}
            disabled={regionPresetBusy}
            aria-label={t.rules.regionCard.presetAriaLabel}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.rules.ruleListCard.title}</CardTitle>
        </CardHeader>
        {rules.length === 0 ? (
          <CardContent className="pt-4 text-sm text-fg-faint">{t.rules.empty}</CardContent>
        ) : (
          <CardContent className="flex flex-col gap-3 pt-4">
            <CardDescription>{t.rules.description}</CardDescription>
            <div className="-mx-5 overflow-x-auto">
              <table className="w-full min-w-[560px] border-collapse text-sm">
                <thead>
                  <tr className="border-b border-line text-left text-xs font-medium uppercase tracking-wide text-fg-faint">
                    <th className="w-12 py-2 pl-5 pr-2 font-medium">{t.rules.ruleListCard.columnEnabled}</th>
                    <th className="py-2 px-2 font-medium">{t.rules.ruleListCard.columnRule}</th>
                    <th className="w-28 py-2 px-2 font-medium">{t.rules.ruleListCard.columnStrategy}</th>
                    <th className="w-40 py-2 pl-2 pr-5 text-right font-medium">
                      {t.rules.ruleListCard.columnActions}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {rules.map((rule, index) => (
                    <tr
                      key={rule.id}
                      className={cn("border-b border-line last:border-0", !rule.enabled && "opacity-60")}
                    >
                      <td className="py-3 pl-5 pr-2">
                        <Toggle
                          checked={rule.enabled}
                          onChange={(checked) => updateRule({ ...rule, enabled: checked })}
                          aria-label={t.rules.enabledAriaLabel}
                        />
                      </td>
                      <td className="min-w-0 py-3 px-2">
                        <p className="truncate font-medium text-fg">{rule.name}</p>
                        <p className="mt-0.5 truncate text-xs text-fg-faint">
                          {t.ruleForm.matchTypeLabels[rule.matchType]} · {rule.values.join(", ")}
                        </p>
                      </td>
                      <td className="py-3 px-2">
                        <Badge variant={OUTBOUND_BADGE[rule.outbound]}>
                          {t.ruleForm.outboundLabels[rule.outbound]}
                        </Badge>
                      </td>
                      <td className="py-3 pl-2 pr-5">
                        <div className="flex items-center justify-end gap-0.5">
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => setActiveForm(rule)}
                            title={t.rules.edit}
                            aria-label={t.rules.edit}
                          >
                            <PencilIcon className="h-4 w-4" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => moveRuleUp(rule.id)}
                            disabled={index === 0}
                            title={t.rules.moveUp}
                            aria-label={t.rules.moveUp}
                          >
                            <ArrowUpIcon className="h-4 w-4" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => moveRuleDown(rule.id)}
                            disabled={index === rules.length - 1}
                            title={t.rules.moveDown}
                            aria-label={t.rules.moveDown}
                          >
                            <ArrowDownIcon className="h-4 w-4" />
                          </Button>
                          <Button
                            variant={pendingDeleteId === rule.id ? "destructive" : "ghost"}
                            size="icon"
                            onClick={() => handleDelete(rule.id)}
                            onBlur={() => setPendingDeleteId(null)}
                            title={pendingDeleteId === rule.id ? t.rules.confirmDelete : t.rules.delete}
                            aria-label={pendingDeleteId === rule.id ? t.rules.confirmDelete : t.rules.delete}
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
          </CardContent>
        )}
      </Card>
    </div>
  );
}
