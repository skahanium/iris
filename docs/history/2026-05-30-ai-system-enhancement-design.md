# AI 体系全面增强设计

> **日期**: 2026-05-30
> **状态**: Approved
> **交付方式**: 按独立 PR 分批，每个模块一个 PR

---

## 1. 架构方向

采用 **方案 B（Harness 内嵌扩展）**：

- 保留现有 `run_harness` 主循环不动
- Sub-agent 作为工具 `spawn_subagent` 注入，harness 内部可递归启动子实例
- Session、Skills、Prompt Profile 等作为独立模块增量添加
- 每个 feature 独立可测试，互不阻塞

DeepSeek V4 工具调用策略：**优先原生 function calling，保留 ReAct fallback 兼容**。

---

## 2. 模块设计

### 2.1 历史会话管理（PR1）

#### 2.1.1 后端

**数据库变更（migration）：**

```sql
ALTER TABLE sessions ADD COLUMN title TEXT;
```

**SessionManager 新增方法（`session.rs`）：**

```rust
/// 列出会话摘要（支持按 scene/note_path 筛选）。
pub fn list_sessions(
    db: &Database,
    scene: Option<&str>,
    note_path: Option<&str>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<SessionSummary>>;

/// 按 ID 删除会话（级联删除消息）。
pub fn delete_session(db: &Database, session_id: i64) -> AppResult<bool>;

/// 重命名会话标题。
pub fn rename_session(db: &Database, session_id: i64, new_title: &str) -> AppResult<()>;
```

`SessionSummary` 结构：

```rust
pub struct SessionSummary {
    pub id: i64,
    pub title: String,        // 取第一条 user message 前 40 字或手动设置的标题
    pub scene: String,
    pub note_path: Option<String>,
    pub message_count: u32,
    pub created_at: String,
    pub updated_at: String,
}
```

**Tauri commands（`ai_commands.rs`）：**

- `session_list { scene?: string, note_path?: string, limit?: u32, offset?: u32 }` → `Vec<SessionSummary>`
- `session_delete { session_id: i64 }` → `bool`
- `session_rename { session_id: i64, title: string }` → `()`
- `session_load { session_id: i64, limit?: u32 }` → `Vec<SessionMessage>`

#### 2.1.2 前端

**新组件 `src/components/ai/SessionHistoryDropdown.tsx`：**

- 位于 `UnifiedAssistantPanel` header 的"新对话"按钮旁
- 触发：点击 history icon 打开 Popover
- 内容：
  - 会话列表（按 updated_at 降序）
  - 每条：title 截断 + 相对时间 + inline rename（Edit icon → input）+ delete（Trash icon + 确认）
  - 底部："清空所有历史" 操作
- 交互：
  - 点击某条 → 调用 `session_load` → 加载消息到面板 → 设置 sessionId
  - 删除 → 确认 dialog → 调用 `session_delete`
  - 重命名 → inline input blur/enter → 调用 `session_rename`

**IPC 类型补充（`src/types/ipc.ts`, `src/lib/ipc.ts`）：**

- `SessionSummary` 类型
- `sessionList()`, `sessionDelete()`, `sessionRename()`, `sessionLoad()` 函数

---

### 2.2 Markdown 渲染增强（PR2）

#### 2.2.1 问题与根因

| 症状                 | 根因                                                          |
| -------------------- | ------------------------------------------------------------- |
| `**加粗**` 显示星号  | `linkifyAiCitations()` 在 markdown 源码级替换，破坏了语法边界 |
| Citation `[C0]` 乱码 | 正则对中文书名号格式匹配不稳定                                |
| 嵌套列表缩进失败     | `breaks: true` 对列表内文本的换行处理有歧义                   |
| 流式中途格式乱       | `repairStreamingMarkdown` 只修复 fence，不修复行内标记        |
| 表格溢出             | `.ai-table-wrap` 缺少 `overflow-x: auto`                      |

