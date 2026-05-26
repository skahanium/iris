interface StatusBarProps {
  path: string | null;
  /** User-facing document name (`files.title`). */
  documentTitle?: string | null;
  wordCount: number;
  aiStatus: string;
}

export function StatusBar({
  path,
  documentTitle,
  wordCount,
  aiStatus,
}: StatusBarProps) {
  const label = documentTitle ?? path ?? "未打开文件";
  return (
    <footer className="flex h-8 shrink-0 items-center gap-4 border-t border-border bg-panel px-3 font-sans text-xs text-muted-foreground">
      <span className="min-w-0 truncate" title={path ?? undefined}>
        {label}
      </span>
      <span className="shrink-0 tabular-nums">
        {wordCount.toLocaleString()} 字
      </span>
      <span className="ml-auto shrink-0 truncate">{aiStatus}</span>
    </footer>
  );
}
