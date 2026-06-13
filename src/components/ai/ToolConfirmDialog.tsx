import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronUp,
  Edit3,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { toolAuditQuery, type ToolAuditEntry } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { ToolConfirmRequestEvent } from "@/types/ipc";

// ─── Tool Confirm Request Type ───────────────────────────

export type ToolConfirmRequest = ToolConfirmRequestEvent;

// ─── Tool Display Names ──────────────────────────────────

const TOOL_DISPLAY_NAMES: Record<string, string> = {
  insert_text_at_cursor: "在光标处插入文本",
  replace_selection: "替换选中文本",
  add_tags: "添加标签",
  confirm_block_link: "确认块级链接",
  save_genre_template: "保存文种模板",
  update_user_rule: "更新用户规则",
  create_note_from_deposit: "从收件箱创建笔记",
  web_search: "联网搜索",
  fetch_web_page: "打开网页正文",
  skills_install: "安装 Skill",
  skills_uninstall: "卸载 Skill",
  skills_toggle: "启停 Skill",
  vault_create_note: "创建 Markdown 笔记",
  vault_rename_move: "重命名或移动笔记",
  vault_delete_to_trash: "移入回收站",
  vault_asset_write: "写入 Vault 资源",
  vault_version_list: "查看版本快照",
  git_read_status: "读取 Git 状态",
  git_read_diff: "读取 Git 差异",
  git_read_log: "读取 Git 历史",
  git_write_commit: "创建 Git 提交",
  fs_import_to_vault: "导入外部 Markdown",
  fs_export: "导出外部文件",
  fs_read_authorized_folder: "读取授权目录摘要",
  fs_write_authorized_export: "写入授权导出目录",
  doc_normalize_markdown: "规范化 Markdown",
  doc_extract_citations: "提取引用信息",
  web_to_markdown: "网页转 Markdown",
  web_download_to_assets: "下载到 Vault 资源",
  web_citation_extract: "提取网页引用",
  skill_request_capabilities: "预检 Skill 能力",
  process_run_readonly: "运行只读命令",
  secret_exists: "检查凭据是否存在",
};

const TOOL_DESCRIPTIONS: Record<string, string> = {
  insert_text_at_cursor: "将在编辑器当前光标位置插入以下文本。请确认内容无误。",
  replace_selection: "将替换编辑器中当前选中的文本。请确认替换内容。",
  add_tags: "将为指定笔记添加标签。请确认标签列表。",
  confirm_block_link: "将确认一条 AI 建议的隐含块级链接。",
  save_genre_template: "将保存或更新文种模板。",
  update_user_rule: "将添加或更新一条长期规则。此规则将在后续对话中生效。",
  create_note_from_deposit: "将从 AI 收件箱创建一个新的 .md 笔记文件。",
  web_search: "将进行联网搜索。注意：外部网页内容可信度较低。",
  fetch_web_page:
    "将从指定 HTTPS 地址下载并提取正文片段（受体积与频率限制）。请确认 URL 正确且您有权访问该页面。",
  skills_install:
    "将从指定来源安装 Agent Skill 到本地 skills 目录。新 skill 默认启用；可在设置 → Skills 查看与管理。",
  skills_uninstall: "将从 skills 目录删除该 skill 及其本地文件。",
  skills_toggle: "将启用或禁用该 skill；禁用后其指令与工具扩权不再生效。",
  vault_create_note:
    "将在当前 Markdown vault 中创建新的 .md 笔记。请确认目标路径和初始内容摘要。",
  vault_rename_move:
    "将重命名或移动笔记，并同步常见 wikilink 引用。请确认 link impact 摘要。",
  vault_delete_to_trash:
    "将把笔记移入 Iris 回收站，可在保留期内恢复，不会直接永久删除。",
  vault_asset_write:
    "将把资源写入 vault 的 assets/ 目录。请确认目标路径和资源大小。",
  vault_version_list: "将读取指定笔记的版本快照列表，不修改 Markdown 内容。",
  git_read_status: "将读取当前 vault 的 Git 状态摘要，不返回文件正文。",
  git_read_diff: "将读取当前 vault 的 Git 差异摘要；默认只返回统计信息。",
  git_read_log: "将读取当前 vault 的 Git 提交历史摘要。",
  git_write_commit:
    "将在当前 vault 内仅对指定路径执行 git add 并创建提交。请确认路径和提交信息。",
  fs_import_to_vault:
    "将从用户授权目录读取外部 UTF-8 Markdown，并导入当前 vault 的目标 .md 路径。",
  fs_export: "将把内容写入用户授权的外部导出目录，不允许越过授权根目录。",
  fs_read_authorized_folder:
    "将列出用户授权目录的文件名、类型和大小摘要，不读取文件正文。",
  fs_write_authorized_export: "将内容写入用户授权导出目录中的指定相对路径。",
  doc_normalize_markdown:
    "将规范化 Markdown 文本的换行与多余空行，不直接写入文件。",
  doc_extract_citations:
    "将从给定文本中提取 URL 引用元数据，不修改 Markdown 文件。",
  web_to_markdown:
    "将抓取 HTTPS 页面正文并生成 Markdown 草稿；需要本轮已授权 Web 访问。",
  web_download_to_assets:
    "将从 HTTPS 下载资源并写入当前 vault 的 assets/ 目录。",
  web_citation_extract:
    "将抓取 HTTPS 页面并提取标题、URL、访问时间等引用元数据。",
  skill_request_capabilities:
    "将检查 Skill 请求的能力在 Iris 中是否支持、需要确认或被阻断。",
  process_run_readonly:
    "将在 vault 内运行受控只读命令 allowlist，并限制环境、时间和输出大小。",
  secret_exists:
    "将检查指定 named credential 是否存在；不会读取、显示或传给模型任何明文凭据。",
};

