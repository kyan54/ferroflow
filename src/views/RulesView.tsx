import { useState } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { RuleForm } from "../components/RuleForm";
import { Card, CardContent, Button, Toggle, Badge } from "../components/ui";
import type { BadgeVariant } from "../components/ui";
import type { RuleOutbound } from "../types";

const OUTBOUND_BADGE: Record<RuleOutbound, BadgeVariant> = {
  proxy: "default",
  direct: "secondary",
  block: "destructive",
};

export function RulesView() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const deleteRule = useAppStore((s) => s.deleteRule);
  const updateRule = useAppStore((s) => s.updateRule);
  const moveRuleUp = useAppStore((s) => s.moveRuleUp);
  const moveRuleDown = useAppStore((s) => s.moveRuleDown);
  const [showForm, setShowForm] = useState(false);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);

  const rules = config?.rules ?? [];

  function handleDelete(id: string) {
    if (pendingDeleteId === id) {
      deleteRule(id);
      setPendingDeleteId(null);
    } else {
      setPendingDeleteId(id);
    }
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h1 className="font-display text-xl font-semibold text-fg">{t.rules.title}</h1>
        {!showForm && <Button onClick={() => setShowForm(true)}>{t.rules.addRule}</Button>}
      </div>

      <p className="text-sm text-fg-faint">{t.rules.description}</p>

      {showForm && <RuleForm onDone={() => setShowForm(false)} />}

      {rules.length === 0 ? (
        <Card>
          <CardContent className="text-sm text-fg-faint">{t.rules.empty}</CardContent>
        </Card>
      ) : (
        <ul className="flex flex-col gap-2">
          {rules.map((rule, index) => (
            <li key={rule.id}>
              <Card className={rule.enabled ? undefined : "opacity-60"}>
                <CardContent className="flex items-center justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <p className="font-medium text-fg">{rule.name}</p>
                      <Badge variant={OUTBOUND_BADGE[rule.outbound]}>
                        {t.ruleForm.outboundLabels[rule.outbound]}
                      </Badge>
                    </div>
                    <p className="mt-0.5 truncate text-sm text-fg-faint">
                      {t.ruleForm.matchTypeLabels[rule.matchType]} · {rule.values.join(", ")}
                    </p>
                  </div>

                  <div className="flex shrink-0 items-center gap-1">
                    <Toggle
                      checked={rule.enabled}
                      onChange={(checked) => updateRule({ ...rule, enabled: checked })}
                      aria-label={t.rules.enabledAriaLabel}
                      className="mr-2"
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => moveRuleUp(rule.id)}
                      disabled={index === 0}
                      aria-label={t.rules.moveUp}
                    >
                      ↑
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => moveRuleDown(rule.id)}
                      disabled={index === rules.length - 1}
                      aria-label={t.rules.moveDown}
                    >
                      ↓
                    </Button>
                    <Button
                      variant={pendingDeleteId === rule.id ? "destructive" : "ghost"}
                      size="sm"
                      onClick={() => handleDelete(rule.id)}
                      onBlur={() => setPendingDeleteId(null)}
                    >
                      {pendingDeleteId === rule.id ? t.rules.confirmDelete : t.rules.delete}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
