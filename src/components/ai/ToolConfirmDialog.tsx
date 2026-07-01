import {
  Check,
  Download,
  FilePenLine,
  FileText,
  FolderOpen,
  GitCommitHorizontal,
  KeyRound,
  Search,
  ShieldCheck,
  Trash2,
  X,
  type LucideIcon,
} from "lucide-react";
import { useCallback } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import type { ToolConfirmRequestEvent } from "@/types/ipc";

export type ToolConfirmRequest = ToolConfirmRequestEvent;

type ToolTone = "neutral" | "read" | "web" | "write" | "danger" | "skill";

interface PermissionCard {
  action: string;
  target: string;
  detail?: string;
  impact: string;
  tone: ToolTone;
  Icon: LucideIcon;
}

interface ToolConfirmDialogProps {
  request: ToolConfirmRequest | null;
  onConfirm: (
    requestId: string,
    toolCallId: string,
    decision: "approve" | "reject" | "modify",
    modifiedArgs?: unknown,
  ) => void;
  onClose: () => void;
}

const WRITE_TOOLS = new Set([
  "insert_text_at_cursor",
  "replace_selection",
  "add_tags",
  "update_user_rule",
  "create_note_from_deposit",
  "vault_create_note",
  "vault_rename_move",
  "vault_delete_to_trash",
  "vault_asset_write",
  "fs_import_to_vault",
  "fs_export",
  "fs_write_authorized_export",
  "git_write_commit",
]);

function argText(
  request: ToolConfirmRequest,
  keys: string[],
  fallback = "",
): string {
  for (const key of keys) {
    const value = request.arguments[key];
    if (typeof value === "string" && value.trim()) return value.trim();
    if (typeof value === "number" || typeof value === "boolean") {
      return String(value);
    }
  }
  return fallback;
}