#### 2.2.2 管线重构

**当前管线：**

```
content → linkifyAiCitations → parseMarkdownToHtml → tagCitationLinksInHtml → sanitize
```

**新管线：**

```
content → repairStreamingMarkdown → parseMarkdownToHtml → postProcessCitations(html) → sanitize
```

关键改动：**citation 处理从 pre-markdown 移到 post-HTML**。

#### 2.2.3 具体修复

1. **`repairStreamingMarkdown` 扩展**：检测未闭合的 `**`/`*`/`~~`/`` ` `` 并临时补闭合
2. **`postProcessCitations(html)`**：在 HTML 中匹配 `[Cn]` 和中文书名号引用格式，替换为 `<a class="ai-citation">`
3. **嵌套列表**：在 marked 配置中对列表项内禁用 `breaks` 语义（通过 renderer override）
4. **CSS 补充**：`.ai-table-wrap { overflow-x: auto; max-width: 100%; }`
5. **样式统一**：抽取 `.iris-prose` 共享类，确保 AI 面板与编辑区代码块/表格/列表风格一致

#### 2.2.4 涉及文件

- `src/lib/markdown-render.ts` — 主渲染管线
- `src/lib/ai/citation-markdown.ts` — 重写为 post-HTML 处理器
- `src/components/ui/ai-message.tsx` — 调用链调整
- `src/styles/globals.css` — 统一 prose 样式

---

### 2.3 Harness 进阶工程（PR3）

#### 2.3.1 工具结果流式推送

扩展 `emit_trace`，在 `tool_complete` phase 推送 output 摘要（截断 200 字符）：

```rust
pub struct HarnessTraceEvent {
    pub request_id: String,
    pub round: u32,
    pub phase: HarnessPhase,    // 新增枚举
    pub tool_name: String,
    pub status: String,
    pub message: Option<String>,
    pub output_preview: Option<String>,  // 新增：工具输出摘要
}

