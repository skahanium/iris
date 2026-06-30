import { Activity, Database, Server } from "lucide-react";

import type {
  McpHealthEventSummaryDto,
  McpRuntimeProfileSummaryDto,
  McpToolInventorySummaryDto,
} from "@/lib/ipc";

export function McpProfileCard({
  profile,
  inventory,
  healthEvents,
}: {
  profile: McpRuntimeProfileSummaryDto;
  inventory: McpToolInventorySummaryDto[];
  healthEvents: McpHealthEventSummaryDto[];
}) {
  const latestEvent = healthEvents[0];

  return (
    <div className="rounded-lg border border-border/70 bg-background px-3 py-3 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-2">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate text-sm font-medium">{profile.display_name}</p>
            <span className="rounded-full border border-border/70 bg-muted/60 px-2 py-0.5 text-[10px] text-muted-foreground">
              MCP Profile
            </span>
            <span className="rounded-full border border-border/70 bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
              {profile.enabled ? "已启用" : "已禁用"}
            </span>
            <span
              className={`rounded-full border px-2 py-0.5 text-[10px] ${
                profile.status === "ready"
                  ? "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900/60 dark:bg-emerald-950/35 dark:text-emerald-300"
                  : "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/35 dark:text-amber-300"
              }`}
            >
              {profile.status}
            </span>
          </div>

          <div className="grid gap-1 text-[11px] text-muted-foreground">
            <div className="flex min-w-0 items-center gap-1.5">
              <Server className="h-3.5 w-3.5 shrink-0" />
              <span className="shrink-0">profile id</span>
              <span className="truncate text-foreground/75">{profile.id}</span>
            </div>
            <div className="flex min-w-0 items-center gap-1.5">
              <Database className="h-3.5 w-3.5 shrink-0" />
              <span className="shrink-0">server id</span>
              <span className="truncate text-foreground/75">
                {profile.server_id}
              </span>
            </div>
            <div className="flex min-w-0 items-center gap-1.5">
              <Activity className="h-3.5 w-3.5 shrink-0" />
              <span className="shrink-0">tool inventory</span>
              <span className="truncate text-foreground/75">
                {inventory.length}
              </span>
            </div>
          </div>

          {profile.last_error ? (
            <p className="rounded-md border border-amber-200 bg-amber-50 px-2 py-1.5 text-[11px] leading-5 text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/35 dark:text-amber-200">
              {profile.last_error}
            </p>
          ) : null}

          {latestEvent ? (
            <p className="text-[11px] leading-5 text-muted-foreground">
              最近健康事件：{latestEvent.reason_code}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
