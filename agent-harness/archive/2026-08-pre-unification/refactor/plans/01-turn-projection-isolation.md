# 跨 Run 回答投影隔离 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新 Run 从同步首帧到首个本轮 answer delta 之间只显示本轮过程状态，绝不显示上一 Run 正文。

**Architecture:** 保留 `useAssistantRun`、`AssistantPresentationState` 和 `useAssistantConversationProjection` 的单一生产链。让 reveal 值显式携带 `runId`，投影层按身份消费；Run 切换时同步隔离旧 frame/event，终态正文仍以同 Run 持久化事实恢复。

**Tech Stack:** React 19、TypeScript、Vitest、现有 Assistant Run/presentation hooks。

**Status:** Completed；对应 `UI-003`。测试已通过并更新附录 A、B；未写入 `ARCHITECTURE.md` 作为已实现事实。

**Dependencies:** 无代码前置依赖；应先于当前事实能力施工，以便固定用户可见的 Run 内容边界。

## Global Constraints

- 不创建 worktree，不新增依赖，不新增消息列表或第二套 presentation 状态。
- 修改前完整阅读目标文件及所有调用方；IPC 和 Rust 后端不在本计划范围。
- 每项先写失败测试，确认失败原因与 UI-003 一致，再修改实现。
- 只运行列出的前端定向测试；本计划收口时运行 lint、format check 和 typecheck。
- Commit 使用中文 Conventional Commit。

---

### Task 1: 用组合投影测试复现旧答案串入新 Run

**Files:**

- Modify: `tests/use-assistant-run-transcript.test.tsx`
- Read: `tests/use-assistant-answer-reveal.test.tsx`
- Read: `src/components/ai/hooks/useAssistantAnswerReveal.ts`
- Read: `src/components/ai/hooks/useAssistantConversationProjection.ts`

**Interfaces:**

- Consumes: `AssistantPresentationState`、`replayAssistantRunEvents`、两个生产 hook。
- Produces: `new_run_never_projects_previous_reveal_answer` 回归测试，覆盖同一次 React tree 中的 reveal → projection 数据流。

- [x] **Step 1: 新增组合 Harness**

在测试中新增真实组合，而不是把 `presentationAnswer` 手工传值：

```tsx
function RevealProjectionProbe({
  run,
  presentation,
}: {
  run: ReturnType<typeof replayAssistantRunEvents>;
  presentation: AssistantPresentationState;
}) {
  const reveal = useAssistantAnswerReveal(presentation);
  useAssistantConversationProjection({
    run,
    presentation,
    presentationAnswer: reveal.answer,
    presentationRevealing: reveal.revealing,
    messages,
    setMessages: (updater) => {
      messages = typeof updater === "function" ? updater(messages) : updater;
    },
    setStreaming: (next) => {
      streaming = next;
    },
    setActivityHint: () => undefined,
    setError: () => undefined,
  });
  return null;
}
```

这里先使用当前生产接口，确保失败来自真实跨 hook 行为，而不是目标类型尚未定义。Task 3 再把 Harness 和生产调用一起迁移到 `presentationReveal`。

- [x] **Step 2: 增加两轮切换断言**

测试顺序固定为：

```tsx
messages = [
  { role: "assistant", content: "上一轮完整回答", runId: "run-old" },
  { role: "user", content: "新问题", runId: "run-new" },
  { role: "assistant", content: "", runId: "run-new" },
];

// 先让 run-old reveal 完整排空，再同步 render run-new accepted + empty presentation。
// 断言 run-old 保留旧正文，run-new 始终为空。
expect(messages.find((item) => item.runId === "run-old")?.content).toBe(
  "上一轮完整回答",
);
expect(messages.find((item) => item.runId === "run-new")?.content).toBe("");
```

- [x] **Step 3: 运行测试确认失败**

Run:

```bash
npm run test -- tests/use-assistant-run-transcript.test.tsx
```

Expected: FAIL，`run-new` 的 content 为“上一轮完整回答”；不能接受编译错误或无关环境失败。

### Task 2: 让 reveal 同步按 Run 失败关闭

**Files:**

- Modify: `src/components/ai/hooks/useAssistantAnswerReveal.ts`
- Modify: `tests/use-assistant-answer-reveal.test.tsx`

**Interfaces:**

- Consumes: `AssistantPresentationState.runId/resetEpoch/answer`。
- Produces:

```ts
export interface AssistantAnswerReveal {
  runId: string | null;
  answer: string;
  revealing: boolean;
}
```

- [x] **Step 1: 扩展单 hook 测试**

让测试 Harness 把 `runId` 写入 `data-run-id`，并断言切换到 `run-new` 的同一 commit：

```tsx
expect(output?.getAttribute("data-run-id")).toBe("run-new");
expect(output?.getAttribute("data-answer")).toBe("");
```

- [x] **Step 2: 实现 render 阶段身份门**

保留 effect 中的清理，但返回值不能等待 effect：

```ts
const answerBelongsToRun = runIdRef.current === runId;
const visibleAnswer = answerBelongsToRun ? answer : "";

return {
  runId,
  answer: visibleAnswer,
  revealing:
    runId !== null &&
    (!answerBelongsToRun || visibleAnswer.length < target.length),
};
```

`runId=null` 时返回空 answer 和 `revealing=false`。resetEpoch 变化仍由现有 effect 清理，不能破坏 surrogate pair、reduced motion 和 reasoning markup 测试。

