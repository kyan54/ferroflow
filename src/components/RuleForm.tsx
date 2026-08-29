import { useState } from "react";
import { useAppStore } from "../store";
import { RULE_MATCH_TYPES, RULE_OUTBOUNDS } from "../types";
import type { RoutingRule, RuleMatchType, RuleOutbound } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Select, Textarea, Toggle } from "./ui";

const MATCH_TYPE_LABELS: Record<RuleMatchType, string> = {
  domain: "Domain (exact)",
  domainSuffix: "Domain suffix",
  domainKeyword: "Domain keyword",
  ipCidr: "IP CIDR",
  processName: "Process name",
  ruleSet: "Rule set",
};

const MATCH_TYPE_PLACEHOLDERS: Record<RuleMatchType, string> = {
  domain: "example.com",
  domainSuffix: ".cn",
  domainKeyword: "ads",
  ipCidr: "10.0.0.0/8",
  processName: "chrome.exe",
  ruleSet: "",
};

const OUTBOUND_LABELS: Record<RuleOutbound, string> = {
  proxy: "Proxy",
  direct: "Direct",
  block: "Block",
};

function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `rule-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function RuleForm({ onDone }: { onDone: () => void }) {
  const addRule = useAppStore((s) => s.addRule);
  const ruleResources = useAppStore((s) => s.config?.ruleResources ?? []);

  const [name, setName] = useState("");
  const [matchType, setMatchType] = useState<RuleMatchType>("domainSuffix");
  const [valuesText, setValuesText] = useState("");
  const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>([]);
  const [outbound, setOutbound] = useState<RuleOutbound>("direct");
  const [enabled, setEnabled] = useState(true);
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
      id: newId(),
      name: name.trim(),
      enabled,
      matchType,
      values,
      outbound,
    };

    setSubmitting(true);
    try {
      await addRule(rule);
      onDone();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Card>
      <form onSubmit={handleSubmit}>
        <CardHeader>
          <CardTitle>Add rule</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 pt-4">
          <div className="grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              Name
              <Input
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="China direct"
              />
            </label>

            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              Match type
              <Select value={matchType} onChange={(e) => setMatchType(e.target.value as RuleMatchType)}>
                {RULE_MATCH_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {MATCH_TYPE_LABELS[t]}
                  </option>
                ))}
              </Select>
            </label>

            {matchType === "ruleSet" ? (
              <div className="col-span-2 flex flex-col gap-1 text-sm font-medium text-fg-dim">
                Rule-set resources
                {ruleResources.length === 0 ? (
                  <p className="rounded-md border border-line bg-surface-2 px-3 py-2 text-xs font-normal text-fg-faint">
                    No rule resources downloaded yet -- add one from the "Rule resources" tab first.
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
                Values (comma-separated)
                <Textarea
                  required
                  rows={2}
                  value={valuesText}
                  onChange={(e) => setValuesText(e.target.value)}
                  placeholder={MATCH_TYPE_PLACEHOLDERS[matchType]}
                />
              </label>
            )}

            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              Outbound
              <Select value={outbound} onChange={(e) => setOutbound(e.target.value as RuleOutbound)}>
                {RULE_OUTBOUNDS.map((o) => (
                  <option key={o} value={o}>
                    {OUTBOUND_LABELS[o]}
                  </option>
                ))}
              </Select>
            </label>

            <div className="flex items-end pb-1.5">
              <Toggle checked={enabled} onChange={setEnabled} label="Enabled" />
            </div>
          </div>

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onDone}>
              Cancel
            </Button>
            <Button type="submit" busy={submitting}>
              Add rule
            </Button>
          </div>
        </CardContent>
      </form>
    </Card>
  );
}

export { MATCH_TYPE_LABELS, OUTBOUND_LABELS };