pub enum HarnessPhase {
    ToolStart,
    ToolComplete,
    SubagentSpawn,
    SubagentComplete,
    Reflection,
    FinalStream,
}
```

#### 2.3.2 工具重试与降级

```rust
async fn dispatch_with_retry(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolCallResult {
    let result = dispatch_tool(state, ctx, tool_name, args).await;
    if !result.success && is_retryable(tool_name, &result) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        return dispatch_tool(state, ctx, tool_name, args).await;
    }
    if !result.success && tool_name == "search_hybrid" {
        // 降级为 keyword search
        return dispatch_tool(state, ctx, "search_keyword", args).await;
    }
    result
}
```

可重试条件：`web_search` 超时/网络错误。

#### 2.3.3 证据去重压缩

新函数 `compact_evidence`（放入 `evidence_mixer.rs`）：

```rust
pub fn compact_evidence(
    packets: &mut Vec<ContextPacket>,
    token_budget: usize,
) {
    packets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut tokens_used = 0;
    for packet in packets.iter_mut() {
        let est = estimate_tokens(&packet.excerpt);
        if tokens_used + est > token_budget {
            // 低分包：只保留元数据
            packet.excerpt = format!("[已压缩] {}", &packet.title);
        }
        tokens_used += est.min(50); // 压缩后按 50 token 计
    }
}
```

#### 2.3.4 Self-Critique 反思步骤

工具循环结束后、final streaming 前插入一轮反思：

```rust
// 在最终 streaming 前追加
messages.push(LlmMessage {
    role: MessageRole::System,
    content: "请审视上面的检索结果和你的推理。如果证据不充分无法准确回答用户问题，\
              请调用工具补充检索。如果证据充分，请直接回答。".into(),
    ..Default::default()
});
let reflection_response = gateway.send_request(reflection_request).await?;
// 如果 reflection 产生了新的 tool_calls → 再执行一轮
// 否则进入 final streaming
```

#### 2.3.5 Think-Act-Observe 可视化

检测 DeepSeek V4 的 `reasoning_content` 字段或 `<thinking>` 标签：

```rust
if let Some(reasoning) = response.reasoning_content {
    emit_trace(app_handle, &input.request_id, round, "thinking", "reasoning")?;
    // 通过事件推送前端，前端渲染为折叠卡片
    app_handle.emit("ai:thinking", &serde_json::json!({
        "request_id": input.request_id,
        "round": round,
        "content": reasoning,
    }))?;
}
```

#### 2.3.6 滑动窗口摘要压缩

```rust
fn compress_history(messages: &[(String, String)], max_full: usize) -> Vec<(String, String)> {
    if messages.len() <= max_full {
        return messages.to_vec();
    }
    let old = &messages[..messages.len() - max_full];
    let recent = &messages[messages.len() - max_full..];

    let summary = summarize_messages(old); // 规则压缩：取每条前 80 字符拼接
    let mut result = vec![("system".to_string(), format!("[历史摘要] {summary}"))];
    result.extend(recent.to_vec());
    result
}
```

#### 2.3.7 自适应轮次控制

注册虚拟工具 `conclude_reasoning`：

```rust
ToolSpec {
    name: "conclude_reasoning",
    description: "当你认为已收集到足够信息可以回答用户问题时调用此工具，结束工具循环直接生成回答。",
    input_schema: json!({"type": "object", "properties": {}}),
    access_level: ToolAccessLevel::ReadIndex,
    scene_allowlist: vec![/* all scenes */],
    requires_confirmation: false,
}
```

harness 循环中检测到 `conclude_reasoning` 调用时 `break` 进入 final response。

#### 2.3.8 中间状态 Checkpoint

```rust
// 每轮结束时
TraceRecorder::save_checkpoint(&state.db, &input.request_id, &CheckpointData {
    round: harness_rounds,
    messages: messages.clone(),
    tool_calls: all_tool_calls.clone(),
    tool_results: tool_results_json.clone(),
    evidence_packets: evidence_packets.clone(),
    usage: total_usage.clone(),
})?;
```

`ai_traces` 表新增 `checkpoint BLOB` 列（序列化的 JSON）。

---

### 2.4 Sub-agent 能力（PR4）

#### 2.4.1 工具定义

```rust
ToolSpec {
    name: "spawn_subagent",
    description: "将子任务委派给独立 agent 执行。适用于并行检索、多角度分析、\
                  子问题分解等场景。可同时 spawn 多个子任务并行执行。",
    input_schema: json!({
        "type": "object",
        "properties": {
            "task": {
                "type": "string",
                "description": "子任务的完整描述，需包含足够上下文"
            },
            "context_hint": {
                "type": "string",
                "description": "可选的额外上下文提示"
            },
            "max_rounds": {
                "type": "integer",
                "description": "子任务最大工具调用轮次",
                "default": 2,
                "maximum": 3
            }
        },
        "required": ["task"]
    }),
    access_level: ToolAccessLevel::ReadIndex,
    scene_allowlist: vec![KnowledgeLookup, ResearchSynthesis, DraftingAssist],
    requires_confirmation: false,
    max_results: None,
}
```

#### 2.4.2 执行流程

```rust
// tool_dispatch.rs
"spawn_subagent" => {
    if ctx.depth >= 2 {
        return Err(AppError::msg("sub-agent 嵌套深度超限"));
    }
    spawn_subagent(state, app_handle, ctx, args, provider_config).await
}
```

并行执行：当一轮中有多个 `spawn_subagent` 调用时：

```rust
let subagent_futures: Vec<_> = subagent_calls.iter().map(|call| {
    spawn_subagent(state, app_handle, ctx, &call.args, provider_config.clone())
}).collect();
let results = futures::future::join_all(subagent_futures).await;
```

#### 2.4.3 安全约束

- 最大嵌套深度：2（`HarnessRunInput` 新增 `depth: u32`）
- depth >= 2 时 `spawn_subagent` 不出现在工具列表中
- 子 agent token 预算：`parent_remaining / subagent_count`
- 子 agent 继承父的 scene/note_path/web_search_enabled
- 子 agent 共享父的 session（tool_results 汇总）

#### 2.4.4 前端展示

`ToolCallBubble.tsx` 中识别 `spawn_subagent`：

```tsx
if (toolCall.name === "spawn_subagent") {
  return (
    <SubagentCard
      task={toolCall.args.task}
      status={toolCall.status} // "running" | "completed"
      result={toolCall.result} // 子 agent 回答内容
      expandable
    />
  );
}
```

通过 `ai:harness_trace` 事件中的 `SubagentSpawn`/`SubagentComplete` phase 驱动状态更新。

---

### 2.5 Skills 系统（PR5）

#### 2.5.1 目录结构

```
~/.iris/skills/              ← 全局 skills
  └── <skill-name>/
      └── SKILL.md
