import { useState } from "react";
import { ChevronDown } from "lucide-react";

import { assistantRunConfirmationDiff } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type {
  AssistantRunConfirmationDiffRequest,
  ConfirmationDiffLine,
  ConfirmationDiffPreview,
} from "@/types/ai";

export interface AssistantConfirmationDiffProps {
  /** Session-owned pending confirmation identity for the diff lookup. */
  request: AssistantRunConfirmationDiffRequest;
}

type DiffStatus = "idle" | "loading" | "ready" | "error";

function lineMarker(kind: ConfirmationDiffLine["kind"]): string {
  switch (kind) {
    case "add":
      return "+";
    case "del":
      return "−";
    default:
      return " ";
  }
}

function lineClass(kind: ConfirmationDiffLine["kind"]): string {
  switch (kind) {
    case "add":
      return "bg-success-bg text-success-fg";
    case "del":
      return "bg-destructive/10 text-destructive";
    default:
      return "text-muted-foreground";
  }
}

/**
 * Collapsed-by-default unified diff for one pending frozen change plan. The
 * preview is fetched on first expand and kept in memory only; it never enters
 * persisted events and never blocks approving or rejecting the plan.
 */
export function AssistantConfirmationDiff({
  request,
}: AssistantConfirmationDiffProps) {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<DiffStatus>("idle");
  const [preview, setPreview] = useState<ConfirmationDiffPreview | null>(null);

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next && status === "idle") {
      setStatus("loading");
      assistantRunConfirmationDiff(request)
        .then((result) => {
          setPreview(result);
          setStatus("ready");
        })
        .catch(() => {
          setStatus("error");
        });
    }
  };

  return (
    <div
      className="assistant-message-meta-disclosure"
      data-testid="assistant-confirmation-diff"
    >
      <button
        type="button"
        className="flex w-full min-w-0 items-center gap-1.5 text-left"
        onClick={toggle}
        aria-expanded={open}
        aria-controls="assistant-confirmation-diff-body"
        aria-label={open ? "折叠更改差异" : "展开更改差异"}
      >
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 shrink-0 transition-transform",
            !open && "-rotate-90",
          )}
        />
        <span className="shrink-0 font-medium text-foreground/75">
          更改差异
        </span>
      </button>
      {open ? (
        <div id="assistant-confirmation-diff-body" className="mt-2 space-y-2">
          {status === "loading" ? (
            <p className="text-muted-foreground">正在载入差异…</p>
          ) : null}
          {status === "error" ? (
            <p className="text-muted-foreground">差异暂不可用</p>
          ) : null}
          {status === "ready" && preview ? (
            <>
              {preview.files.map((file) => (
                <div key={file.path}>
                  <p className="font-medium text-foreground/75">{file.path}</p>
                  {!file.previewable ? (
                    <p className="text-muted-foreground">此目标无法预览差异</p>
                  ) : (
                    <div className="mt-1 space-y-1">
                      {file.hunks.map((hunk, hunkIndex) => (
                        <div
                          key={`${hunk.oldStart}-${hunk.newStart}-${hunkIndex}`}
                          className="space-y-px overflow-x-auto whitespace-pre-wrap break-words rounded-sm bg-surface-inset p-2 font-mono text-caption"
                        >
                          {hunk.lines.map((line, lineIndex) => (
                            <div
                              key={`${line.kind}-${lineIndex}`}
                              className={lineClass(line.kind)}
                            >
                              {lineMarker(line.kind)}
                              {line.text}
                            </div>
                          ))}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))}
              {preview.truncated ? (
                <p className="text-muted-foreground">
                  差异已截断，仅显示部分内容
                </p>
              ) : null}
            </>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