type RiskMeta = {
  label: string;
  className: string;
  badgeClassName: string;
};

const DEFAULT_RISK_META: RiskMeta = {
  label: "中风险",
  className: "border-amber-200 bg-amber-50/80 text-amber-900",
  badgeClassName: "border-amber-300 bg-white/70 text-amber-800",
};

const RISK_META: Record<string, RiskMeta> = {
  low: {
    label: "低风险",
    className: "border-emerald-200 bg-emerald-50/80 text-emerald-900",
    badgeClassName: "border-emerald-300 bg-white/70 text-emerald-800",
  },
  medium: {
    label: "中风险",
    className: "border-amber-200 bg-amber-50/80 text-amber-900",
    badgeClassName: "border-amber-300 bg-white/70 text-amber-800",
  },
  high: {
    label: "高风险",
    className: "border-orange-300 bg-orange-50/85 text-orange-950",
    badgeClassName: "border-orange-300 bg-white/75 text-orange-900",
  },
  critical: {
    label: "关键风险",
    className: "border-destructive/35 bg-destructive/10 text-destructive",
    badgeClassName: "border-destructive/35 bg-background text-destructive",
  },
};

function riskMeta(riskLevel: string): RiskMeta {
  return RISK_META[riskLevel] ?? DEFAULT_RISK_META;
}

// ─── Component ───────────────────────────────────────────

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