<vault>/.iris/skills/        ← vault 级 skills
  └── <skill-name>/
      └── SKILL.md
```

#### 2.5.2 SKILL.md 格式（兼容 Claude）

```markdown
---
trigger: "writing assistance"
description: "Enhanced writing guidance for formal documents"
version: "1.0.0"
author: "iris-community"
source_url: "https://github.com/example/skill-pack"
---

# Writing Assistant

(system prompt instructions here — injected when skill is active)
```

#### 2.5.3 后端（`src-tauri/src/ai_runtime/skills.rs`）

```rust
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub trigger: Option<String>,
    pub content: String,        // SKILL.md 正文（不含 frontmatter）
    pub scope: SkillScope,      // Global | Vault
    pub source_url: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub enabled: bool,
    pub file_path: PathBuf,
}

pub enum SkillScope {
    Global,
    Vault,
}

pub struct SkillRegistry;

impl SkillRegistry {
    /// 扫描全局 + vault 目录下所有 skills。
    pub fn scan_all(vault_path: &Path) -> Vec<SkillEntry>;

    /// 解析单个 SKILL.md。
    pub fn load_skill(path: &Path, scope: SkillScope) -> AppResult<SkillEntry>;

    /// 从 URL 下载并安装（支持 raw SKILL.md 链接）。
    pub async fn install_from_url(
        url: &str,
        target_scope: SkillScope,
        vault_path: &Path,
    ) -> AppResult<SkillEntry>;

    /// 从 Git 仓库安装（clone → 提取子路径）。
    pub async fn install_from_git(
        repo_url: &str,
        subpath: Option<&str>,
        target_scope: SkillScope,
        vault_path: &Path,
    ) -> AppResult<Vec<SkillEntry>>;

    /// 卸载 skill。
    pub fn uninstall(name: &str, scope: SkillScope, vault_path: &Path) -> AppResult<()>;

    /// 启用/禁用 skill（写入 .iris/skills.json 配置）。
    pub fn set_enabled(name: &str, scope: SkillScope, enabled: bool) -> AppResult<()>;

