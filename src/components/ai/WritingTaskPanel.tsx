import { useCallback, useState } from "react";
import { Loader2, PenLine } from "lucide-react";

import { PatchPreview } from "@/components/ai/PatchPreview";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ContextPacketDrawer } from "@/components/ai/ContextPacketDrawer";
import { invokeErrorMessage } from "@/lib/credentials";
import { sha256Hex } from "@/lib/content-hash";
import { patchApply, writingExecute } from "@/lib/ipc";
import type { ContextPacket, PatchProposal } from "@/types/ai";

export interface WritingEditorContext {
  selection: string;
  cursorContext: string;
}

interface WritingTaskPanelProps {
  notePath: string | null;
  noteContent: string;
  webSearch?: boolean;
  getEditorContext: () => WritingEditorContext | null;
  onPatchApplied?: (newContent: string) => void;
}

export function WritingTaskPanel({
  notePath,
  noteContent,
  webSearch = false,
  getEditorContext,
  onPatchApplied,
}: WritingTaskPanelProps) {
  const [goal, setGoal] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [patches, setPatches] = useState<PatchProposal[]>([]);
  const [packets, setPackets] = useState<ContextPacket[]>([]);
  const [packetsOpen, setPacketsOpen] = useState(false);

  const runWriting = useCallback(async () => {
    if (!notePath || !goal.trim()) return;
    const ctx = getEditorContext();
    if (!ctx) {
      setError("请先在编辑器中选中文本或放置光标");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const hash = await sha256Hex(noteContent);
      const res = await writingExecute({
        target_path: notePath,
        base_content_hash: hash,
        selection: ctx.selection || undefined,
        cursor_context: ctx.cursorContext,
        writing_goal: goal.trim(),
        web_authorized: webSearch,
      });
      setPatches(res.patches as PatchProposal[]);
      setPackets(res.evidence_used as ContextPacket[]);
      if (res.patches.length > 0) {
        setPacketsOpen(true);
      }
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [notePath, goal, noteContent, webSearch, getEditorContext]);

  const handleAccept = useCallback(
    async (patch: PatchProposal) => {
      try {
        const result = await patchApply(patch);
        if (!result.success) {
          setError(result.error ?? "补丁应用失败");
          return;
        }
        const ctx = getEditorContext();
        const base = ctx?.cursorContext ?? noteContent;
        const before = base.slice(0, patch.range.start);
        const after = base.slice(patch.range.end);
        const next = before + patch.replacement_text + after;
        onPatchApplied?.(next);
        setPatches((prev) => prev.filter((p) => p.id !== patch.id));
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [getEditorContext, noteContent, onPatchApplied],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden p-3">
      <Card className="border-border/60 shrink-0">
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium">辅助写作</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          <textarea
            className="min-h-[72px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm"
            placeholder="描述写作目标：续写、改写、补依据…"
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
          />
          <Button
            type="button"
            size="sm"
            className="w-full"
            disabled={loading || !notePath}
            onClick={() => void runWriting()}
          >
            {loading ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : (
              <PenLine className="mr-1 h-3.5 w-3.5" />
            )}
            生成补丁建议
          </Button>
          {error ? (
            <p className="text-xs text-destructive">{error}</p>
          ) : null}
        </CardContent>
      </Card>

      <ContextPacketDrawer
        open={packetsOpen}
        onOpenChange={setPacketsOpen}
        packets={packets}
        selectedIds={[]}
        onSelect={() => {}}
      />

      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto">
        {patches.map((patch) => (
          <PatchPreview
            key={patch.id}
            patch={patch}
            onAccept={(p) => void handleAccept(p)}
            onReject={(p) =>
              setPatches((prev) => prev.filter((x) => x.id !== p.id))
            }
            onCopy={(p) => void navigator.clipboard.writeText(p.replacement_text)}
            onRegenerate={() => void runWriting()}
          />
        ))}
      </div>
    </div>
  );
}
