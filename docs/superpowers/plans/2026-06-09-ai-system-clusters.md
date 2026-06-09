# AI 体系五集群修复 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Iris AI 子系统的 17 项问题，覆盖安全、工具体系、Skills 激活、编辑确认体验、性能与工程卫生五个独立集群。

**Architecture:** 五集群独立实施，Cluster A 优先（安全修复），Cluster E 最后（性能优化与预算调整）。各集群无交叉依赖。每集群内按 TDD 先写测试再实现。

**Tech Stack:** Rust (Tauri 2.x, tokio), TypeScript/React 19, SQLite + sqlite-vec, fastembed

---

# Cluster A：安全与正确性（3 项）

## Task A1: install_from_git Git Hook 禁用

**Files:**
- Modify: `src-tauri/src/ai_runtime/skills.rs`（`install_from_git` 函数）

- [ ] **Step 1: 写失败测试 `install_from_git_rejects_invalid_skill`**

```rust
// 在 skills.rs 的 #[cfg(test)] mod tests 中新增
#[tokio::test]
async fn install_from_git_rejects_invalid_skill() {
    // 使用本地临时 git repo 模拟（不需要真正 clone）
    // 验证：当 clone 的 SKILL.md 缺少 description 时，返回错误
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test install_from_git_rejects_invalid_skill
```

- [ ] **Step 3: 实现改动**

在 `install_from_git` 中修改 `git clone` 参数：

```rust
// 改前
.args(["clone", "--depth", "1", "--"])

// 改后
.args(["clone", "--depth", "1", "-c", "core.hooksPath=NUL", "--"])
```

在 clone 完成后、复制到 skills 目录前，增加 SKILL.md 验证：

```rust
// 在 let src = ... 之后，复制循环之前插入
if src.join("SKILL.md").exists() {
    match load_skill(&src.join("SKILL.md"), scope) {
        Ok(entry) if entry.validation_status() == SkillValidationStatus::Valid => {},
        Ok(entry) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(AppError::msg(format!(
                "Skill validation failed: {:?}", entry.validation_status()
            )));
        },
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        },
    }
}
```

tmp 清理改用 guard：

```rust
// 在 let tmp = ... 之后
struct TmpGuard(PathBuf);
impl Drop for TmpGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
let _cleanup = TmpGuard(tmp.clone());
```

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test install_from_git
```

---

## Task A2: 工具执行超时

**Files:**
- Modify: `src-tauri/src/ai_runtime/tool_dispatch.rs`

- [ ] **Step 1: 添加超时常量**

```rust
use std::time::Duration;

const TOOL_TIMEOUT_NETWORK: Duration = Duration::from_secs(30);
const TOOL_TIMEOUT_RETRIEVAL: Duration = Duration::from_secs(10);
const TOOL_TIMEOUT_FILE: Duration = Duration::from_secs(5);

