import { Button } from "@/components/ui/button";

export type WorkspaceEmptyMode = "vault" | "workspace";

export interface WorkspaceEmptyProps {
  mode: WorkspaceEmptyMode;
  onNew: () => void | Promise<void>;
  onOpenRecent?: () => void | Promise<void>;
  errorMessage?: string | null;
}

const COPY: Record<WorkspaceEmptyMode, { hint: string; newLabel: string }> = {
  vault: { hint: "还没有笔记", newLabel: "新建第一篇" },
  workspace: { hint: "未打开笔记", newLabel: "新建笔记" },
};

export function WorkspaceEmpty({
  mode,
  onNew,
  onOpenRecent,
  errorMessage,
}: WorkspaceEmptyProps) {
  const { hint, newLabel } = COPY[mode];

  return (
    <div
      data-testid="workspace-empty"
      data-mode={mode}
      className="flex flex-1 items-center justify-center"
    >
      <div className="max-w-sm space-y-4 text-center">
        <p className="text-sm text-muted-foreground">{hint}</p>
        {errorMessage ? (
          <p role="status" className="text-sm text-destructive">
            {errorMessage}
          </p>
        ) : null}
        <div className="flex flex-col items-center gap-2">
          <Button
            type="button"
            variant="brand"
            data-testid="workspace-empty-new"
            onClick={() => {
              void onNew();
            }}
          >
            {newLabel}
          </Button>
          {mode === "workspace" && onOpenRecent ? (
            <Button
              type="button"
              variant="ghost"
              data-testid="workspace-empty-open-recent"
              onClick={() => {
                void onOpenRecent();
              }}
            >
              打开最近
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
