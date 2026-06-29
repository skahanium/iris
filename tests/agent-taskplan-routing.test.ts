import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  buildAssistantTaskPlan,
  shouldAttachNoteContextToTaskPlan,
} from "@/lib/assistant-taskplan";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

interface AssistantRouteInput {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  explicitScope: boolean;
}

interface IntentDetectionResult {
  detectedIntent: string;
}

interface AssistantRoutingModule {
  detectAgentIntent(input: AssistantRouteInput): IntentDetectionResult;
}

async function loadRouting(): Promise<AssistantRoutingModule> {
  const spec = "../src/lib/assistant-routing";
  return (await import(/* @vite-ignore */ spec)) as AssistantRoutingModule;
}

describe("assistant TaskPlan routing contract", () => {
  it("creates a per-turn TaskPlan instead of locking a conversation scene", () => {
    const taskplan = read("src/lib/assistant-taskplan.ts");

    expect(taskplan).toContain("buildAssistantTaskPlan");
    expect(taskplan).toContain("contextReferences");
    expect(taskplan).toContain("retrievalMode");
    expect(taskplan).toContain("executionMode");
    expect(taskplan).toContain("artifactPlan");
  });

  it("keeps novel continuation with analysis words on the writer path", () => {
    const taskplan = read("src/lib/assistant-taskplan.ts");

    expect(taskplan).toContain("creative_write");
    expect(taskplan).toContain("requiresClarification");
    expect(taskplan).toContain("writingKeywordBeforeResearchKeyword");
  });

  it("keeps legacy routing as an adapter, not the primary decision system", () => {
    const routing = read("src/lib/assistant-routing.ts");

    expect(routing).toContain("buildAssistantTaskPlan");
  });

  it("removes the legacy research keyword priority from routing", () => {
    const routing = read("src/lib/assistant-routing.ts");

    expect(routing).not.toContain("const RESEARCH_KEYWORDS");
    expect(routing).not.toContain("includesAny(message, RESEARCH_KEYWORDS)");
  });

  it("does not detect fiction continuation as research just because it says 分析 or 研究", async () => {
    const routing = await loadRouting();
    const result = routing.detectAgentIntent({
      message:
        "根据以上文字写出第四章，要求描写更火爆、剧情更诱人，同时分析人物心理",
      hasSelection: true,
      notePath: "/novel.md",
      explicitScope: false,
    });

    expect(result.detectedIntent).not.toBe("research");
    expect(["rewrite_selection", "write"]).toContain(result.detectedIntent);
  });

  it("keeps explicit research questions on the research path", async () => {
    const routing = await loadRouting();
    const result = routing.detectAgentIntent({
      message: "研究一下 sqlite-vec 和 FTS5 在本地知识库中的取舍",
      hasSelection: false,
      notePath: null,
      explicitScope: true,
    });

    expect(result.detectedIntent).toBe("research");
  });

  it("routes selected-text opinion questions to notes instead of citation checks", async () => {
    const routing = await loadRouting();
    const result = routing.detectAgentIntent({
      message:
        "这个思路是不是过于浅薄了？万一该商人是血站的供应商呢？问题不是更严重吗？",
      hasSelection: true,
      notePath: "/notes/work.md",
      explicitScope: false,
    });

    expect(result.detectedIntent).not.toBe("citation_check");
    expect(result.detectedIntent).toBe("ask_notes");
  });

  it("keeps explicit citation evidence checks on the citation path", async () => {
    const routing = await loadRouting();
    const result = routing.detectAgentIntent({
      message: "帮我核查这段引用证据是否充分，有没有可靠出处支撑",
      hasSelection: true,
      notePath: "/notes/work.md",
      explicitScope: false,
    });

    expect(result.detectedIntent).toBe("citation_check");
  });

  it("routes note insertion requests to writing when a note is open", () => {
    const plan = buildAssistantTaskPlan({
      message: "请补充到当前标题下方",
      hasSelection: false,
      notePath: "/notes/work.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: false,
    });

    expect(plan.intent).toBe("creative_write");
    expect(plan.modelSlot).toBe("writer");
    expect(plan.executionMode).toBe("writing_candidate");
    expect(plan.sourceHints).toContain("context:note");
  });

  it("does not treat bare confirmation text as writing without a pending proposal", () => {
    const plan = buildAssistantTaskPlan({
      message: "我确认",
      hasSelection: false,
      notePath: "/notes/work.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: false,
    });

    expect(plan.intent).not.toBe("creative_write");
    expect(plan.intent).not.toBe("rewrite_selection");
  });

  it("recognizes confirmation text only when a writing proposal is pending", () => {
    const plan = buildAssistantTaskPlan({
      message: "按此修改",
      hasSelection: false,
      notePath: "/notes/work.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: false,
      hasPendingWriteProposal: true,
    });

    expect(plan.intent).toBe("rewrite_selection");
    expect(plan.outputMode).toBe("confirmation_required");
    expect(plan.sourceHints).toContain("context:pending_write_proposal");
  });
});
describe("regression: ordinary chat must not be promoted by open note or fresh words", () => {
  it("keeps a greeting in chat even when a note is open", () => {
    const plan = buildAssistantTaskPlan({
      message: "你好？",
      hasSelection: false,
      notePath: "/notes/empty.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: false,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.retrievalMode).toBe("none");
    expect(plan.executionMode).toBe("direct_answer");
    expect(plan.artifactPlan).toEqual([]);
    expect(shouldAttachNoteContextToTaskPlan(plan)).toBe(false);
  });

  it("keeps public fact lookup in chat while preserving brokered web access", () => {
    const plan = buildAssistantTaskPlan({
      message: "冯小刚的出生年月？",
      hasSelection: false,
      notePath: null,
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.webMode).toBe("brokered");
    expect(plan.executionMode).toBe("direct_answer");
    expect(plan.outputMode).toBe("markdown_message");
    expect(plan.artifactPlan).toEqual([]);
  });

  it("keeps a simple date question out of research even with web enabled", () => {
    const plan = buildAssistantTaskPlan({
      message: "今天是几月几日？",
      hasSelection: false,
      notePath: "/notes/empty.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.modelSlot).toBe("fast");
    expect(plan.executionMode).toBe("direct_answer");
    expect(plan.outputMode).toBe("markdown_message");
    expect(plan.artifactPlan).toEqual([]);
  });

  it("answers a legal basis question directly unless the user asks for research output", () => {
    const plan = buildAssistantTaskPlan({
      message:
        "在我国，国家公职人员或者领导干部关于兼职取酬、兼职兼薪方面的规定是怎么样的？依据是什么？",
      hasSelection: false,
      notePath: "/notes/empty.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.executionMode).toBe("direct_answer");
    expect(plan.outputMode).toBe("markdown_message");
    expect(plan.artifactPlan).toEqual([]);
  });

  it("uses note QA only for explicit current-note references", () => {
    const plan = buildAssistantTaskPlan({
      message: "请根据当前笔记回答这个问题",
      hasSelection: false,
      notePath: "/notes/empty.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: false,
    });

    expect(plan.intent).toBe("ask_notes");
    expect(plan.executionMode).toBe("context_answer");
    expect(shouldAttachNoteContextToTaskPlan(plan)).toBe(true);
  });

  it("keeps follow-up permission to search in chat with brokered web access", () => {
    const plan = buildAssistantTaskPlan({
      message: "可以，去补充吧",
      hasSelection: false,
      notePath: null,
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.webMode).toBe("brokered");
    expect(plan.executionMode).toBe("direct_answer");
    expect(plan.artifactPlan).toEqual([]);
  });

  it("promotes only explicit multi-source research requests to research artifacts", () => {
    const plan = buildAssistantTaskPlan({
      message: "请联网调研这个主题，形成研究综述并对比多来源证据",
      hasSelection: false,
      notePath: "/notes/empty.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("research");
    expect(plan.executionMode).toBe("structured_task");
    expect(plan.outputMode).toBe("artifact_backed_message");
    expect(plan.artifactPlan).toEqual([
      expect.objectContaining({ kind: "evidence_sources" }),
    ]);
  });
});
describe("backend compatibility routing fallback", () => {
  it("does not promote legacy requests only because a note is open or sources are requested", () => {
    const src = read("src-tauri/src/commands/ai_commands.rs");
    const fnBody =
      src
        .split("fn infer_agent_intent_for_new_request")[1]
        ?.split("fn derive_task_policy_for_new_request")[0] ?? "";

    expect(fnBody).not.toContain("note_path.is_some()");
    expect(fnBody).not.toContain('"依据"');
  });
});
