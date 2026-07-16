import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import {
  documentRecoveryAudit,
  documentRecoveryRestoreMissing,
  documentRecoveryRestoreOrphan,
  documentTitleRepair,
} from "@/lib/ipc";
import type {
  DocumentRecoveryAudit,
  DocumentTitleAuditItem,
  MissingDocumentRecoveryItem,
  OrphanedDocumentRecoveryItem,
} from "@/types/ipc";

interface DocumentRecoveryDialogProps {
  open: boolean;
  onClose: () => void;
  onRecovered: () => void;
}

function candidateSourceLabel(
  source: DocumentTitleAuditItem["candidateSource"],
): string {
  switch (source) {
    case "version":
      return "历史版本";
    case "index":
      return "现有索引";
    case "filename":
      return "文件名";
    default:
      return "无可靠候选";
  }
}

/** Audits and explicitly restores title, missing-document, and CAS-orphan recovery candidates. */
export function DocumentRecoveryDialog({
  open,
  onClose,
  onRecovered,
}: DocumentRecoveryDialogProps) {
  const [audit, setAudit] = useState<DocumentRecoveryAudit | null>(null);
  const [loading, setLoading] = useState(false);
  const [repairingId, setRepairingId] = useState<string | null>(null);
  const [orphanTargets, setOrphanTargets] = useState<Record<string, string>>(
    {},
  );
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await documentRecoveryAudit();
      setAudit(next);
      setOrphanTargets(
        Object.fromEntries(
          next.orphanedDocuments.map((item) => [
            item.objectHash,
            item.suggestedPath,
          ]),
        ),
      );
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : String(nextError),
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  const repairTitle = async (item: DocumentTitleAuditItem) => {
    if (!item.candidateTitle || !item.contentHash) return;
    if (
      !window.confirm(
        `将“${item.path}”的文档标题修复为“${item.candidateTitle}”？\n\n仅修改 YAML frontmatter 标题，正文不会改动。`,
      )
    ) {
      return;
    }
    setRepairingId(`title:${item.path}`);
    setError(null);
    try {
      await documentTitleRepair(
        item.path,
        item.contentHash,
        item.candidateTitle,
      );
      onRecovered();
      await refresh();
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : String(nextError),
      );
    } finally {
      setRepairingId(null);
    }
  };

  const restoreMissing = async (item: MissingDocumentRecoveryItem) => {
    if (
      !window.confirm(
        `从 ${item.createdAt} 的版本快照恢复遗失文档“${item.path}”？\n\n该操作只会在原路径仍不存在时原子创建 Markdown，绝不会覆盖同名文件。`,
      )
    ) {
      return;
    }
    setRepairingId(`missing:${item.path}`);
    setError(null);
    try {
      await documentRecoveryRestoreMissing(
        item.path,
        item.versionId,
        item.contentHash,
      );
      onRecovered();
      await refresh();
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : String(nextError),
      );
    } finally {
      setRepairingId(null);
    }
  };

  const restoreOrphan = async (item: OrphanedDocumentRecoveryItem) => {
    const targetPath = orphanTargets[item.objectHash]?.trim() ?? "";
    if (!targetPath) {
      setError("请先为孤立快照填写恢复路径。");
      return;
    }
    if (
      !window.confirm(
        `将无主快照 ${item.objectHash.slice(0, 12)}… 恢复为“${targetPath}”？\n\n该对象没有可靠的原始路径；仅当目标路径不存在时才会原子创建 Markdown。`,
      )
    ) {
      return;
    }
    setRepairingId(`orphan:${item.objectHash}`);
    setError(null);
    try {
      await documentRecoveryRestoreOrphan(item.objectHash, targetPath);
      onRecovered();
      await refresh();
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : String(nextError),
      );
    } finally {
      setRepairingId(null);
    }
  };

  const isEmpty =
    audit !== null &&
    audit.titleIssues.length === 0 &&
    audit.missingDocuments.length === 0 &&
    audit.orphanedDocuments.length === 0 &&
    audit.unavailableDocuments.length === 0;

  return (
    <IrisOverlay open={open} onClose={onClose} title="文档恢复" size="wide">
      <div className="space-y-4 p-4">
        <p className="text-xs leading-relaxed text-muted-foreground">
          此检查只读取 Markdown、索引、版本记录和 CAS
          对象。每项恢复均需确认；恢复写入使用原子创建，若目标文件已存在会安全失败，不会覆盖内容。
        </p>
        <div className="flex justify-end">
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={loading}
            onClick={() => void refresh()}
          >
            重新检查
          </Button>
        </div>
        {error ? (
          <p role="alert" className="text-xs text-destructive">
            检查或恢复失败：{error}
          </p>
        ) : null}
        {loading ? (
          <p className="text-sm text-muted-foreground">正在检查恢复来源…</p>
        ) : null}
        {isEmpty ? (
          <p className="text-sm text-muted-foreground">
            未发现需要恢复的文档或标题问题。
          </p>
        ) : null}

        {audit && audit.missingDocuments.length > 0 ? (
          <section className="space-y-2">
            <h3 className="text-sm font-medium">遗失文档：可从版本恢复</h3>
            {audit.missingDocuments.map((item) => (
              <article
                key={item.path}
                className="rounded-lg border border-border/65 p-3"
              >
                <p className="font-mono text-xs text-muted-foreground">
                  {item.path}
                </p>
                <p className="mt-1 text-sm">
                  候选标题：{item.candidateTitle ?? "（无 frontmatter 标题）"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  快照时间：{item.createdAt}
                </p>
                <pre className="mt-2 max-h-36 overflow-auto whitespace-pre-wrap rounded bg-muted/45 p-2 font-mono text-[11px] leading-relaxed">
                  {item.preview}
                </pre>
                <div className="mt-3">
                  <Button
                    type="button"
                    size="sm"
                    disabled={repairingId === `missing:${item.path}`}
                    onClick={() => void restoreMissing(item)}
                  >
                    {repairingId === `missing:${item.path}`
                      ? "正在恢复…"
                      : "恢复到原路径"}
                  </Button>
                </div>
              </article>
            ))}
          </section>
        ) : null}

        {audit && audit.orphanedDocuments.length > 0 ? (
          <section className="space-y-2">
            <h3 className="text-sm font-medium">孤立 CAS 文档快照</h3>
            <p className="text-xs text-muted-foreground">
              这些快照不再关联可靠原路径。请检查预览并指定一个新的 `.md` 路径。
            </p>
            {audit.orphanedDocuments.map((item) => (
              <article
                key={item.objectHash}
                className="rounded-lg border border-border/65 p-3"
              >
                <p className="font-mono text-xs text-muted-foreground">
                  {item.objectHash}
                </p>
                <p className="mt-1 text-sm">
                  候选标题：{item.candidateTitle ?? "（无 frontmatter 标题）"}
                </p>
                <pre className="mt-2 max-h-36 overflow-auto whitespace-pre-wrap rounded bg-muted/45 p-2 font-mono text-[11px] leading-relaxed">
                  {item.preview}
                </pre>
                <div className="mt-3 flex gap-2">
                  <Input
                    aria-label={`恢复路径 ${item.objectHash}`}
                    value={orphanTargets[item.objectHash] ?? ""}
                    onChange={(event) =>
                      setOrphanTargets((previous) => ({
                        ...previous,
                        [item.objectHash]: event.target.value,
                      }))
                    }
                  />
                  <Button
                    type="button"
                    size="sm"
                    disabled={repairingId === `orphan:${item.objectHash}`}
                    onClick={() => void restoreOrphan(item)}
                  >
                    {repairingId === `orphan:${item.objectHash}`
                      ? "正在恢复…"
                      : "恢复"}
                  </Button>
                </div>
              </article>
            ))}
          </section>
        ) : null}

        {audit && audit.titleIssues.length > 0 ? (
          <section className="space-y-2">
            <h3 className="text-sm font-medium">标题恢复</h3>
            {audit.titleIssues.map((item) => (
              <article
                key={item.path}
                className="rounded-lg border border-border/65 p-3"
              >
                <p className="font-mono text-xs text-muted-foreground">
                  {item.path}
                </p>
                <p className="mt-2 text-sm">
                  当前标题：{item.currentTitle || "（缺失）"}
                </p>
                <p className="mt-1 text-sm">
                  候选标题：{item.candidateTitle ?? "（无可靠候选）"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  来源：{candidateSourceLabel(item.candidateSource)}；原因：
                  {item.reason}
                </p>
                <div className="mt-3">
                  <Button
                    type="button"
                    size="sm"
                    disabled={
                      !item.candidateTitle ||
                      !item.contentHash ||
                      repairingId === `title:${item.path}`
                    }
                    onClick={() => void repairTitle(item)}
                  >
                    {repairingId === `title:${item.path}`
                      ? "正在修复…"
                      : "确认修复标题"}
                  </Button>
                </div>
              </article>
            ))}
          </section>
        ) : null}

        {audit && audit.unavailableDocuments.length > 0 ? (
          <section className="space-y-2">
            <h3 className="text-sm font-medium">未找到安全恢复来源</h3>
            {audit.unavailableDocuments.map((item) => (
              <article
                key={item.path}
                className="rounded-lg border border-border/65 p-3"
              >
                <p className="font-mono text-xs text-muted-foreground">
                  {item.path}
                </p>
                <p className="mt-1 text-sm">
                  索引标题：{item.currentTitle || "（缺失）"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  没有可验证的本地版本快照。请检查回收站或外部备份；永久丢弃后无法保证可恢复。
                </p>
              </article>
            ))}
          </section>
        ) : null}
      </div>
    </IrisOverlay>
  );
}
