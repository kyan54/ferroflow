import { useState } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { RULE_OUTBOUNDS } from "../types";
import type { RoutingRule, RuleMatchType, RuleOutbound } from "../types";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Input,
  Select,
  Textarea,
  Toggle,
  SegmentedControl,
} from "./ui";
import type { SegmentedOption } from "./ui";

const MATCH_TYPE_PLACEHOLDERS: Record<RuleMatchType, string> = {
  domain: "example.com",
  domainSuffix: ".cn",
  domainKeyword: "ads",
  ipCidr: "10.0.0.0/8",
  processName: "chrome.exe",
  ruleSet: "",
};

/** Groups `RULE_MATCH_TYPES` the same way the real FlowZ app's rule dialog
 * categorizes match types (domain / network / process / rule-set) -- see
 * `rule-type-meta.ts`'s `CATEGORY_TYPES` in the reference Electron source.
 * Trimmed to only the match types ferroflow's `RuleMatchType` actually has
 * (no domainRegex/sourceIpCidr/port/device categories -- those aren't part
 * of this app's routing-rule model). */
const MATCH_TYPE_CATEGORIES: { category: "domain" | "network" | "process" | "ruleset"; types: RuleMatchType[] }[] = [
  { category: "domain", types: ["domain", "domainSuffix", "domainKeyword"] },
  { category: "network", types: ["ipCidr"] },
  { category: "process", types: ["processName"] },
  { category: "ruleset", types: ["ruleSet"] },
];

function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `rule-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function RuleForm({
  onDone,
  initialRule,
}: {
  onDone: () => void;
  /** When set, the form edits this existing rule (via `updateRule`) instead
   * of creating a new one. */
  initialRule?: RoutingRule;
}) {
  const { t } = useTranslation();
  const addRule = useAppStore((s) => s.addRule);
  const updateRule = useAppStore((s) => s.updateRule);
  const ruleResources = useAppStore((s) => s.config?.ruleResources ?? []);
  const isEditing = !!initialRule;

  const outboundOptions: SegmentedOption<RuleOutbound>[] = RULE_OUTBOUNDS.map((o) => ({
    value: o,
    label: t.ruleForm.outboundLabels[o],
  }));

  const [name, setName] = useState(initialRule?.name ?? "");
  const [matchType, setMatchType] = useState<RuleMatchType>(initialRule?.matchType ?? "domainSuffix");
  const [valuesText, setValuesText] = useState(
    initialRule && initialRule.matchType !== "ruleSet" ? initialRule.values.join(", ") : "",
  );
  const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>(
    initialRule && initialRule.matchType === "ruleSet" ? initialRule.values : [],
  );
  const [outbound, setOutbound] = useState<RuleOutbound>(initialRule?.outbound ?? "direct");
  const [enabled, setEnabled] = useState(initialRule?.enabled ?? true);
  const [submitting, setSubmitting] = useState(false);

  function toggleResource(id: string) {
    setSelectedResourceIds((prev) =>
      prev.includes(id) ? prev.filter((v) => v !== id) : [...prev, id],
    );
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const values =
      matchType === "ruleSet"
        ? selectedResourceIds
        : valuesText
            .split(",")
            .map((v) => v.trim())
            .filter((v) => v.length > 0);
    if (!name.trim() || values.length === 0) return;

    const rule: RoutingRule = {
      id: initialRule?.id ?? newId(),
      name: name.trim(),
      enabled,
      matchType,
      values,
      outbound,
    };

    setSubmitting(true);
    try {
      if (isEditing) {
        await updateRule(rule);
      } else {
        await addRule(rule);
      }
      onDone();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Card>
      <form onSubmit={handleSubmit}>
        <CardHeader>
          <CardTitle>{isEditing ? t.ruleForm.editTitle : t.ruleForm.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 pt-4">
          <div className="grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.ruleForm.name}
              <Input
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="China direct"
              />
            </label>

            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.ruleForm.matchType}
              <Select value={matchType} onChange={(e) => setMatchType(e.target.value as RuleMatchType)}>
                {MATCH_TYPE_CATEGORIES.map(({ category, types }) => (
                  <optgroup key={category} label={t.ruleForm.matchTypeCategories[category]}>
                    {types.map((mt) => (
                      <option key={mt} value={mt}>
                        {t.ruleForm.matchTypeLabels[mt]}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </Select>
            </label>

            {matchType === "ruleSet" ? (
              <div className="col-span-2 flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.ruleForm.ruleSetResources}
                {ruleResources.length === 0 ? (
                  <p className="rounded-md border border-line bg-surface-2 px-3 py-2 text-xs font-normal text-fg-faint">
                    {t.ruleForm.noRuleResources}
                  </p>
                ) : (
                  <div className="flex max-h-40 flex-col gap-1.5 overflow-y-auto rounded-md border border-line bg-surface-2 px-3 py-2">
                    {ruleResources.map((resource) => (
                      <label key={resource.id} className="flex items-center gap-2 text-sm font-normal text-fg">
                        <input
                          type="checkbox"
                          checked={selectedResourceIds.includes(resource.id)}
                          onChange={() => toggleResource(resource.id)}
                          className="h-3.5 w-3.5 rounded border-line accent-flow"
                        />
                        {resource.name} ({resource.category})
                      </label>
                    ))}
                  </div>
                )}
              </div>
            ) : (
              <label className="col-span-2 flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.ruleForm.values}
                <Textarea
                  required
                  rows={2}
                  value={valuesText}
                  onChange={(e) => setValuesText(e.target.value)}
                  placeholder={MATCH_TYPE_PLACEHOLDERS[matchType]}
                />
                <span className="text-xs font-normal text-fg-faint">{t.ruleForm.matchTypeHints[matchType]}</span>
              </label>
            )}

            <label className="col-span-2 flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.ruleForm.outbound}
              <SegmentedControl options={outboundOptions} value={outbound} onChange={setOutbound} />
            </label>

            <div className="col-span-2 rounded-md border border-line bg-surface-2 px-3 py-2">
              <Toggle checked={enabled} onChange={setEnabled} label={t.ruleForm.enabled} />
            </div>
          </div>

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onDone}>
              {t.ruleForm.cancel}
            </Button>
            <Button type="submit" busy={submitting}>
              {isEditing ? t.ruleForm.save : t.ruleForm.submit}
            </Button>
          </div>
        </CardContent>
      </form>
    </Card>
  );
}
