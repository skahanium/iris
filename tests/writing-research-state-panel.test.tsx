import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ResearchStatePanel } from "@/components/ai/assistant/ResearchStatePanel";
import { WritingStatePanel } from "@/components/ai/assistant/WritingStatePanel";
import type { ResearchState, WritingState } from "@/types/ai";

const writingState: WritingState = {
  request_id: "req-writing",
  target_path: "Drafts/report.md",
  document_goal: "改写投委会备忘录引言",
  audience: "投委会",
  genre: "行业研究备忘录",
  structure_outline: ["背景", "判断", "风险"],
  key_arguments: ["证据 A", "证据 B"],
  material_packet_ids: ["ev-1"],
  citation_labels: ["S1"],
  style_constraints: ["克制表达", "证据驱动"],
  revision_records: [
    {
      patch_id: "patch-1",
      scope: "12..48",
      reason: "rewrite: 强化证据链",
      risk: "medium",
      rollback: "恢复到 base_content_hash=abc123",
      evidence_packet_ids: ["ev-1"],
    },
  ],
  draft_version_hash: "abc123",
};

const researchState: ResearchState = {
  request_id: "req-research",
  research_question: "AI agent 行业研究",
  sub_questions: ["需求是否增长"],
  sources: [
    {
      evidence_id: "ev-web",
      citation_label: "W1",
      source_type: "web",
      title: "行业报告",
      credibility: "medium",
      freshness: "needs_check",
      score: 0.82,
    },
  ],
  credibility_summary: "1 sources, 0 high credibility",
  freshness_summary: "1 sources need freshness check",
  conflicts: ["存在冲突证据"],
  counter_arguments: ["反方解释仍需保留"],
  evidence_gaps: ["缺少一手收入数据"],
  preliminary_conclusions: [
    {
      statement: "市场增长但商业化节奏需验证",
      evidence_item_ids: ["ev-web"],
      boundary: "需验证样本范围",
      inference: false,
    },
  ],
};

describe("writing and research state panels", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("renders writing state summary without note body", async () => {
    await act(async () => {
      root.render(<WritingStatePanel state={writingState} />);
    });

    expect(document.body.textContent).toContain("文稿状态");
    expect(document.body.textContent).toContain("改写投委会备忘录引言");
    expect(document.body.textContent).toContain("克制表达");
    expect(document.body.textContent).toContain("rewrite: 强化证据链");
    expect(document.body.textContent).toContain(
      "恢复到 base_content_hash=abc123",
    );
    expect(document.body.textContent).not.toContain("full_content");
    expect(document.body.textContent).not.toContain("noteContent");
  });

  it("renders research state summary with evidence boundaries", async () => {
    await act(async () => {
      root.render(<ResearchStatePanel state={researchState} />);
    });

    expect(document.body.textContent).toContain("研究状态");
    expect(document.body.textContent).toContain("AI agent 行业研究");
    expect(document.body.textContent).toContain("暂无高可信证据");
    expect(document.body.textContent).toContain("证据新鲜度需核验");
    expect(document.body.textContent).toContain("存在冲突证据");
    expect(document.body.textContent).toContain("需验证样本范围");
    expect(document.body.textContent).not.toContain("0 sources");
    expect(document.body.textContent).not.toContain("freshness check");
    expect(document.body.textContent).not.toContain("needs_check");
    expect(document.body.textContent).not.toContain("raw_web_page");
    expect(document.body.textContent).not.toContain("full_note_content");
  });
});