- [x] **Step 3: 运行 reveal 定向测试**

Run:

```bash
npm run test -- tests/use-assistant-answer-reveal.test.tsx
```

Expected: PASS。

### Task 3: 投影层只消费同 Run reveal，并移除活动空答案回退

**Files:**

- Modify: `src/components/ai/hooks/useAssistantConversationProjection.ts`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Modify: `tests/use-assistant-run-transcript.test.tsx`

**Interfaces:**

- Consumes: Task 2 的 `AssistantAnswerReveal`。
- Produces: `AssistantConversationProjectionOptions.presentationReveal?: AssistantAnswerReveal`；删除 `presentationAnswer` 和 `presentationRevealing` 两个松散参数。

- [x] **Step 1: 收紧 options 和生产调用**

```ts
export interface AssistantConversationProjectionOptions {
  run: AssistantRunEventState | null;
  presentation?: AssistantPresentationState | null;
  presentationReveal?: AssistantAnswerReveal;
  // 其余字段保持不变
}
```

`UnifiedAssistantPanel` 传入完整对象：

```tsx
useAssistantConversationProjection({
  run: assistantRun.eventState,
  presentation: assistantRun.presentationState,
  presentationReveal: assistantAnswerReveal,
  // 其余参数保持不变
});
```

- [x] **Step 2: 计算同 Run 可见内容**

```ts
const revealMatchesRun =
  presentationReveal?.runId === run.runId && presentation?.runId === run.runId;
const visiblePresentationAnswer = revealMatchesRun
  ? sanitizeAssistantVisibleText(presentationReveal.answer)
  : "";
```

projection key 必须包含 `presentationReveal.runId`，不能只使用 answer 长度。

- [x] **Step 3: 删除活动 presentation 的旧正文回退**

内容选择固定为：

```ts
const content = presentationOwnsContent
  ? visiblePresentationAnswer
  : run.content.trim()
    ? run.content
    : (currentMessage?.content ?? "");
```

持久化 `currentMessage.content` 回退只留在 presentation 不拥有内容的终态/恢复分支。

- [x] **Step 4: 补充同 Run 终态恢复负例**

新增 `terminal_recovery_uses_only_its_own_persisted_answer`：同时放置两个 Run 的助手消息，让 `run-new` 的终态恢复只能使用 `run-new` 内容。

- [x] **Step 5: 运行投影测试**

Run:

```bash
npm run test -- tests/use-assistant-run-transcript.test.tsx tests/use-assistant-answer-reveal.test.tsx
```

Expected: PASS。

### Task 4: 清理切换时排队的上一 Run presentation 工作

**Files:**

- Modify: `src/hooks/useAssistantRun.ts`
- Modify: `tests/use-assistant-run.test.tsx`

**Interfaces:**

- Consumes: `presentationFrameRef`、`pendingPresentationEventsRef`、`activeRunIdRef`、`earlyPresentationRef`。
- Produces: `activateAccepted` 在建立新 Run 前同步清理上一 Run 待 flush 工作。

- [x] **Step 1: 添加迟到 frame 回归测试**

新增 `queued_previous_run_frame_cannot_patch_new_run`：保存上一 Run 的 `requestAnimationFrame` callback，接受新 Run 后再手工执行旧 callback，断言新 presentation 的 `runId`、`answer` 和 `lastSeq` 不变。

- [x] **Step 2: 实现切换清理**

在 `activateAccepted` 的最前部、写入新 `activeRunIdRef` 之前执行：

```ts
if (presentationFrameRef.current !== null) {
  window.cancelAnimationFrame(presentationFrameRef.current);
  presentationFrameRef.current = null;
}
pendingPresentationEventsRef.current = [];
```

之后再设置新 active run，并只回放 `earlyPresentationRef` 中 key 等于新 runId 的事件。不能清空其他尚未激活 Run 的早到事件 map。

- [x] **Step 3: 运行 Run hook 测试**

Run:

```bash
npm run test -- tests/use-assistant-run.test.tsx
```

Expected: PASS。

### Task 5: 阶段验证与提交

**Files:**

- Modify after tests pass: `refactor/appendices/A-current-state-audit.md`
- Modify after tests pass: `refactor/appendices/B-issue-test-traceability.md`

**Interfaces:**

- Consumes: Tasks 1–4 的真实测试结果。
- Produces: UI-003 从 `Confirmed` 变为 `Resolved`，目标测试移入实证表。

- [x] **Step 1: 运行受影响质量检查**

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test -- tests/use-assistant-answer-reveal.test.tsx tests/use-assistant-run-transcript.test.tsx tests/use-assistant-run.test.tsx
```

Expected: 全部 exit 0。

- [x] **Step 2: 更新追踪状态**

只有上述命令通过后，更新附录 A/B；保留测试的精确名称和证明边界。

- [x] **Step 3: 提交**

```bash
git add src/components/ai/hooks/useAssistantAnswerReveal.ts src/components/ai/hooks/useAssistantConversationProjection.ts src/components/ai/UnifiedAssistantPanel.impl.tsx src/hooks/useAssistantRun.ts tests/use-assistant-answer-reveal.test.tsx tests/use-assistant-run-transcript.test.tsx tests/use-assistant-run.test.tsx refactor/appendices/A-current-state-audit.md refactor/appendices/B-issue-test-traceability.md
git commit -m "fix(ui): 按 Run 隔离回答揭示与消息投影"
```
