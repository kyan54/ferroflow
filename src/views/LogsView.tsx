import { useEffect, useRef } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { getT } from "../i18n/current";
import type { LogEntry, LogLevel } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Badge } from "../components/ui";
import type { BadgeVariant } from "../components/ui";

const LEVEL_BADGE_VARIANT: Record<LogLevel, BadgeVariant> = {
  trace: "outline",
  debug: "outline",
  info: "secondary",
  warn: "warning",
  error: "destructive",
};

function formatTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return timestamp;
  return date.toLocaleTimeString();
}

function formatLine(entry: LogEntry): string {
  const target = entry.target ? ` ${entry.target}` : "";
  return `[${entry.timestamp}] ${entry.level.toUpperCase()} [${entry.source}]${target}: ${entry.message}`;
}

export function LogsView() {
  const { t } = useTranslation();
  const logEntries = useAppStore((s) => s.logEntries);
  const refreshLogs = useAppStore((s) => s.refreshLogs);
  const clearLogs = useAppStore((s) => s.clearLogs);

  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    refreshLogs();
    const interval = setInterval(refreshLogs, 2000);
    return () => clearInterval(interval);
  }, [refreshLogs]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [logEntries]);

  const copyAll = async () => {
    const text = logEntries.map(formatLine).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      useAppStore.getState().pushToast("success", getT().toasts.logsCopied);
    } catch (err) {
      useAppStore.getState().pushToast("error", getT().toasts.logsCopyFailed(String(err)));
    }
  };

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 p-6">
      <h1 className="font-display text-xl font-semibold text-fg">{t.logs.title}</h1>

      <Card>
        <CardHeader>
          <CardTitle>{t.logs.cardTitle}</CardTitle>
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="sm" onClick={refreshLogs}>
              {t.logs.refresh}
            </Button>
            <Button variant="outline" size="sm" disabled={logEntries.length === 0} onClick={copyAll}>
              {t.logs.copyAll}
            </Button>
            <Button variant="destructive" size="sm" disabled={logEntries.length === 0} onClick={clearLogs}>
              {t.logs.clear}
            </Button>
          </div>
        </CardHeader>

        <CardContent className="pt-4">
          <p className="-mt-2 mb-3 text-sm text-fg-faint">{t.logs.explainer(logEntries.length)}</p>

          <div
            ref={scrollRef}
            className="h-[480px] overflow-y-auto rounded-lg border border-line bg-surface-2 p-3 font-mono text-xs leading-relaxed"
          >
            {logEntries.map((entry, i) => (
              <div key={i} className="flex items-start gap-2 py-0.5">
                <span className="shrink-0 text-fg-faint">{formatTime(entry.timestamp)}</span>
                <Badge variant={LEVEL_BADGE_VARIANT[entry.level]} className="shrink-0">
                  {entry.level}
                </Badge>
                <Badge variant="outline" className="shrink-0 normal-case">
                  {entry.source}
                </Badge>
                {entry.target && <span className="shrink-0 text-fg-faint">{entry.target}</span>}
                <span className="min-w-0 flex-1 whitespace-pre-wrap break-all text-fg">{entry.message}</span>
              </div>
            ))}

            {logEntries.length === 0 && <p className="text-sm text-fg-faint">{t.logs.empty}</p>}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
