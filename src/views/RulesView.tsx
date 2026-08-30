import { useEffect, useMemo, useState } from "react";
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
  Input,
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

/** Below this many rules, the search box in the rule-list toolbar stays
 * hidden (mirrors the real FlowZ app's `SEARCH_THRESHOLD` in
 * `rules-page.tsx` -- a short list doesn't need searching, and showing an
 * empty search box permanently would just be clutter). */
const SEARCH_THRESHOLD = 10;

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

function ArrowUpToLineIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 22V8M6 13l6-6 6 6" />
      <path d="M4 3h16" />
    </svg>
  );
}

function ArrowDownToLineIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 2v14M6 11l6 6 6-6" />
      <path d="M4 21h16" />
    </svg>
  );
}

function ListOrderedIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M10 6h11M10 12h11M10 18h11" />
      <path d="M4 6h1v4M4 10h2" />
      <path d="M4 19v-1.5a1.5 1.5 0 0 1 3 0V18a1.5 1.5 0 0 1-1.5 1.5H4M6.5 19.5H4" />
    </svg>
  );
}

function SearchIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <circle cx="11" cy="11" r="7" />
      <path d="m21 21-4.3-4.3" />
    </svg>
  );
}

function AlertTriangleIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M12 3l9 16H3z" strokeLinejoin="round" />
      <path d="M12 10v4M12 16.5h.01" />
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
  const commitRuleOrder = useAppStore((s) => s.commitRuleOrder);
  const regionPresetBusy = useAppStore((s) => s.regionPresetBusy);
  const applyRegionPreset = useAppStore((s) => s.applyRegionPreset);
  const clearRegionPreset = useAppStore((s) => s.clearRegionPreset);
  const pushToast = useAppStore((s) => s.pushToast);

  const [activeForm, setActiveForm] = useState<"new" | RoutingRule | null>(null);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  // Order-edit draft: null in normal mode; a locally-reordered copy of every
  // rule id while editing (mirrors the real FlowZ app's `orderDraft` -- see
  // `rules-page.tsx` -- zero IPC/store writes until "Save order", so
  // rearranging is free to try and cheap to cancel).
  const [orderDraft, setOrderDraft] = useState<string[] | null>(null);
  const [savingOrder, setSavingOrder] = useState(false);

  const rules = config?.rules ?? [];
  const isOrderEditing = orderDraft !== null;
  const isSmartMode = config?.proxyMode === "smart";

  // External-change guard: if rules were added/removed elsewhere (another
  // view, another sync) while a reorder draft is open, the draft's id set no
  // longer matches reality -- bail out rather than risk saving a stale,
  // mismatched order.
  useEffect(() => {
    if (
      orderDraft &&
      (orderDraft.length !== rules.length || !orderDraft.every((id) => rules.some((r) => r.id === id)))
    ) {
      setOrderDraft(null);
      pushToast("info", t.toasts.ruleOrderConflict);
    }
  }, [rules]);

  const ruleResourceIds = useMemo(
    () => new Set((config?.ruleResources ?? []).map((r) => r.id)),
    [config?.ruleResources],
  );

  /** A `ruleSet`-type rule whose `values` reference a resource id no longer
   * present in `config.ruleResources` (deleted from the "Rule resources"
   * tab) silently does nothing at runtime -- `core_manager::config::
   * build_route_rules` just drops the missing reference. Surfaced here so
   * the gap isn't invisible. */
  function ruleHasMissingResource(rule: RoutingRule): boolean {
    if (rule.matchType !== "ruleSet") return false;
    return rule.values.some((v) => !ruleResourceIds.has(v));
  }

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

  const searchActive = !isOrderEditing && search.trim() !== "";

  function matchesSearch(rule: RoutingRule): boolean {
    const q = search.trim().toLowerCase();
    if (!q) return true;
    if (rule.name.toLowerCase().includes(q)) return true;
    if (rule.values.some((v) => v.toLowerCase().includes(q))) return true;
    if (t.ruleForm.matchTypeLabels[rule.matchType].toLowerCase().includes(q)) return true;
    if (t.ruleForm.outboundLabels[rule.outbound].toLowerCase().includes(q)) return true;
    return false;
  }

  const byId = new Map(rules.map((r) => [r.id, r]));
  const visibleRules: RoutingRule[] = isOrderEditing
    ? orderDraft.map((id) => byId.get(id)).filter((r): r is RoutingRule => !!r)
    : searchActive
      ? rules.filter(matchesSearch)
      : rules;

  const showSearchBox = (rules.length > SEARCH_THRESHOLD || search.trim() !== "") && !isOrderEditing;

  function enterOrderEdit() {
    setSearch("");
    setOrderDraft(rules.map((r) => r.id));
  }

  function cancelOrderEdit() {
    setOrderDraft(null);
  }

  function moveDraft(index: number, dir: -1 | 1) {
    setOrderDraft((prev) => {
      if (!prev) return prev;
      const j = index + dir;
      if (j < 0 || j >= prev.length) return prev;
      const next = prev.slice();
      [next[index], next[j]] = [next[j], next[index]];
      return next;
    });
  }

  function moveDraftToEdge(index: number, edge: "top" | "bottom") {
    setOrderDraft((prev) => {
      if (!prev || index < 0 || index >= prev.length) return prev;
      const next = prev.slice();
      const [id] = next.splice(index, 1);
      if (edge === "top") next.unshift(id);
      else next.push(id);
      return next;
    });
  }

  async function saveOrderEdit() {
    if (!orderDraft) return;
    setSavingOrder(true);
    try {
      const ok = await commitRuleOrder(orderDraft);
      if (ok) setOrderDraft(null);
    } finally {
      setSavingOrder(false);
    }
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <h1 className="font-display text-xl font-semibold text-fg">{t.rules.title}</h1>
        <div className="flex flex-col items-end gap-2">
          <p className="max-w-sm text-right text-xs text-fg-faint">{t.rules.subtitle}</p>
          {activeForm === null && (
            <Button size="sm" onClick={() => setActiveForm("new")} disabled={isOrderEditing}>
              <PlusIcon className="h-3.5 w-3.5" />
              {t.rules.addRule}
            </Button>
          )}
        </div>
      </div>

      <p className="text-sm text-fg-faint">{t.rules.smartRoutingNote}</p>

      {!isSmartMode && (
        <div className="flex items-start gap-2 rounded-lg border border-warn/30 bg-warn-weak px-3 py-2.5 text-sm text-warn">
          <AlertTriangleIcon className="mt-0.5 h-4 w-4 shrink-0" />
          <p>{t.rules.smartOnlyNotice}</p>
        </div>
      )}

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
          {!isSmartMode && (
            <p className="flex items-center gap-1.5 text-xs text-fg-faint">
              <span className="h-1.5 w-1.5 rounded-full bg-fg-faint" />
              {t.rules.regionCard.notActiveHint}
            </p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex-wrap">
          <CardTitle>{t.rules.ruleListCard.title}</CardTitle>
          <div className="flex items-center gap-2">
            {showSearchBox && (
              <div className="relative w-52">
                <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-faint" />
                <Input
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder={t.rules.searchPlaceholder}
                  aria-label={t.rules.searchAriaLabel}
                  className="pl-8"
                />
              </div>
            )}
            {rules.length >= 2 && !isOrderEditing && (
              <Button
                variant="ghost"
                size="sm"
                onClick={enterOrderEdit}
                disabled={searchActive}
                title={searchActive ? t.rules.editOrderHintSearch : undefined}
              >
                <ListOrderedIcon className="h-3.5 w-3.5" />
                {t.rules.editOrder}
              </Button>
            )}
          </div>
        </CardHeader>
        {rules.length === 0 ? (
          <CardContent className="flex flex-col items-center gap-3 pt-4 pb-6 text-center">
            <p className="text-sm text-fg-faint">{t.rules.empty}</p>
            <Button variant="ghost" size="sm" onClick={() => setActiveForm("new")}>
              <PlusIcon className="h-3.5 w-3.5" />
              {t.rules.addFirstRule}
            </Button>
          </CardContent>
        ) : visibleRules.length === 0 ? (
          <CardContent className="pt-4 text-center text-sm text-fg-faint">{t.rules.searchNoMatch}</CardContent>
        ) : (
          <CardContent className="flex flex-col gap-3 pt-4">
            <CardDescription>{t.rules.description}</CardDescription>

            {isOrderEditing ? (
              <div className="-mx-5 flex flex-col">
                {visibleRules.map((rule, index) => (
                  <div
                    key={rule.id}
                    className="flex items-center gap-3 border-b border-line px-5 py-2.5 last:border-0"
                  >
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-fg">{rule.name}</span>
                    <Badge variant={OUTBOUND_BADGE[rule.outbound]}>{t.ruleForm.outboundLabels[rule.outbound]}</Badge>
                    <div className="flex items-center gap-0.5">
                      <Button
                        variant="ghost"
                        size="icon"
                        disabled={savingOrder || index === 0}
                        onClick={() => moveDraftToEdge(index, "top")}
                        title={t.rules.moveTop}
                        aria-label={t.rules.moveTop}
                      >
                        <ArrowUpToLineIcon className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        disabled={savingOrder || index === 0}
                        onClick={() => moveDraft(index, -1)}
                        title={t.rules.moveUp}
                        aria-label={t.rules.moveUp}
                      >
                        <ArrowUpIcon className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        disabled={savingOrder || index === visibleRules.length - 1}
                        onClick={() => moveDraft(index, 1)}
                        title={t.rules.moveDown}
                        aria-label={t.rules.moveDown}
                      >
                        <ArrowDownIcon className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        disabled={savingOrder || index === visibleRules.length - 1}
                        onClick={() => moveDraftToEdge(index, "bottom")}
                        title={t.rules.moveBottom}
                        aria-label={t.rules.moveBottom}
                      >
                        <ArrowDownToLineIcon className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                ))}
                <div className="mt-2 flex items-center justify-between gap-3 border-t border-line px-5 pt-3">
                  <span className="flex items-center gap-1.5 text-xs text-warn">
                    <span className="h-1.5 w-1.5 rounded-full bg-warn" />
                    {t.rules.orderDraftUnsaved}
                  </span>
                  <div className="flex gap-2">
                    <Button variant="ghost" size="sm" onClick={cancelOrderEdit} disabled={savingOrder}>
                      {t.common.cancel}
                    </Button>
                    <Button size="sm" onClick={saveOrderEdit} busy={savingOrder}>
                      {t.rules.saveOrder}
                    </Button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="-mx-5 overflow-x-auto">
                <table className="w-full min-w-[560px] border-collapse text-sm">
                  <thead>
                    <tr className="border-b border-line text-left text-xs font-medium uppercase tracking-wide text-fg-faint">
                      <th className="w-12 py-2 pl-5 pr-2 font-medium">{t.rules.ruleListCard.columnEnabled}</th>
                      <th className="py-2 px-2 font-medium">{t.rules.ruleListCard.columnRule}</th>
                      <th className="w-28 py-2 px-2 font-medium">{t.rules.ruleListCard.columnStrategy}</th>
                      <th className="w-20 py-2 pl-2 pr-5 text-right font-medium">
                        {t.rules.ruleListCard.columnActions}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleRules.map((rule) => (
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
                          <div className="flex flex-wrap items-center gap-1">
                            <Badge variant={OUTBOUND_BADGE[rule.outbound]}>
                              {t.ruleForm.outboundLabels[rule.outbound]}
                            </Badge>
                            {ruleHasMissingResource(rule) && (
                              <Badge variant="warning" title={t.rules.resourceMissingTip}>
                                {t.rules.resourceMissing}
                              </Badge>
                            )}
                          </div>
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
            )}
          </CardContent>
        )}
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t.rules.chain.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 pt-4">
          <div className="flex flex-wrap items-center gap-1.5">
            {[t.rules.chain.stepRules, t.rules.chain.stepSmart, t.rules.chain.stepDefault].map((step, i, arr) => (
              <span key={step} className="flex items-center gap-1.5">
                <span
                  className={cn(
                    "rounded-md px-2 py-1 text-xs font-semibold",
                    i === 0 ? "bg-flow-weak text-flow-hi" : "bg-surface-2 text-fg-dim",
                  )}
                >
                  {step}
                </span>
                {i < arr.length - 1 && <span className="text-fg-faint">›</span>}
              </span>
            ))}
          </div>
          <p className="text-xs text-fg-faint">{t.rules.chain.instruction1}</p>
          <p className="text-xs text-fg-faint">
            {t.rules.chain.instruction2(t.ruleForm.outboundLabels[config?.defaultOutbound ?? "proxy"])}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
