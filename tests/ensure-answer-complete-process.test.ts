import { describe, expect, it } from "vitest";

import {
  ANSWER_COMPLETE_PROCESS_ID,
  ANSWER_COMPLETE_PROCESS_LABEL,
} from "@/lib/assistant-presentation";
import { ensureTerminalAnswerComplete } from "@/lib/ensure-answer-complete-process";
import type { AssistantProcessItem } from "@/lib/assistant-process";

const generating: AssistantProcessItem = {
  id: "stage:3",
  kind: "stage",
  label: "正在生成答复",
  status: "completed",
  createdAt: 3,
};

describe("ensureTerminalAnswerComplete", () => {
  it("run completed 且缺完成项时追加答复完毕", () => {
    const result = ensureTerminalAnswerComplete([generating], "completed");
    expect(result.at(-1)).toMatchObject({
      id: ANSWER_COMPLETE_PROCESS_ID,
      label: ANSWER_COMPLETE_PROCESS_LABEL,
      status: "completed",
    });
  });

  it("已有 ANSWER_COMPLETE_PROCESS_ID 时不重复追加", () => {
    const withComplete: AssistantProcessItem[] = [
      generating,
      {
        id: ANSWER_COMPLETE_PROCESS_ID,
        kind: "stage",
        label: ANSWER_COMPLETE_PROCESS_LABEL,
        status: "completed",
        createdAt: 4,
      },
    ];
    expect(ensureTerminalAnswerComplete(withComplete, "completed")).toEqual(
      withComplete,
    );
  });

  it("已有答复完毕标签时不重复追加", () => {
    const withLabel: AssistantProcessItem[] = [
      generating,
      {
        id: "stage:custom",
        kind: "stage",
        label: ANSWER_COMPLETE_PROCESS_LABEL,
        status: "completed",
        createdAt: 4,
      },
    ];
    expect(ensureTerminalAnswerComplete(withLabel, "completed")).toEqual(
      withLabel,
    );
  });

  it("failed 与 cancelled 不追加答复完毕", () => {
    expect(ensureTerminalAnswerComplete([generating], "failed")).toEqual([
      generating,
    ]);
    expect(ensureTerminalAnswerComplete([generating], "cancelled")).toEqual([
      generating,
    ]);
  });

  it("running 或 undefined runState 不追加", () => {
    expect(ensureTerminalAnswerComplete([generating], "running")).toEqual([
      generating,
    ]);
    expect(ensureTerminalAnswerComplete([generating], undefined)).toEqual([
      generating,
    ]);
  });

  it("undefined items 在 completed 时返回仅含答复完毕的列表", () => {
    const result = ensureTerminalAnswerComplete(undefined, "completed");
    expect(result).toHaveLength(1);
    expect(result[0]?.label).toBe(ANSWER_COMPLETE_PROCESS_LABEL);
  });
});