    /// 将活跃 skills 注入 system prompt。
    pub fn inject_into_prompt(skills: &[SkillEntry], scene: AiScene) -> String;
}
```

**注入时机：** `harness.rs` 的 `build_initial_messages` 中：

```rust
let active_skills = SkillRegistry::scan_all(&vault);
let enabled: Vec<_> = active_skills.iter().filter(|s| s.enabled).collect();
if !enabled.is_empty() {
    let skills_prompt = SkillRegistry::inject_into_prompt(&enabled, input.scene);
    messages.push(LlmMessage {
        role: MessageRole::System,
        content: skills_prompt,
        ..Default::default()
    });
}
```

**Tauri commands：**

- `skills_list` → `Vec<SkillEntry>`
- `skills_install { source: "url" | "git" | "local", path_or_url: string, scope: "global" | "vault" }` → `SkillEntry`
- `skills_uninstall { name: string, scope: "global" | "vault" }` → `()`
- `skills_toggle { name: string, scope: "global" | "vault", enabled: bool }` → `()`

#### 2.5.4 前端 Skills 面板

**新组件 `src/components/ai/SkillsPanel.tsx`：**

入口：AI 面板 header 或设置页中的 "Skills" 标签。

布局：

- **已安装列表**（分 Global / Vault 两组 Accordion）
  - 每个 skill 卡片：名称、描述（1行）、来源 badge、版本号
  - 操作：启用/禁用 Switch、编辑（打开 SKILL.md）、删除
- **安装区域**（底部或 dialog）
  - Tab 1: URL 安装 — 输入框 + "安装" 按钮
  - Tab 2: Git 仓库 — repo URL + 子路径输入
  - Tab 3: 本地导入 — 文件拖放区域
- **搜索/筛选**：顶部搜索栏

---

### 2.6 Prompt Profile + Token 仪表盘（PR6）

#### 2.6.1 Prompt Profile

**后端（`src-tauri/src/ai_runtime/prompt_profile.rs`）：**

```rust
pub struct PromptProfile {
    pub persona: String,           // 人格描述
    pub writing_style: String,     // 写作风格偏好
    pub custom_rules: Vec<String>, // 自定义规则列表
    pub language: String,          // 回答语言偏好
}

impl PromptProfile {
    pub fn load(db: &Database) -> AppResult<Self>;
    pub fn save(db: &Database, profile: &Self) -> AppResult<()>;
    pub fn to_system_prompt_fragment(&self) -> String;
}
```

存储：`user_profile` 表中新增 `prompt_profile JSON` 列。

**前端：** 设置页中添加 "AI 人格" 编辑区域：

- Persona textarea
- Writing style 选择（或自定义）
- Custom rules 列表编辑器
- 预设模板快捷选择

#### 2.6.2 Token 用量 + Cache 命中仪表盘

**展示位置：** AI 面板底部（composer 上方），可折叠的一行摘要。

**数据来源：** `HarnessRunResult.usage` → 前端 state

**展示内容：**

折叠态（一行）：

```
本轮 1,234 tokens | Cache 命中 78% | 累计 12.5K tokens
```

展开态（详细卡片）：

```
┌─────────────────────────────────┐
│ 本轮用量                         │
│  Prompt:     800 tokens         │
│  Completion: 434 tokens         │
│  Total:      1,234 tokens       │
│                                 │
│ 缓存效率                         │
│  Cache Hit:  624 tokens (78%)   │
│  Cache Miss: 176 tokens (22%)   │
│                                 │
│ 累计（本 session）               │
│  Total: 12,530 tokens           │
│  Rounds: 3                      │
└─────────────────────────────────┘
```

**实现：**

- 前端新增 `TokenUsageBar` 组件
- 每次 harness 返回后更新 usage state
- Session 级累计通过前端本地累加（不存数据库）

---

## 3. 实施顺序

```
PR2 (Markdown 渲染) → PR1 (历史会话) → PR3 (Harness 进阶)
                                          → PR4 (Sub-agent)
                                             → PR5 (Skills)
                                                → PR6 (Profile + Token)
```

**依赖关系：**

- PR4 依赖 PR3（结构化事件 + 自适应轮次）
- PR5 独立（可与 PR3/PR4 并行开发）
- PR6 独立（可与任何 PR 并行）

---

## 4. 测试策略

每个 PR 必须包含：

- 后端：Rust unit tests（覆盖新增 API）
- 前端：Vitest 单元测试（渲染逻辑、IPC mock）
- 集成：至少 1 个端到端场景测试

重点测试：

- PR2: 各种 markdown 边界情况（流式半截、中文引用、嵌套列表）
- PR3: harness 轮次控制、checkpoint 恢复
- PR4: 并行 sub-agent 竞态、depth 限制
- PR5: SKILL.md 解析、URL 安装失败处理