function objectValue(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function unknownStringList(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string");
}

function trustWarnings(request: ToolConfirmRequest): string[] {
  const trustPreview = objectValue(request.preview?.trust_profile_preview);
  return unknownStringList(trustPreview?.warnings);
}

function compactPath(path: string, fallback: string): string {
  if (!path) return fallback;
  const normalized = path.replaceAll("\\", "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts.at(-1) ?? fallback;
}

function targetPath(request: ToolConfirmRequest, fallback: string): string {
  const path = argText(request, [
    "path",
    "target_path",
    "targetPath",
    "note_path",
    "notePath",
    "file_path",
    "filePath",
    "relative_path",
    "relativePath",
  ]);
  return compactPath(path, fallback);
}

function buildPermissionCard(request: ToolConfirmRequest): PermissionCard {
  const permissionEffects = request.permissionEffects ?? [];
  const blockedEffect = permissionEffects.find((effect) =>
    Boolean(effect.blockedReason),
  );

  if (blockedEffect) {
    return {
      action: "当前不能执行",
      target: blockedEffect.scopeSummary || "权限边界",
      impact: "需要先调整相关权限或设置。",
      tone: "danger",
      Icon: ShieldCheck,
    };
  }

  switch (request.tool_name) {
    case "web_search":
      return {
        action: "联网搜索",
        target: argText(request, ["query", "q"], "当前问题"),
        impact: "搜索结果会进入当前对话。",
        tone: "web",
        Icon: Search,
      };
    case "insert_text_at_cursor":
      return {
        action: "修改笔记",
        target: targetPath(request, "当前笔记"),
        detail: "在当前光标位置插入文本",
        impact: "会直接修改当前笔记内容。",
        tone: "write",
        Icon: FilePenLine,
      };
    case "replace_selection":
      return {
        action: "修改笔记",
        target: targetPath(request, "当前笔记"),
        detail: "替换当前选区文本",
        impact: "会直接修改当前笔记内容。",
        tone: "write",
        Icon: FilePenLine,
      };
    case "add_tags":
      return {
        action: "添加标签",
        target: targetPath(request, "当前笔记"),
        impact: "会更新笔记元数据。",
        tone: "write",
        Icon: FilePenLine,
      };
    case "update_user_rule":
      return {
        action: "保存规则",
        target: argText(request, ["title", "name"], "长期规则"),
        impact: "会影响后续对话。",
        tone: "write",
        Icon: ShieldCheck,
      };
    case "create_note_from_deposit":
    case "vault_create_note":
      return {
        action: "创建笔记",
        target: targetPath(request, "新 Markdown 笔记"),
        impact: "会在当前库里创建新笔记。",
        tone: "write",
        Icon: FileText,
      };
    case "vault_rename_move": {
      const from = targetPath(request, "原笔记");
      const to = compactPath(
        argText(request, ["new_path", "newPath", "to", "target_path"]),
        "新位置",
      );
      return {
        action: "移动笔记",
        target: `${from} -> ${to}`,
        impact: "会修改笔记路径。",
        tone: "write",
        Icon: FilePenLine,
      };
    }
    case "vault_delete_to_trash":
      return {
        action: "移入回收站",
        target: targetPath(request, "当前笔记"),
        impact: "不会永久删除，之后仍可恢复。",
        tone: "danger",
        Icon: Trash2,
      };
    case "vault_asset_write":
      return {
        action: "写入资源",
        target: targetPath(request, "Vault 资源"),
        impact: "会把资源文件写入当前库。",
        tone: "write",
        Icon: Download,
      };
    case "git_write_commit":
      return {
        action: "创建 Git 提交",
        target: argText(request, ["message", "commit_message"], "当前改动"),
        impact: "会写入 Git 历史。",
        tone: "write",
        Icon: GitCommitHorizontal,
      };
    case "fs_import_to_vault":
      return {
        action: "导入文件",
        target: targetPath(request, "本地文件"),
        impact: "会把文件写入当前库。",
        tone: "write",
        Icon: FolderOpen,
      };
    case "fs_export":
    case "fs_write_authorized_export":
      return {
        action: "导出文件",
        target: targetPath(request, "已授权目录"),
        impact: "会把文件写到你授权的目录。",
        tone: "write",
        Icon: FolderOpen,
      };
    case "fs_read_authorized_folder":
      return {
        action: "读取目录",
        target: targetPath(request, "已授权目录"),
        impact: "只会读取该目录内容。",
        tone: "read",
        Icon: FolderOpen,
      };
    case "vault_version_list":
      return {
        action: "查看版本",
        target: targetPath(request, "当前笔记"),
        impact: "只会读取版本记录。",
        tone: "read",
        Icon: FileText,
      };
    case "git_read_status":
    case "git_read_diff":
    case "git_read_log":
      return {
        action:
          request.tool_name === "git_read_status"
            ? "读取 Git 状态"
            : request.tool_name === "git_read_diff"
              ? "读取 Git 差异"
              : "读取 Git 历史",
        target: "当前仓库",
        impact: "只会读取 Git 信息。",
        tone: "read",
        Icon: GitCommitHorizontal,
      };
    case "doc_normalize_markdown":
      return {
        action: "整理 Markdown",
        target: "当前文本",
        impact: "只会生成整理结果，不会直接写入文件。",
        tone: "read",
        Icon: FileText,
      };
    case "doc_extract_citations":
      return {
        action: "提取引用",
        target: "当前文本",
        impact: "只会分析文本，不会修改文件。",
        tone: "read",
        Icon: FileText,
      };
    case "secret_exists":
      return {
        action: "检查凭据",
        target: argText(request, ["name", "credential"], "系统凭据"),
        impact: "只检查是否存在，不会读取明文。",
        tone: "read",
        Icon: KeyRound,
      };
    default:
      return {
        action: WRITE_TOOLS.has(request.tool_name) ? "执行更改" : "读取信息",
        target: targetPath(request, "Iris 请求的操作"),
        impact: WRITE_TOOLS.has(request.tool_name)
          ? "会修改当前数据。"
          : "只会读取当前数据。",
        tone: WRITE_TOOLS.has(request.tool_name) ? "write" : "read",
        Icon: WRITE_TOOLS.has(request.tool_name) ? FilePenLine : ShieldCheck,
      };
  }
}

function toneClassName(tone: ToolTone): string {
  switch (tone) {
    case "web":
      return "border-sky-200/70 bg-sky-50/70 text-sky-900 dark:border-sky-400/20 dark:bg-sky-400/10 dark:text-sky-100";
    case "write":
      return "border-amber-200/80 bg-amber-50/70 text-amber-950 dark:border-amber-400/20 dark:bg-amber-400/10 dark:text-amber-100";
    case "danger":
      return "border-destructive/25 bg-destructive/10 text-destructive";
    case "skill":
      return "border-emerald-200/80 bg-emerald-50/70 text-emerald-950 dark:border-emerald-400/20 dark:bg-emerald-400/10 dark:text-emerald-100";
    case "read":
    case "neutral":
    default:
      return "border-border/70 bg-surface-inset text-foreground";
  }
}

export function ToolConfirmDialog({
  request,
  onConfirm,
  onClose,
}: ToolConfirmDialogProps) {
  const handleApprove = useCallback(() => {
    if (!request) return;
    onConfirm(request.request_id, request.tool_call_id, "approve");
    onClose();
  }, [request, onConfirm, onClose]);

  const handleReject = useCallback(() => {
    if (!request) return;
    onConfirm(request.request_id, request.tool_call_id, "reject");
    onClose();
  }, [request, onConfirm, onClose]);

  if (!request) return null;

  const card = buildPermissionCard(request);
  const Icon = card.Icon;
  const sandboxProfile = request.sandboxProfile;
  const permissionDecision = request.permissionDecision;
  const warnings = trustWarnings(request);
  const pendingIndex = request.pendingConfirmationIndex ?? 1;
  const pendingCount = request.pendingConfirmationCount ?? 1;

  return (
    <Dialog open={!!request} onOpenChange={() => onClose()}>
      <DialogContent className="ai-task-surface max-w-[420px] p-0">
        <DialogHeader className="px-5 pb-0 pt-5">
          <div
            className={cn(
              "mb-3 flex h-10 w-10 items-center justify-center rounded-xl border",
              toneClassName(card.tone),
            )}
          >
            <Icon className="h-5 w-5" />
          </div>
          <DialogTitle className="text-base font-semibold">
            {card.action}
          </DialogTitle>
          <DialogDescription className="sr-only">
            Iris 请求执行这项操作，需要你的确认。
          </DialogDescription>
        </DialogHeader>

        <div className="px-5 pb-4 pt-4">
          <div
            className={cn(
              "rounded-lg border px-4 py-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.45)]",
              toneClassName(card.tone),
            )}
          >
            <p className="break-words text-sm font-semibold leading-5">
              {card.target}
            </p>
            {card.detail ? (
              <p className="mt-1 break-words text-xs leading-5 opacity-75">
                {card.detail}
              </p>
            ) : null}
          </div>

          <p className="mt-3 text-xs leading-5 text-muted-foreground">
            {card.impact}
          </p>

          <div className="mt-3 space-y-2">
            <div className="rounded-md border border-border/60 px-3 py-2 text-xs">
              <div className="flex items-center justify-between gap-3">
                <span className="font-medium">确认进度</span>
                <span className="text-muted-foreground">
                  {pendingIndex} / {pendingCount}
                </span>
              </div>
              {permissionDecision ? (
                <p className="mt-1 text-muted-foreground">
                  权限决策: {permissionDecision.decision}
                </p>
              ) : null}
            </div>

            {sandboxProfile ? (
              <div
                className="rounded-md border border-border/60 px-3 py-2 text-xs"
                data-testid="tool-confirm-sandbox-profile"
              >
                <div className="flex items-center justify-between gap-3">
                  <span className="font-medium">Sandbox Profile</span>
                  <span className="text-muted-foreground">
                    {sandboxProfile.level}
                  </span>
                </div>
                <p className="mt-1 text-muted-foreground">
                  {sandboxProfile.summary}
                </p>
                {sandboxProfile.support === "unsupported" ? (
                  <p className="mt-1 text-destructive">
                    当前运行时不支持该级别隔离。
                  </p>
                ) : null}
              </div>
            ) : null}

            {warnings.length > 0 ? (
              <div className="rounded-md border border-amber-300/70 bg-amber-50 px-3 py-2 text-xs text-amber-900">
                <p className="font-medium">Skill Trust</p>
                {warnings.slice(0, 3).map((warning) => (
                  <p key={warning} className="mt-1">
                    {warning}
                  </p>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        <DialogFooter className="border-t border-border/60 bg-surface-inset/60 px-5 py-4">
          <Button variant="outline" size="sm" onClick={handleReject}>
            <X className="h-4 w-4" />
            拒绝
          </Button>
          <Button size="sm" onClick={handleApprove}>
            <Check className="h-4 w-4" />
            允许
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
