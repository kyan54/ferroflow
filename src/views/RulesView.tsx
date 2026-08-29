import { useState } from "react";
import { useAppStore } from "../store";
import { RuleForm, MATCH_TYPE_LABELS, OUTBOUND_LABELS } from "../components/RuleForm";

export function RulesView() {
  const config = useAppStore((s) => s.config);
  const deleteRule = useAppStore((s) => s.deleteRule);
  const updateRule = useAppStore((s) => s.updateRule);
  const moveRuleUp = useAppStore((s) => s.moveRuleUp);
  const moveRuleDown = useAppStore((s) => s.moveRuleDown);
  const [showForm, setShowForm] = useState(false);

  const rules = config?.rules ?? [];

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Rules</h2>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
          >
            Add rule
          </button>
        )}
      </div>

      <p className="text-sm text-slate-500 dark:text-slate-400">
        Rules are evaluated top to bottom; the first match wins. Traffic that matches no rule falls
        back to the current proxy mode.
      </p>

      {showForm && <RuleForm onDone={() => setShowForm(false)} />}

      {rules.length === 0 ? (
        <p className="rounded-xl bg-white p-5 text-sm text-slate-500 shadow dark:bg-slate-800 dark:text-slate-400">
          No custom routing rules yet. Everything goes through the current proxy mode.
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {rules.map((rule, index) => (
            <li
              key={rule.id}
              className="flex items-center justify-between gap-3 rounded-xl bg-white p-4 shadow dark:bg-slate-800"
            >
              <div className="min-w-0 flex-1">
                <p className="font-medium">{rule.name}</p>
                <p className="truncate text-sm text-slate-500 dark:text-slate-400">
                  {MATCH_TYPE_LABELS[rule.matchType]} · {rule.values.join(", ")} ·{" "}
                  {OUTBOUND_LABELS[rule.outbound]}
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-1">
                <label className="mr-2 flex items-center gap-1.5 text-sm text-slate-600 dark:text-slate-300">
                  <input
                    type="checkbox"
                    checked={rule.enabled}
                    onChange={(e) => updateRule({ ...rule, enabled: e.target.checked })}
                  />
                  Enabled
                </label>
                <button
                  onClick={() => moveRuleUp(rule.id)}
                  disabled={index === 0}
                  aria-label="Move up"
                  className="rounded-md px-2 py-1.5 text-sm text-slate-600 hover:bg-slate-100 disabled:opacity-30 dark:text-slate-300 dark:hover:bg-slate-700"
                >
                  ↑
                </button>
                <button
                  onClick={() => moveRuleDown(rule.id)}
                  disabled={index === rules.length - 1}
                  aria-label="Move down"
                  className="rounded-md px-2 py-1.5 text-sm text-slate-600 hover:bg-slate-100 disabled:opacity-30 dark:text-slate-300 dark:hover:bg-slate-700"
                >
                  ↓
                </button>
                <button
                  onClick={() => deleteRule(rule.id)}
                  className="rounded-md px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
                >
                  Delete
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
