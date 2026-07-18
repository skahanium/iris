# Iris 技术债深挖审计

> 审计日期：2026-07-18  
> 基线分支：`branch/v1.2.9`  
> 来源：DeepSeek 报告纠偏 + 文件生命周期 / Markdown / Harness·MCP·LLM 第二轮深挖

## 执行摘要

DeepSeek 报告在**方向**上可用，但有两处高优误报需剔除：**生产路径 panic 崩溃**（实测 6/7 在 `#[cfg(test)]`）、**commands/indexer/knowledge 零测试**（indexer/knowledge 内联测试较完整）。

本轮深挖更值得关注的是重构残留：**双栈并行**、**文件生命周期缺口**、**Markdown 多路径分叉**、**IPC 孤儿入口**。

| 域           | 最关键发现                                                                               | 严重度 |
| ------------ | ---------------------------------------------------------------------------------------- | ------ |
| 文件生命周期 | 删除不 flush 脏内容；`write`/`trash` 与 rename 锁不一致                                  | P0/P1  |
| Markdown     | 生产 PM 路径 vs 遗留 Turndown 测试分叉；chunker 无视 code fence                          | P1     |
| Harness      | 涉密 ephemeral 为唯一生产路径；`classified_run_engine.rs` 未编入模块；orphan `llm_*` IPC | P1/P2  |
| MCP/LLM      | legacy stdio/HTTP 死代码；fetch 路由不对称；preset 双端硬编码                            | P1/P2  |

## DeepSeek 纠偏表

| 原结论                                                | 裁决                                                                    |
| ----------------------------------------------------- | ----------------------------------------------------------------------- |
| 生产 7 处 panic 崩溃                                  | **误报**（6 处测试；1 处 KDF `unwrap`）                                 |
| 三目录零测试                                          | **夸大**（commands 约 13 文件无单测；indexer/knowledge 有测）           |
| `allow(dead_code)` / failover 字符串 / 无 IPC codegen | **成立**（dead_code / failover 已在本轮收口；IPC codegen 仍属 Phase 4） |
| `fetch_web_page` v1/v2 生产不一致                     | **误报**（v2 仅测试）                                                   |
| HarnessOnly 在 catalog                                | 2 活跃 + 5 僵尸                                                         |

## A. 文件保存链与生命周期

**主路径（正向）**：Editor → `DocumentPersistenceCoordinator` → `file_write` → `NoteWriteService` → `atomic_write` → best-effort index。

### 问题（历史基线；代码侧已修）

1. **删除丢未保存编辑（P0/P1）** — 已修：删除前 `persistBeforeLeave`。
2. **`WelcomeEmpty` 绕过生命周期（P1）** — 已修。
3. **写/删锁不一致（P1）** — 已修：`write_inner`/`trash` 持锁；路径解析亦在锁内；真并发测已补。
4. **卫生（P2）** — 占位标题 / `title_from_path` 已收敛到 `storage/note_title.rs`。

## B. Markdown

1. **多路径（P1）** — 生产 round-trip 测试已迁到 PM harness；遗留 Marked/Turndown 仅作非契约路径。
2. **编辑器 vs indexer 漂移（P1）** — chunker fence 已修；media/frontmatter 见契约文档。
3. **卫生（P2）** — 过时 TDD 注释已清。

## C. Harness / MCP / LLM

1. **涉密** — ephemeral 单栈；CEF run API 仅 `cfg(test)`。
2. **Orphan LLM IPC** — 已删。
3. **MCP** — rmcp 生产路径；legacy 解析死代码已删；optional 凭据服务走共享 JSON。
4. **配置** — failover 优先 `AppError::Provider`；LLM builtin / MCP optional 共享 `config/*.json`；temperature 文档化为固定 `None`。

## 本轮实施验收（2026-07-18）

| 计划项                            | 状态               | 证据 / 缺口                                                                          |
| --------------------------------- | ------------------ | ------------------------------------------------------------------------------------ |
| Phase 0 审计落盘                  | **完成**           | 本文件；audit/progress 事实更新                                                      |
| 1.1 删除前 flush                  | **完成**           | `persistBeforeLeave` + WelcomeEmpty                                                  |
| 1.2 持久化锁                      | **完成**           | 锁内 resolve+write；`concurrent_rename_and_write_*` / `concurrent_trash_and_write_*` |
| 1.3 Orphan LLM IPC                | **完成**           | `llm_generate`/`llm_chat` 已注销                                                     |
| 1.4 涉密单栈                      | **完成**           | 删除未编入引擎；CEF 生命周期 `cfg(test)`                                             |
| 1.5 web.fetch 对齐                | **完成**           | 跟随 selected search provider                                                        |
| 1.6 Tavily                        | **完成**           | 从 preset 移除                                                                       |
| 2 chunker fence                   | **完成**           | `FenceState`                                                                         |
| 2 统一 PM round-trip              | **完成**           | 四个 markdown-contract 文件改走生产 harness                                          |
| 2 media/frontmatter 契约          | **完成（文档）**   | `docs/markdown-indexing-contract.md`                                                 |
| 3 删 MCP legacy                   | **完成**           | stdio 手写层 + `parse_http_json_rpc_response` 已删                                   |
| 3 去掉 blanket `allow(dead_code)` | **完成**           | 全 `src-tauri/src` 无 `allow(dead_code)`；clippy `-D warnings` 绿                    |
| 3 僵尸 HarnessOnly                | **完成**           | catalog 仅 dispatchable 写工具                                                       |
| 3 failover 结构化                 | **完成**           | `AppError::Provider` + gateway/streaming 发射；message 仅 fallback                   |
| 3 preset 单源                     | **完成**           | `config/llm-builtin-providers.json` + `config/mcp-optional-credential-services.json` |
| 3 占位标题收敛                    | **完成**           | `storage/note_title.rs`                                                              |
| 3 temperature                     | **完成（文档化）** | 固定 `None`；契约文档说明；不暴露 UI                                                 |

### 明确未做（计划 Phase 4 / 范围外）

- IPC codegen、大文件拆分、temperature UI 暴露、索引 degraded 重试队列。

## 相关文档

- [2026-07-13 Agent Harness 差距审计](./2026-07-13-agent-harness-refactor-gap-analysis.md)
- [progress.md](../progress.md)
- [markdown-indexing-contract.md](../markdown-indexing-contract.md)
