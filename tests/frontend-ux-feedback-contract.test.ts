import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("frontend UX feedback contract", () => {
  it("shows explicit empty search results and global copy feedback", () => {
    const search = read("src/components/file/SearchPanel.tsx");
    const messages = read("src/components/ai/AiMessageList.tsx");
    const toast = read("src/components/ui/toast.tsx");
    const useToast = read("src/components/ui/use-toast.ts");
    const main = read("src/main.tsx");
    const artifacts = read("src/components/ai/hooks/useAssistantArtifacts.ts");

    expect(search).toContain("未找到匹配结果");
    expect(search).toContain("试试更具体的关键词，或切换语义搜索。");
    expect(toast).toContain("ToastProvider");
    expect(useToast).toContain("useToast");
    expect(main).toContain("<ToastProvider>");
    expect(messages).toContain("useToast");
    expect(messages).toContain("已复制回答");
    expect(messages).toContain("复制失败");
    expect(artifacts).toContain("useToast");
    expect(artifacts).not.toContain("ignore clipboard failures");
  });

  it("uses skeleton loading surfaces instead of plain loading text", () => {
    const vault = read("src/components/file/VaultNavigator.tsx");
    const knowledge = read(
      "src/components/knowledge/KnowledgeRelationsPanel.tsx",
    );
    const graph = read("src/components/graph/GraphView.tsx");

    expect(vault).toContain("VaultNavigatorLoadingSkeleton");
    expect(knowledge).toContain("KnowledgeRelationsLoadingSkeleton");
    expect(graph).toContain("GraphLoadingSkeleton");
  });
});
