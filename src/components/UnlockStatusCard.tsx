import { Card, CardHeader, CardTitle, CardContent, Button, Badge } from "./ui";
import type { BadgeVariant } from "./ui";
import type { UnlockResult, UnlockStatus } from "../types";

const STATUS_BADGE: Record<UnlockStatus, BadgeVariant> = {
  unlocked: "success",
  locked: "destructive",
  unknown: "warning",
  error: "secondary",
};

const STATUS_LABEL: Record<UnlockStatus, string> = {
  unlocked: "Unlocked",
  locked: "Not unlocked",
  unknown: "Unknown",
  error: "Error",
};

interface UnlockStatusCardProps {
  results: UnlockResult[] | null;
  busy: boolean;
  error: string | null;
  /** Whether the proxy is currently running -- the button is disabled (and
   * an explanatory message shown instead of the results grid) when it
   * isn't, since there's no local proxy port to route the probes through. */
  running: boolean;
  onCheck: () => void;
}

/** Dashboard card for the "is this streaming/AI service reachable through my
 * current server" check (`unlock_check` -- see `core_manager::unlock`).
 * Manually triggered, not polled: each check makes real requests to several
 * external services and can take a few seconds. */
export function UnlockStatusCard({ results, busy, error, running, onCheck }: UnlockStatusCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Unlock status</CardTitle>
        <Button variant="secondary" size="sm" busy={busy} onClick={onCheck} disabled={!running}>
          Check unlock status
        </Button>
      </CardHeader>
      <CardContent className="pt-4">
        {!running ? (
          <p className="text-sm text-fg-faint">
            Start the proxy to check which streaming services it unlocks.
          </p>
        ) : error ? (
          <p className="rounded-md border border-err/30 bg-err-weak px-3 py-2 text-sm text-err">{error}</p>
        ) : !results ? (
          <p className="text-sm text-fg-faint">
            Checks whether well-known streaming and AI services are reachable through the current
            server. Makes real requests to each service, so it can take a few seconds.
          </p>
        ) : (
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {results.map((r) => (
              <div
                key={r.service}
                className="flex flex-col gap-1.5 rounded-lg border border-line bg-surface-2 px-3 py-2"
              >
                <span className="truncate text-sm font-medium text-fg">{r.service}</span>
                <div className="flex flex-wrap items-center gap-1.5">
                  <Badge variant={STATUS_BADGE[r.status]}>{STATUS_LABEL[r.status]}</Badge>
                  {r.region && <span className="text-xs text-fg-faint">{r.region}</span>}
                </div>
                {r.detail && (
                  <span className="truncate text-xs text-fg-faint" title={r.detail}>
                    {r.detail}
                  </span>
                )}
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
