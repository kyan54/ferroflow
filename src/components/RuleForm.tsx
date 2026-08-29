import { useState } from "react";
import { useAppStore } from "../store";
import { RULE_MATCH_TYPES, RULE_OUTBOUNDS } from "../types";
import type { RoutingRule, RuleMatchType, RuleOutbound } from "../types";

const MATCH_TYPE_LABELS: Record<RuleMatchType, string> = {
  domain: "Domain (exact)",
  domainSuffix: "Domain suffix",
  domainKeyword: "Domain keyword",
  ipCidr: "IP CIDR",
  processName: "Process name",
};

const MATCH_TYPE_PLACEHOLDERS: Record<RuleMatchType, string> = {
  domain: "example.com",
  domainSuffix: ".cn",
  domainKeyword: "ads",
  ipCidr: "10.0.0.0/8",
  processName: "chrome.exe",
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

  const [name, setName] = useState("");
  const [matchType, setMatchType] = useState<RuleMatchType>("domainSuffix");
  const [valuesText, setValuesText] = useState("");
  const [outbound, setOutbound] = useState<RuleOutbound>("direct");
  const [enabled, setEnabled] = useState(true);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const values = valuesText
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
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-4 rounded-xl bg-white p-5 shadow dark:bg-slate-800"
    >
      <h3 className="text-base font-semibold">Add rule</h3>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1 text-sm">
          Name
          <input
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="China direct"
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          />
        </label>

        <label className="flex flex-col gap-1 text-sm">
          Match type
          <select
            value={matchType}
            onChange={(e) => setMatchType(e.target.value as RuleMatchType)}
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          >
            {RULE_MATCH_TYPES.map((t) => (
              <option key={t} value={t}>
                {MATCH_TYPE_LABELS[t]}
              </option>
            ))}
          </select>
        </label>

        <label className="col-span-2 flex flex-col gap-1 text-sm">
          Values (comma-separated)
          <textarea
            required
            rows={2}
            value={valuesText}
            onChange={(e) => setValuesText(e.target.value)}
            placeholder={MATCH_TYPE_PLACEHOLDERS[matchType]}
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          />
        </label>

        <label className="flex flex-col gap-1 text-sm">
          Outbound
          <select
            value={outbound}
            onChange={(e) => setOutbound(e.target.value as RuleOutbound)}
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          >
            {RULE_OUTBOUNDS.map((o) => (
              <option key={o} value={o}>
                {OUTBOUND_LABELS[o]}
              </option>
            ))}
          </select>
        </label>

        <label className="mt-6 flex items-center gap-2 text-sm">
          <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          Enabled
        </label>
      </div>

      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onDone}
          className="rounded-md px-4 py-2 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={submitting}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          Add rule
        </button>
      </div>
    </form>
  );
}

export { MATCH_TYPE_LABELS, OUTBOUND_LABELS };