export function ToolConfirmDialog({
  request,
  onConfirm,
  onClose,
}: ToolConfirmDialogProps) {
  const [editing, setEditing] = useState(false);
  const [modifiedArgs, setModifiedArgs] = useState("");
  const [auditEntries, setAuditEntries] = useState<ToolAuditEntry[]>([]);
  const [showAudit, setShowAudit] = useState(false);

  useEffect(() => {
    if (request?.request_id) {
      toolAuditQuery(request.request_id)
        .then(setAuditEntries)
        .catch(() => setAuditEntries([]));
    } else {
      setAuditEntries([]);
    }
  }, [request?.request_id]);

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

  const handleModify = useCallback(() => {
    if (!request) return;
    try {
      const parsed = JSON.parse(modifiedArgs);
      onConfirm(request.request_id, request.tool_call_id, "modify", parsed);
      onClose();
    } catch (e) {
      console.warn("[ToolConfirm] JSON parse failed:", e);
      alert("修改后的参数必须是有效的 JSON 格式");
    }
  }, [request, modifiedArgs, onConfirm, onClose]);

  if (!request) return null;

  const displayName =
    TOOL_DISPLAY_NAMES[request.tool_name] ?? request.tool_name;
  const description =
    TOOL_DESCRIPTIONS[request.tool_name] ?? "请确认是否执行此操作。";
  const fetchUrl =
    request.tool_name === "fetch_web_page"
      ? String(request.arguments.url ?? "")
      : "";
  const fetchHost = (() => {
    if (!fetchUrl) return "";
    try {
      return new URL(fetchUrl).hostname;
    } catch {
      return "";
    }
  })();

  const isWriteOperation = [
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
    "web_download_to_assets",
    "git_write_commit",
  ].includes(request.tool_name);
  const baseContentHash = String(
    request.arguments.base_content_hash ??
      request.arguments.baseContentHash ??
      "",
  );
  const riskLevel = String(request.arguments.risk_level ?? "medium");
  const showPatchReview =
    isWriteOperation &&
    (request.tool_name === "insert_text_at_cursor" ||
      request.tool_name === "replace_selection");
  const permissionEffects = request.permissionEffects ?? [];

  return (
    <Dialog open={!!request} onOpenChange={() => onClose()}>
      <DialogContent className="ai-task-surface max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isWriteOperation && (
              <AlertTriangle className="h-5 w-5 text-amber-500" />
            )}
            确认工具调用
          </DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* Tool name badge */}
          <div className="flex items-center gap-2">
            <Badge variant="secondary">{displayName}</Badge>
            <span className="text-xs text-muted-foreground">
              {request.tool_name}
            </span>
          </div>

          {permissionEffects.length > 0 ? (
            <section className="space-y-2">
              <div className="flex items-center justify-between gap-2">
                <p className="text-xs font-medium text-foreground">权限影响</p>
                <span className="text-[11px] text-muted-foreground">
                  本次批准仅用于当前工具调用
                </span>
              </div>
              <div className="space-y-2">
                {permissionEffects.map((effect) => {
                  const meta = riskMeta(effect.riskLevel);
                  return (
                    <div
                      key={`${effect.permissionName}:${effect.scopeSummary}`}
                      className={cn(
                        "rounded-md border px-3 py-2 text-xs",
                        meta.className,
                      )}
                    >
                      <div className="flex flex-wrap items-center gap-1.5">
                        <span className="font-mono font-semibold">
                          {effect.permissionName}
                        </span>
                        <Badge
                          variant="outline"
                          className={cn(
                            "rounded-md px-1.5 py-0 text-[10px]",
                            meta.badgeClassName,
                          )}
                        >
                          {effect.riskLevel} · {meta.label}
                        </Badge>
                        <Badge
                          variant="outline"
                          className="rounded-md bg-background/60 px-1.5 py-0 text-[10px]"
                        >
                          {effect.scopeKind}
                        </Badge>
                      </div>
                      <div className="mt-1.5 space-y-1 leading-relaxed">
                        <p>
                          <span className="font-medium">作用域：</span>
                          <span className="break-all">
                            {effect.scopeSummary}
                          </span>
                        </p>
                        <p>
                          <span className="font-medium">可撤销方式：</span>
                          {effect.reversibleBy}
                        </p>
                        {effect.blockedReason ? (
                          <p>
                            <span className="font-medium">阻断原因：</span>
                            {effect.blockedReason}
                          </p>
                        ) : null}
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
          ) : null}

          {request.tool_name === "fetch_web_page" && fetchUrl ? (
            <div className="space-y-1 rounded-md border border-amber-200 bg-amber-50/80 p-3 text-xs">
              <p className="font-medium text-amber-900">目标 URL</p>
              <p className="break-all font-mono text-amber-800">{fetchUrl}</p>
              {fetchHost ? (
                <p className="text-amber-700">域名：{fetchHost}</p>
              ) : null}
              <p className="text-amber-700">
                受单页体积与每轮抓取次数限制；仅 HTTPS。
              </p>
            </div>
          ) : null}

          {request.tool_name === "skills_install" && request.preview ? (
            <div className="space-y-1 rounded-md border border-primary/20 bg-primary/5 p-3 text-xs">
              <p className="font-medium text-foreground">安装预览</p>
              {typeof request.preview.display_name === "string" ? (
                <p>名称：{request.preview.display_name}</p>
              ) : null}
              {typeof request.preview.resolved_source === "string" ? (
                <p>来源类型：{request.preview.resolved_source}</p>
              ) : null}
              {typeof request.preview.resolved_url === "string" ? (
                <p className="break-all font-mono">
                  解析 URL：{request.preview.resolved_url}
                </p>
              ) : null}
              {typeof request.preview.path_or_url === "string" &&
              !request.preview.resolved_url ? (
                <p className="break-all font-mono">
                  路径/URL：{request.preview.path_or_url}
                </p>
              ) : null}
              <p className="text-muted-foreground">
                写入 ~
                {String(request.arguments.scope ?? "global") === "vault"
                  ? "笔记库/.iris/skills"
                  : "/.iris/skills"}
              </p>
            </div>
          ) : null}

          {(request.tool_name === "skills_uninstall" ||
            request.tool_name === "skills_toggle") && (
            <div className="rounded-md border border-border/80 bg-surface-inset p-3 text-xs">
              <p className="font-medium">
                Skill：{String(request.arguments.name ?? "")}
              </p>
              <p className="text-muted-foreground">
                范围：{String(request.arguments.scope ?? "global")}
              </p>
              {request.tool_name === "skills_toggle" ? (
                <p className="text-muted-foreground">
                  操作：{request.arguments.enabled ? "启用" : "禁用"}
                </p>
              ) : null}
            </div>
          )}

          {/* Arguments display */}
          <div className="rounded-md bg-muted p-3">
            <p className="mb-2 text-xs font-medium text-muted-foreground">
              调用参数：
            </p>
            {editing ? (
              <Textarea
                value={modifiedArgs}
                onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) =>
                  setModifiedArgs(e.target.value)
                }
                className="font-mono text-xs"
                rows={6}
                placeholder="修改后的 JSON 参数"
              />
            ) : (
              <pre className="whitespace-pre-wrap break-all text-xs">
                {String(JSON.stringify(request.arguments, null, 2))}
              </pre>
            )}
          </div>

          {/* Diff preview for text operations */}
          {showPatchReview && (
            <div className="rounded-md border border-border/80 bg-surface-inset p-3 text-xs">
              <p className="mb-2 font-medium text-foreground">Patch 审阅</p>
              <div className="space-y-1 text-muted-foreground">
                <p>
                  base_content_hash：
                  <span className="font-mono text-foreground">
                    {baseContentHash || "待执行前校验"}
                  </span>
                </p>
                <p>
                  risk_level：
                  <span className="font-mono text-foreground">{riskLevel}</span>
                </p>
              </div>
            </div>
          )}

          {request.tool_name === "insert_text_at_cursor" &&
            request.arguments.text && (
              <div className="rounded-lg border border-border/80 bg-surface-inset p-3">
                <p className="mb-1 text-xs font-medium text-foreground">
                  将插入的文本：
                </p>
                <p className="whitespace-pre-wrap text-sm">
                  {String(request.arguments.text as string)}
                </p>
              </div>
            )}

          {request.tool_name === "replace_selection" &&
            request.arguments.replacement && (
              <div className="rounded-lg border border-border/80 bg-surface-inset p-3">
                <p className="mb-1 text-xs font-medium text-foreground">
                  替换为：
                </p>
                <p className="whitespace-pre-wrap text-sm">
                  {String(request.arguments.replacement as string)}
                </p>
              </div>
            )}

          {/* Warning for write operations */}
          {isWriteOperation && (
            <div className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 p-3">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
              <p className="text-xs text-amber-700">
                此操作将修改您的笔记内容。请仔细确认后再执行。
              </p>
            </div>
          )}

          {/* Audit history for this request */}
          {auditEntries.length > 0 && (
            <div className="rounded-md border border-border/60">
              <button
                type="button"
                className="flex w-full items-center justify-between px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-muted/50"
                onClick={() => setShowAudit((v) => !v)}
              >
                <span>本次会话已执行 {auditEntries.length} 个工具调用</span>
                {showAudit ? (
                  <ChevronUp className="h-3.5 w-3.5" />
                ) : (
                  <ChevronDown className="h-3.5 w-3.5" />
                )}
              </button>
              {showAudit && (
                <div className="max-h-40 space-y-1 overflow-y-auto border-t border-border/60 px-3 py-2">
                  {auditEntries.map((entry) => (
                    <div
                      key={entry.id}
                      className="flex items-center gap-2 text-xs"
                    >
                      <span
                        className={
                          entry.success ? "text-green-600" : "text-red-500"
                        }
                      >
                        {entry.success ? "✓" : "✗"}
                      </span>
                      <span className="font-mono">{entry.tool_name}</span>
                      {entry.arguments_summary && (
                        <span className="truncate text-muted-foreground">
                          {entry.arguments_summary}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        <DialogFooter className="flex-col gap-2 sm:flex-row">
          <div className="flex flex-1 gap-2">
            {!editing && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setModifiedArgs(JSON.stringify(request.arguments, null, 2));
                  setEditing(true);
                }}
              >
                <Edit3 className="mr-1 h-4 w-4" />
                修改参数
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button variant="destructive" size="sm" onClick={handleReject}>
              <X className="mr-1 h-4 w-4" />
              拒绝
            </Button>
            {editing ? (
              <Button size="sm" onClick={handleModify}>
                <Check className="mr-1 h-4 w-4" />
                确认修改
              </Button>
            ) : (
              <Button size="sm" onClick={handleApprove}>
                <Check className="mr-1 h-4 w-4" />
                批准执行
              </Button>
            )}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
