import { isTauriRuntime } from "@/lib/tauri-runtime";

import { DesktopTitleBar } from "./DesktopTitleBar";

/** 无笔记库 / 加载时的单行顶栏（与文档态共用 DesktopTitleBar） */
export function MinimalWindowChrome() {
  if (!isTauriRuntime()) return null;

  return (
    <DesktopTitleBar
      variant="splash"
      tabs={[]}
      activePath={null}
      onSelect={() => {}}
      onClose={() => {}}
      onNew={() => {}}
    />
  );
}