fn timeout_for_tool(tool_name: &str) -> Duration {
    match tool_name {
        "web_search" | "fetch_web_page" | "web_fetch_batch"
        | "readability_fetch" | "rendered_fetch" => TOOL_TIMEOUT_NETWORK,
        "read_note" | "get_outline" | "get_backlinks"
        | "get_block_links" | "memory_read" | "memory_write"
        | "skills_read_resource" => TOOL_TIMEOUT_FILE,
        _ => TOOL_TIMEOUT_RETRIEVAL,
    }
}
```

- [ ] **Step 2: 在 `dispatch_tool_with_retry` 中包裹 timeout**

```rust
pub async fn dispatch_tool_with_retry(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolCallResult {
    let timeout = timeout_for_tool(tool_name);
    let mut result = match tokio::time::timeout(timeout, dispatch_tool(state, ctx, tool_name, args)).await {
        Ok(r) => r,
        Err(_) => return ToolCallResult {
            tool_name: tool_name.to_string(),
            success: false,
            output: serde_json::Value::Null,
            duration_ms: timeout.as_millis() as u64,
            tokens_used: None,
            error: Some(format!("tool timed out after {}s", timeout.as_secs())),
        },
    };
    // retry 仅在非 timeout 且是 transient error 时触发
    if is_retryable_tool_error(tool_name, &result) {
        tokio::time::sleep(Duration::from_millis(400)).await;
        result = match tokio::time::timeout(timeout, dispatch_tool(state, ctx, tool_name, args)).await {
            Ok(r) => r,
            Err(_) => return ToolCallResult { /* timeout error */ ... },
        };
    }
    if !result.success && tool_name == "search_hybrid" {
        return dispatch_tool(state, ctx, "search_keyword", args).await;
    }
    result
}
```

- [ ] **Step 3: 写测试**

```rust
#[tokio::test]
async fn test_timeout_returns_error() {
    // 用一个 1ms 超时调 read_note（必然超时）
    // 验证返回 success: false 且 error 包含 "timed out"
}

#[tokio::test]
async fn test_retry_not_triggered_on_timeout() {
    // 验证 timeout 后的结果不会触发 is_retryable_tool_error
}
```

- [ ] **Step 4: 跑测试**

```bash
cargo test tool_dispatch::tests::test_timeout
```

---

## Task A3: Parse Retry 上限

**Files:**
- Modify: `src-tauri/src/ai_harness/harness/run.rs`

- [ ] **Step 1: 添加计数器**

在 `'agent: loop` 之前：

```rust
let mut parse_retries = 0u32;
```

- [ ] **Step 2: 在 `should_retry_tool_parse` 分支中改造**

```rust
if should_retry_tool_parse(&tool_calls) {
    parse_retries += 1;
    if parse_retries > 3 {
        let raw = response.content.clone().unwrap_or_default();
        let stripped = strip_tool_markup_from_visible(&raw);
        let (visible, thinking) = extract_thinking_blocks(&stripped);
        if let Some(t) = thinking {
            emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
        }
        return finish_run(
            state,
            input,
            FinishRunParams {
                content: visible,
                tool_calls: all_tool_calls,
                tool_results: tool_results_json,
                usage: total_usage,
                harness_rounds,
                pending_confirmation: false,
                evidence_packets: ledger_to_packets(&evidence_ledger, token_budget),
                usage_source,
            },
        ).await;
    }
    messages.push(LlmMessage {
        role: MessageRole::User,
        content: "工具参数 JSON 不完整，请重新输出合法的 tool_calls。".into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });
    continue;
}
```

- [ ] **Step 3: 写单元测试验证计数器上限**

```rust
#[test]
fn parse_retry_stops_after_3() {
    // 模拟连续 4 次 retry，第 4 次应触发 finish_run 路径
}
```

- [ ] **Step 4: 跑测试**

```bash
cargo test parse_retry
```

---

# Cluster B：工具体系卫生（3 项）

## Task B1: 双轨权限收敛

**Files:**
- Modify: `src-tauri/src/ai_workflows/research_workflow.rs`
- Modify: `src-tauri/src/ai_runtime/tool_executor.rs`

- [ ] **Step 1: 迁移 research_workflow.rs 到新路径**

```rust
// 改前 (research_workflow.rs:293-299)
if let Some(tool_spec) = registry.find(&tool_call.function.name) {
    if check_tool_permission(tool_spec, AiScene::ResearchSynthesis, AutonomyLevel::L3).is_err() {
        continue;
    }
}

// 改后
use crate::ai_runtime::tool_policy::ToolPolicyContext;

let policy_ctx = ToolPolicyContext {
    scene: AiScene::ResearchSynthesis,
    autonomy_level: AutonomyLevel::L3,
    web_search_enabled: true,
    skill_allowed_tools: vec![],
    depth: 0,
};
if registry.check_tool_policy(&tool_call.function.name, &policy_ctx).is_err() {
    continue;
}
```

- [ ] **Step 2: 旧函数标注 deprecated**

```rust
#[deprecated(since = "0.1.0", note = "use ToolRegistry::check_tool_policy with ToolPolicyContext")]
pub fn check_tool_permission(
    tool: &ToolSpec,
    scene: AiScene,
    allowed_level: AutonomyLevel,
) -> Result<(), ToolPermissionError> {
    // ... 保持不变
}
```

- [ ] **Step 3: 跑编译 + 测试**

```bash
cargo clippy --all-targets -- -D warnings
cargo test research_workflow tool_executor
```

---

## Task B2: execute_tool 删除

**Files:**
- Modify: `src-tauri/src/ai_runtime/tool_executor.rs`

- [ ] **Step 1: 删除方法**

删除整个 `execute_tool` 方法体（含 `async fn execute_tool` 及其 impl 块内的位置）。

- [ ] **Step 2: 删除关联测试**

删除 `execute_tool` 相关的 `#[tokio::test]` 函数（如有）。

- [ ] **Step 3: 编译确认无 broken callers**

```bash
cargo check
```

---

## Task B3: DISPATCHABLE_TOOL_NAMES 消重

**Files:**
- Modify: `src-tauri/src/ai_runtime/tool_dispatch.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog.rs`

- [ ] **Step 1: 删除 tool_dispatch.rs 中的常量**

```rust
// 删除 pub const DISPATCHABLE_TOOL_NAMES: &[&str] = &[...];
// 删除 pub const HARNESS_ONLY_TOOL_NAMES: &[&str] = &[...];
```

- [ ] **Step 2: 更新所有引用**

`tool_dispatch.rs` 中的 `is_exposable_tool` 已用 `catalog_find`，无需改。检查其他文件：

```bash
rg "DISPATCHABLE_TOOL_NAMES|HARNESS_ONLY_TOOL_NAMES" src-tauri/src/
```

将所有引用替换为 `catalog_dispatchable_names()` / `catalog_harness_only_names()`。

- [ ] **Step 3: 更新 tool_catalog.rs 测试**

```rust
#[test]
fn dispatch_list_derived_from_catalog() {
    let disp = catalog_dispatchable_names();
    for name in &disp {
        let entry = catalog_find(name).unwrap();
        assert!(matches!(entry.implementation, ToolImplementationStatus::Dispatchable),
            "catalog dispatchable '{name}' must have Dispatchable implementation");
    }
}
```

- [ ] **Step 4: 编译 + 测试**

```bash
cargo test tool_catalog tool_dispatch
```

---

# Cluster C：Skills 激活体系（3 项）

## Task C1: 技能匹配引擎 — BM25 + embedding 重排

**Files:**
- Modify: `src-tauri/src/ai_runtime/skills.rs`

- [ ] **Step 1: 抽离已有 BM25 逻辑到生产函数**

将测试中的 `bm25_score`、`rank_skills_for_scene` 提升为 `pub(crate)`，去掉 `#[cfg(test)]` 门控。

```rust
pub(crate) struct ScoredSkill {
    pub entry: SkillEntry,
    pub score: f64,
}

pub(crate) fn rank_skills_for_scene(
    skills: &[SkillEntry],
    scene: AiScene,
    user_message: &str,
) -> Vec<ScoredSkill> {
    // 从测试模块迁移，增加 user_message 参数
    // BM25 对 name + description + metadata.keywords 评分
    // legacy_trigger 匹配加分 (bonus = 2.0)
}
```

- [ ] **Step 2: 实现 embedding 重排**

```rust
use crate::embedding::engine::{cosine_similarity, embed_text};

pub(crate) fn rerank_with_embedding(
    candidates: Vec<ScoredSkill>,
    user_message: &str,
) -> Vec<ScoredSkill> {
    let Ok(user_vec) = embed_text(&user_message.chars().take(500).collect::<String>()) else {
        return candidates; // 回退：保留 BM25 排序
    };
    // 对每个 candidate 读 embedding_json，计算 cosine
    // 如果 embedding_json 不存在或无 fastembed → 跳过
    for mut s in &mut candidates {
        if let Some(emb_json) = ... { // 从 s.entry.metadata 或 skill_activation_index 读取
            if let Ok(skill_vec) = parse_embedding(emb_json) {
                let cos = cosine_similarity(&user_vec, &skill_vec);
                s.score = s.score * 0.4 + cos as f64 * 0.6; // 混合分数
            }
        }
    }
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    candidates
}
```

- [ ] **Step 3: 替换 `skills_for_scene`**

```rust
pub fn skills_for_scene(
    skills: &[SkillEntry],
    scene: AiScene,
    user_message: &str,
) -> Vec<SkillEntry> {
    let enabled: Vec<SkillEntry> = skills.iter()
        .filter(|s| s.enabled)
        .cloned()
        .collect();
    let ranked = rank_skills_for_scene(&enabled, scene, user_message);
    let reranked = rerank_with_embedding(ranked, user_message);
    reranked.into_iter()
        .filter(|s| s.score >= 0.35)
        .take(3)
        .map(|s| s.entry)
        .collect()
}
```

- [ ] **Step 4: 更新调用者**

在 `prepare_environment_and_skills` 中传入用户消息：

```rust
// context.rs 中，需要额外的参数 user_message
pub(crate) fn prepare_environment_and_skills(
    state: &AppState,
    scene: AiScene,
    note_path: Option<&str>,
    note_title: Option<&str>,
    selection_excerpt: Option<&str>,
    scene_tools: &[ToolSpec],
    user_message: &str,  // 新增
) -> AppResult<(String, String)> {
    // ...
    let enabled_skills = active_skills_for_prompt(&vault, scene, Some(&state.db), user_message)?;
    // ...
}
```

- [ ] **Step 5: Skill 安装时计算 embedding**

在 `load_skill` 完成后（`skills.rs`），异步计算并存储：

```rust
pub fn store_skill_embedding(
    db: &Database,
    entry: &SkillEntry,
) -> AppResult<()> {
    let text = format!("{} {}", entry.name, entry.description);
    if let Ok(vec) = embed_text(&text) {
        let json = serde_json::to_string(&vec).unwrap_or_default();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO skill_activation_index
                 (skill_name, scope, description, keywords, embedding_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
                rusqlite::params![entry.name, format!("{:?}", entry.scope),
                    entry.description, entry.metadata.get("keywords").and_then(|v| v.as_str()).unwrap_or(""),
                    json],
            )?;
            Ok(())
        })?;
    }
    Ok(())
}
```

- [ ] **Step 6: 写测试**

```rust
#[test]
fn skill_matching_bm25_precision() { /* BM25 匹配正确性 */ }
#[test]
fn skill_matching_embedding_rerank() { /* embedding 重排提升相关度 */ }
#[test]
fn skill_matching_fallback_on_no_embedding() { /* fastembed 失败时回退 BM25 */ }
#[test]
fn skill_matching_respects_threshold() { /* score < 0.35 不激活 */ }
#[test]
fn skill_matching_tops_at_3() { /* 超过 3 个 skill 时截断 */ }
```

---

## Task C2: 匹配触发模式

**Files:**
- Modify: `src-tauri/src/ai_harness/harness/context.rs`（`prepare_environment_and_skills` 调用侧）
- Modify: `src-tauri/src/ai_harness/harness/run.rs`（传入 user_message）

- [ ] **Step 1: 在 harness 入口传入 user_message**

```rust
// run.rs 中，用户消息从 input.history_messages 最后一个 User 消息提取
let user_message = input.history_messages.last()
    .map(|(role, msg)| if role == "user" { msg.as_str() } else { "" })
    .unwrap_or("");
```

- [ ] **Step 2: 不逐轮重匹配**

确认只在 harness 启动时调用一次 `prepare_environment_and_skills`，不在循环中重调。

---

## Task C3: skills_read_resource 长度限制

**Files:**
- Modify: `src-tauri/src/ai_runtime/skills.rs`

- [ ] **Step 1: 加常量和截断逻辑**

```rust
const MAX_SKILL_RESOURCE_CHARS: usize = 24_000;

// 在 read_skill_resource 函数返回前
pub fn read_skill_resource(
    vault: &Path,
    name: &str,
    scope: SkillScope,
    relative_path: &str,
) -> AppResult<String> {
    // ... 现有路径校验逻辑 ...
    let content = std::fs::read_to_string(path)?;
    let char_count = content.chars().count();
    let truncated = char_count > MAX_SKILL_RESOURCE_CHARS;
    let body: String = content.chars().take(MAX_SKILL_RESOURCE_CHARS).collect();

    Ok(serde_json::json!({
        "content": body,
        "truncated": truncated,
        "original_char_count": char_count,
    }).to_string())
}
```

注意：返回值从 `String` 改为 JSON 结构。需同步更新 `skills_read_resource_tool` 中的反序列化。

- [ ] **Step 2: 写测试**

```rust
#[test]
fn read_skill_resource_truncates_over_limit() {
    // 创建一个大于 24_000 字符的 reference 文件，验证 truncated: true
}
```

---

# Cluster D：编辑确认体验（1 项）

## Task D1: 写入确认接入 Diff 预览

**Files:**
- Modify: `src-tauri/src/ai_harness/harness/run.rs`（`pause_for_tool_confirmation`）
- Modify: `src/components/ai/PatchPreview.tsx`（抽取 DiffView）
- Modify: `src/components/ai/ToolConfirmDialog.tsx`（接入 DiffView）

- [ ] **Step 1: 后端 — 在 `pause_for_tool_confirmation` 中构建 preview**

```rust
// 在 confirm_request 构建处（run.rs ~line 630）
if tool_name == "insert_text_at_cursor" || tool_name == "replace_selection" {
    if let Some(target_path) = args.get("target_path").and_then(|v| v.as_str())
        .or(input.note_path.as_deref())
    {
        if let Ok(vault) = state.vault_path() {
            let abs = resolve_vault_path(&vault, target_path).ok();
            if let Some(abs) = abs {
                if let Ok(current) = std::fs::read_to_string(&abs) {
                    let context_len = 80;
                    let range_start = args.get("range").and_then(|v| v.get("start")).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let range_end = args.get("range").and_then(|v| v.get("end")).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let before_start = range_start.saturating_sub(context_len);
                    let before_end = (range_end + context_len).min(current.chars().count());
                    let before_context: String = current.chars().skip(before_start).take(before_end - before_start).collect();

                    let replacement = if tool_name == "insert_text_at_cursor" {
                        args.get("text")
                    } else {
                        args.get("replacement")
                    }.and_then(|v| v.as_str()).unwrap_or("");

                    let mut after = current.clone();
                    after.replace_range(range_start..range_end, replacement);
                    let after_context: String = after.chars().skip(before_start).take(before_end - before_start + replacement.chars().count()).collect();

                    confirm_request["preview"] = serde_json::json!({
                        "patch_type": if tool_name == "insert_text_at_cursor" { "insert" } else { "replace" },
                        "target_path": target_path,
                        "before_context": before_context,
                        "after_context": after_context,
                        "risk_level": args.get("risk_level").and_then(|v| v.as_str()).unwrap_or("medium"),
                    });
                }
            }
        }
    }
}
```

- [ ] **Step 2: 前端 — 从 `PatchPreview.tsx` 抽取 `DiffView`**

```typescript
// PatchPreview.tsx 中新增 export
export interface DiffViewProps {
  beforeText: string;
  afterText: string;
  patchType: "insert" | "replace";
  riskLevel: "low" | "medium" | "high";
  targetPath: string;
}

export function DiffView({ beforeText, afterText, patchType, riskLevel, targetPath }: DiffViewProps) {
  const [showFullDiff, setShowFullDiff] = useState(false);
  const riskStyle = RISK_STYLES[riskLevel] ?? RISK_STYLES.low!;
  const beforeLines = beforeText.split("\n");
  const afterLines = afterText.split("\n");
  const maxLines = Math.max(beforeLines.length, afterLines.length);
  const displayLines = showFullDiff ? maxLines : Math.min(5, maxLines);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-xs text-muted-foreground">{targetPath}</span>
        <Badge variant="outline" className={cn("text-xs", riskStyle.className)}>
          {riskStyle.label}
        </Badge>
      </div>
      <div className="overflow-hidden rounded-md border border-border/60">
        <div className="max-h-[200px] overflow-auto font-mono text-xs">
          <div className="border-b border-border/40">
            <div className="bg-red-500/5 px-3 py-1 text-red-600">- 原文</div>
            {beforeLines.slice(0, displayLines).map((line, i) => (
              <div key={i} className="px-3 py-0.5 text-red-600/80">
                <span className="mr-2 select-none text-red-400">-</span>{line || " "}
              </div>
            ))}
          </div>
          <div>
            <div className="bg-green-500/5 px-3 py-1 text-green-600">+ 改后</div>
            {afterLines.slice(0, displayLines).map((line, i) => (
              <div key={i} className="px-3 py-0.5 text-green-600/80">
                <span className="mr-2 select-none text-green-400">+</span>{line || " "}
              </div>
            ))}
          </div>
        </div>
        {maxLines > 5 && (
          <button className="w-full border-t border-border/40 bg-muted/30 px-3 py-1 text-xs" onClick={() => setShowFullDiff(!showFullDiff)}>
            {showFullDiff ? "收起" : `展开全部 ${maxLines} 行`}
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: `PatchPreview` 复用 `DiffView`**

将 `PatchPreview` 中的原文/替换文逐行对比逻辑替换为 `<DiffView beforeText={...} afterText={...} ... />`。

- [ ] **Step 4: `ToolConfirmDialog` 接入 `DiffView`**

```typescript
// 在 showPatchReview 分支中，替换当前 hash-only 卡片
{showPatchReview && request.preview && (
  <DiffView
    beforeText={String(request.preview.before_context ?? "")}
    afterText={String(request.preview.after_context ?? "")}
    patchType={(request.preview.patch_type as "insert" | "replace") ?? "replace"}
    riskLevel={(request.preview.risk_level as "low" | "medium" | "high") ?? "medium"}
    targetPath={String(request.preview.target_path ?? "")}
  />
)}
```

- [ ] **Step 5: 编译 + 类型检查**

```bash
cargo check
npm run typecheck
npm run lint
```

---

# Cluster E：性能与工程卫生（7 项）

## Task E1: Persona 预设 display_name

**Files:**
- Modify: `src-tauri/src/ai_runtime/prompt_profile.rs`

- [ ] **Step 1: 修改三行常量**

```rust
// 学术严谨
PromptProfile {
    display_name: "学者".into(),   // 原为 "砃".into()
    ...
}
// 创意写作
PromptProfile {
    display_name: "墨韵".into(),
    ...
}
// 简洁高效
PromptProfile {
    display_name: "疾风".into(),
    ...
}
```

- [ ] **Step 2: 更新测试中的断言**

```bash
cargo test persona_resolver::tests::default_persona_uses_custom_display_name
```

---

## Task E2: 重复人设注入

**Files:**
- Modify: `src-tauri/src/ai_runtime/environment.rs`

- [ ] **Step 1: 删除 `to_system_prompt_fragment` 调用**

在 `build_environment_map` 中找到以下逻辑并删除：

```rust
// 删除此段
let profile = PromptProfile::load(db).unwrap_or_default();
let persona_fragment = profile.to_system_prompt_fragment();
// env_text += persona_fragment;  ← 删除对应拼接
```

- [ ] **Step 2: 编译确认**

```bash
cargo check
```

---

## Task E3: ToolPolicy 逐轮缓存

**Files:**
- Modify: `src-tauri/src/ai_harness/harness/run.rs`

- [ ] **Step 1: 在 harness 循环开始处缓存**

```rust
// 'agent: loop 内部、while 之前
let policy_cache = tool_policy::compute_available_tools(&policy_ctx);
let (auto_tools, confirm_tools) = policy_cache;
```

- [ ] **Step 2: 替换散落的 `check_tool_policy` 调用**

将同轮内 `registry.check_tool_policy(name, &policy_ctx)` 替换为：

```rust
fn is_tool_cached(name: &str, auto: &[String], confirm: &[String]) -> bool {
    auto.iter().any(|t| t == name) || confirm.iter().any(|t| t == name)
}

// 调用处
if !is_tool_cached(tool_name, &auto_tools, &confirm_tools) {
    // denied
}
```

- [ ] **Step 3: 测试缓存一致性**

```rust
#[test]
fn policy_cache_consistent_with_individual_eval() {
    let ctx = ToolPolicyContext { ... };
    let (auto, confirm) = compute_available_tools(&ctx);
    for name in &auto {
        assert_eq!(evaluate_tool(name, &ctx), ToolPolicyVerdict::AutoAllowed);
    }
    for name in &confirm {
        assert_eq!(evaluate_tool(name, &ctx), ToolPolicyVerdict::RequiresConfirmation);
    }
}
```

---

## Task E4: SQLite spawn_blocking

**Files:**
- Modify: `src-tauri/src/ai_runtime/tool_dispatch.rs`

- [ ] **Step 1: 包裹 DB 调用**

覆盖 `hybrid_search`、`list_vault`、`get_backlinks`、`get_block_links`、`get_context_packets`、`regulation_lookup`：

```rust
async fn hybrid_search(
    state: &AppState,
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let query = args["query"].as_str()...;
    let limit = ...;
    let layers = ...;
    let state_ref = state.clone(); // 或 Arc clone
    let query_owned = query.to_string();
    let note_ctx = ctx.note_path.map(|s| s.to_string());
    let packets = tokio::task::spawn_blocking(move || {
        state_ref.db.with_read_conn(|conn| {
            let request = RetrievalRequest {
                query: query_owned,
                max_results: limit,
                layers,
                note_context: note_ctx,
                file_id_context: ctx_ref.file_id,
                scope: RetrievalScope::default(),
            };
            crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
        })
    }).await.map_err(|e| AppError::msg(e.to_string()))??;
    Ok(serde_json::json!({ "results": packets, "count": packets.len() }))
}
```

同样模式应用到 `list_vault`、`get_backlinks`、`get_block_links`。

- [ ] **Step 2: 编译 + 测试**

```bash
cargo test tool_dispatch
```

---

## Task E5: 并行工具执行

**Files:**
- Modify: `src-tauri/src/ai_harness/harness/run.rs`

- [ ] **Step 1: 分流只读工具**

```rust
const PARALLEL_READONLY_TOOLS: &[&str] = &[
    "search_hybrid", "search_semantic", "search_keyword",
    "read_note", "get_outline", "get_backlinks", "get_block_links",
    "list_vault", "get_context_packets", "get_regulation",
];

let (parallel_calls, sequential_calls): (Vec<_>, Vec<_>) = other_calls
    .iter()
    .partition(|tc| PARALLEL_READONLY_TOOLS.contains(&tc.function.name.as_str()));
```

- [ ] **Step 2: 并行执行 + 统一收集**

```rust
if !parallel_calls.is_empty() {
    let futures: Vec<_> = parallel_calls.iter().map(|tc| {
        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
        let dispatch_ctx = ToolDispatchContext { ... };
        dispatch_tool_with_retry(state, &dispatch_ctx, &tc.function.name, &args)
    }).collect();
    let results = futures_util::future::join_all(futures).await;
    for (tc, result) in parallel_calls.iter().zip(results) {
        // 推 tool message（与原有串行逻辑一致）
        messages.push(LlmMessage { role: MessageRole::Tool, content: ..., tool_call_id: Some(tc.id.clone()), ... });
        tool_results_json.push(...);
    }
}
```

- [ ] **Step 3: 顺序执行剩余 sequential_calls**（含 `fetch_web_page` 限制逻辑）

```rust
for tc in &sequential_calls {
    // 保持原有串行逻辑不变
}
```

- [ ] **Step 4: 编译 + 测试**

```bash
cargo test run_harness
```

---

## Task E6: Fastembed 预热

**Files:**
- Modify: `src-tauri/src/app.rs`（`AppState::new` 或 setup 逻辑中）

- [ ] **Step 1: 添加快捷预热调用**

```rust
// 在 AppState::new 或 setup 完成后
tokio::spawn(async {
    let _ = crate::embedding::engine::embed_text("warmup");
    tracing::info!("fastembed warmup complete");
});
```

- [ ] **Step 2: 编译确认**

```bash
cargo check
```

---

## Task E7: 场景预算翻倍

**Files:**
- Modify: `src-tauri/src/ai_types/mod.rs`（`resolve_scene`）

- [ ] **Step 1: 修改四组数值**

```rust
pub fn resolve_scene(scene: AiScene) -> SceneProfile {
    match scene {
        AiScene::KnowledgeLookup => SceneProfile {
            default_token_budget: 100_000,  // 原 30_000
            max_token_budget: 240_000,       // 原 80_000
            ..
        },
        AiScene::ExemplarLearning => SceneProfile {
            default_token_budget: 120_000,   // 原 50_000
            max_token_budget: 320_000,       // 原 120_000
            ..
        },
        AiScene::DraftingAssist => SceneProfile {
            default_token_budget: 160_000,   // 原 60_000
            max_token_budget: 320_000,       // 原 160_000
            ..
        },
        AiScene::ResearchSynthesis => SceneProfile {
            default_token_budget: 200_000,   // 原 100_000
            max_token_budget: 480_000,       // 原 240_000
            ..
        },
    }
}
```

- [ ] **Step 2: 编译 + 测试**

```bash
cargo test ai_types
cargo test planning
```

---

## 最终回归验证

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run lint
npm run format:check
npm run typecheck
```

---

## 假设

1. A3（子 agent 预算）已移除，父子预算独立，维持当前 60% 分配逻辑
2. fastembed 模型已预下载到用户环境
3. `bm25_score` 函数在 `skills.rs` 测试模块中已存在，可复用
4. 并行工具执行中结果顺序与串行时一致
