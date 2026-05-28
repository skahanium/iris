import { useCallback, useState } from "react";

import { AiPanel } from "@/components/ai/AiPanel";
import { CitationTaskPanel } from "@/components/ai/CitationTaskPanel";
import { OrganizePanel } from "@/components/ai/OrganizePanel";
import { ResearchPanel } from "@/components/ai/ResearchPanel";
import {
  WritingTaskPanel,
  type WritingEditorContext,
} from "@/components/ai/WritingTaskPanel";
import { cn } from "@/lib/utils";
import type { WorkflowTask } from "@/types/ai";

const TASKS: { id: WorkflowTask; label: string }[] = [
  { id: "research", label: "研究问题" },
  { id: "writing", label: "辅助写作" },
  { id: "citation", label: "检查引用" },
  { id: "organize", label: "整理建库" },
  { id: "chat", label: "自由对话" },
];

interface AiWorkflowPanelProps {
  notePath: string | null;
  noteDisplayTitle: string | null;
  noteContent: string;
  webSearch?: boolean;
  getWritingContext: () => WritingEditorContext | null;
  getParagraphText: () => string | null;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
}

export function AiWorkflowPanel({
  notePath,
  noteDisplayTitle,
  noteContent,
  webSearch = false,
  getWritingContext,
  getParagraphText,
  onPatchApplied,
  onVaultRefresh,
}: AiWorkflowPanelProps) {
  const [task, setTask] = useState<WorkflowTask>("research");

  const renderTask = useCallback(() => {
    switch (task) {
      case "research":
        return (
          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            <ResearchPanel notePath={notePath} webSearch={webSearch} />
          </div>
        );
      case "writing":
        return (
          <WritingTaskPanel
            notePath={notePath}
            noteContent={noteContent}
            webSearch={webSearch}
            getEditorContext={getWritingContext}
            onPatchApplied={onPatchApplied}
          />
        );
      case "citation":
        return (
          <CitationTaskPanel
            notePath={notePath}
            getParagraphText={getParagraphText}
            webSearch={webSearch}
          />
        );
      case "organize":
        return <OrganizePanel onApplied={onVaultRefresh} />;
      case "chat":
        return (
          <AiPanel
            notePath={notePath}
            noteDisplayTitle={noteDisplayTitle}
            noteContent={noteContent}
            webSearch={webSearch}
          />
        );
      default: {
        const _exhaustive: never = task;
        return _exhaustive;
      }
    }
  }, [
    task,
    notePath,
    noteDisplayTitle,
    noteContent,
    webSearch,
    getWritingContext,
    getParagraphText,
    onPatchApplied,
    onVaultRefresh,
  ]);

  return (
    <div className="flex h-full flex-col bg-panel">
      <div className="flex shrink-0 flex-wrap gap-1 border-b border-border px-2 py-2">
        {TASKS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={cn(
              "rounded-md px-2 py-1 text-xs transition-colors",
              task === t.id
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-muted",
            )}
            onClick={() => setTask(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {renderTask()}
      </div>
    </div>
  );
}
