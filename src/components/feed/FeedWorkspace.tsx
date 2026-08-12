//! 订阅工作区（Task 4.2 骨架；Task 4.3 扩展为响应式三区布局）。
//!
//! 作为 AppShell 的 `feedWorkspace` 挂载，与编辑器 main 只切换可见性。

import { useFeedLibrary } from "@/hooks/useFeedLibrary";

export function FeedWorkspace() {
  const library = useFeedLibrary();
  return (
    <div
      data-testid="feed-workspace"
      className="flex h-full min-h-0 flex-1 flex-col"
    >
      <div className="flex-1 p-4 text-muted-foreground">
        订阅（{library.items.length} 条）
      </div>
    </div>
  );
}
