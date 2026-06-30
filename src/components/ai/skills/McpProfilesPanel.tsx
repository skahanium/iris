import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  mcpRuntimeHealthEventsList,
  mcpRuntimeHealthCheck,
  mcpRuntimeProfileDelete,
  mcpRuntimeProfileToggle,
  mcpRuntimeProfilesList,
  mcpRuntimeToolInventoryList,
  mcpRuntimeToolsList,
  type McpHealthEventSummaryDto,
  type McpRuntimeProfileSummaryDto,
  type McpToolInventorySummaryDto,
} from "@/lib/ipc";

import { McpProfileCard } from "./McpProfileCard";

interface ProfileDetails {
  inventory: McpToolInventorySummaryDto[];
  healthEvents: McpHealthEventSummaryDto[];
}

export function McpProfilesPanel({ open }: { open: boolean }) {
  const [profiles, setProfiles] = useState<McpRuntimeProfileSummaryDto[]>([]);
  const [details, setDetails] = useState<Record<string, ProfileDetails>>({});
  const [loading, setLoading] = useState(false);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!open) return;
    setLoading(true);
    setError(null);
    try {
      const nextProfiles = await mcpRuntimeProfilesList();
      const detailPairs = await Promise.all(
        nextProfiles.map(async (profile) => {
          const [inventory, healthEvents] = await Promise.all([
            mcpRuntimeToolInventoryList(profile.id),
            mcpRuntimeHealthEventsList(profile.id, 5),
          ]);
          return [profile.id, { inventory, healthEvents }] as const;
        }),
      );
      setProfiles(nextProfiles);
      setDetails(Object.fromEntries(detailPairs));
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setLoading(false);
    }
  }, [open]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runProfileAction = useCallback(
    async (actionId: string, action: () => Promise<unknown>) => {
      setPendingAction(actionId);
      setError(null);
      try {
        await action();
        await refresh();
      } catch (nextError) {
        setError(invokeErrorMessage(nextError));
      } finally {
        setPendingAction(null);
      }
    },
    [refresh],
  );

  const handleToggle = useCallback(
    (profile: McpRuntimeProfileSummaryDto) =>
      runProfileAction(`toggle:${profile.id}`, () =>
        mcpRuntimeProfileToggle(profile.id, !profile.enabled),
      ),
    [runProfileAction],
  );

  const handleDelete = useCallback(
    (profile: McpRuntimeProfileSummaryDto) => {
      const confirmed =
        typeof window === "undefined" ||
        window.confirm(`删除 MCP profile：${profile.display_name}？`);
      if (!confirmed) return;
      void runProfileAction(`delete:${profile.id}`, () =>
        mcpRuntimeProfileDelete(profile.id),
      );
    },
    [runProfileAction],
  );

  const handleHealthCheck = useCallback(
    (profile: McpRuntimeProfileSummaryDto) =>
      runProfileAction(`health:${profile.id}`, () =>
        mcpRuntimeHealthCheck(profile.id),
      ),
    [runProfileAction],
  );

  const handleDiscoverTools = useCallback(
    (profile: McpRuntimeProfileSummaryDto) =>
      runProfileAction(`discover:${profile.id}`, () =>
        mcpRuntimeToolsList(profile.id),
      ),
    [runProfileAction],
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium text-muted-foreground">
          MCP / Providers
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={loading}
          onClick={() => void refresh()}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          刷新
        </Button>
      </div>

      {error ? <p className="text-xs text-destructive">{error}</p> : null}

      {profiles.length === 0 ? (
        <p className="rounded-md border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground">
          暂无 MCP profiles
        </p>
      ) : (
        profiles.map((profile) => (
          <McpProfileCard
            key={profile.id}
            profile={profile}
            inventory={details[profile.id]?.inventory ?? []}
            healthEvents={details[profile.id]?.healthEvents ?? []}
            pendingAction={pendingAction}
            onToggle={() => void handleToggle(profile)}
            onDelete={() => handleDelete(profile)}
            onHealthCheck={() => void handleHealthCheck(profile)}
            onDiscoverTools={() => void handleDiscoverTools(profile)}
          />
        ))
      )}
    </div>
  );
}
