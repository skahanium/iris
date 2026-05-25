interface StatusBarProps {
  path: string | null;
  wordCount: number;
  aiStatus: string;
}

export function StatusBar({ path, wordCount, aiStatus }: StatusBarProps) {
  return (
    <footer className="flex h-7 items-center gap-4 border-t border-border bg-panel/95 px-3 font-sans text-xs text-muted-foreground">
      <span className="truncate">{path ?? "未打开文件"}</span>
      <span>{wordCount.toLocaleString()} 字</span>
      <span className="ml-auto">{aiStatus}</span>
    </footer>
  );
}
