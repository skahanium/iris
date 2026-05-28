# 文档版本系统（B+）Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将版本系统改为双层保存（勤写 `.md`、稀疏快照），引入 `kind`、配额与折叠时间线，并修复 `storage_path` 一致性。

**Architecture:** 前端 `useEditorSave` 仅触发 `file_write`；Rust `file_write` 不再调用 `create_snapshot`；新模块 `version/trigger.rs`（或等价）集中快照规则；`VersionTimeline` 按 kind 分组渲染折叠组。

**Tech Stack:** Tauri 2 / Rust / rusqlite migrations / React 19 / TipTap / 现有 `version/mod.rs` IPC。

**Design doc:** [2026-05-26-document-version-design.md](./2026-05-26-document-version-design.md)

---

## Task 1: Migration — `versions.kind` + `storage_path` 修复

**Files:**

- Create: `src-tauri/migrations/006_versions_kind.sql`
- Create: `src-tauri/migrations/006_versions_kind.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`（注册 006）

**Step 1:** 写 migration `up`：

```sql
ALTER TABLE versions ADD COLUMN kind TEXT NOT NULL DEFAULT 'manual';
UPDATE versions SET storage_path = file_id || '/' || version_no || '.md'
  WHERE storage_path NOT LIKE '%/%' OR storage_path NOT GLOB '*[0-9]*.md';
-- 按实际脏数据调整 WHERE；或逐行用 file_id+version_no 重建路径
```

**Step 2:** 写 `down`：删除 `kind` 列（SQLite 需重建表或保留列 — 与项目既有 migration 风格一致）。

**Step 3:** `cargo test` 迁移相关测试（若有 `migrate` 测试模块）。

**Step 4:** Commit（用户要求时）：`feat(storage): 为 versions 增加 kind 并修复 storage_path`

---

## Task 2: Rust — 解耦 `file_write` 与快照

**Files:**

- Modify: `src-tauri/src/commands/file.rs` — 移除 `thread::spawn` 内 `create_snapshot`
- Modify: `src-tauri/src/version/mod.rs` — `create_snapshot` 增加参数 `kind: VersionKind`
- Create: `src-tauri/src/version/kind.rs` — 枚举 + `as_str()`
- Create: `src-tauri/src/version/policy.rs` — `should_snapshot`, 配额, 间隔检查

**Step 1:** 写失败测试 `policy_skips_duplicate_hash`、`policy_respects_idle_interval`（`src-tauri/src/version/policy.rs` 内 `#[cfg(test)]`）。

**Step 2:** 运行 `cargo test policy_` 确认失败。

**Step 3:** 实现 policy；`create_snapshot` 写入 `kind` 与正确 `storage_path`（`format!("{}/{}.md", file_id, version_no)`）。

**Step 4:** 测试通过；`cargo clippy --all-targets -- -D warnings`。

---

## Task 3: IPC — 显式「保存版本」与空闲触发

**Files:**

- Modify: `src-tauri/src/lib.rs` / `commands` — 新增 `version_save_manual(path, content)`
- Modify: `src/types/ipc.ts`, `src/lib/ipc.ts`
- Modify: `src/hooks/useEditorSave.ts` — 防抖改为 1200ms（或读设置）
- Create: `src/hooks/useVersionIdle.ts` — 文档打开时 idle 计时，调用 `version_create_idle` 或复用 `create_snapshot` IPC

**Step 1:** 前端测试：`useEditorSave` 调用 `fileWrite` 不调用 `version*`（mock ipc）。

**Step 2:** App 内注册 Ctrl+S → flush 层 1 + `versionSaveManual`。

**Step 3:** 验证手动路径产生 `kind=manual`。

---

## Task 4: 定稿 = 新建快照

**Files:**

- Modify: `version_finalize` → 重命名为 `version_finalize_current` 或改签名：读当前 path 内容 → `create_snapshot(..., kind=Finalize, is_finalized=true)`
- Modify: `src/components/version/VersionTimeline.tsx` — 定稿按钮调新 IPC
- Test: `finalize_creates_new_row_with_is_finalized`

---

## Task 5: 恢复与安全确认

**Files:**

- Modify: `version_restore` — 保证 `pre_restore` kind
- Modify: `VersionTimeline.tsx` — 恢复前 `confirm()`；定稿目标额外文案
- Test: restore 前快照条数 +1

---

## Task 6: 配额与清理

**Files:**

- Modify: `version/mod.rs` — `enforce_auto_idle_cap(file_id, max=30)` 在插入后调用
- Modify: `version_cleanup` — 仅删 `kind=auto_idle` 且非 finalized（已定稿永不删）
- Test: 插入第 31 条 auto 时最旧 auto 被删

---

## Task 7: 时间线 UI — 折叠自动备份（决策 A）

**Files:**

- Modify: `src/components/version/VersionTimeline.tsx`
- Create: `src/components/version/version-timeline-groups.ts` — 按日 + kind 分组
- Modify: `src/types/ipc.ts` — `VersionEntry.kind`

**Step 1:** 组件测试：给定 5 条 `auto_idle`，默认渲染 1 个折叠标题「自动备份（5）」，无 5 行列表项。

**Step 2:** 点击展开显示 5 行；定稿区始终可见。

**Step 3:** 双栏布局：当前 `currentContent` | `preview`。

---

## Task 8: 新建文档命名与 title 展示

**Files:**

- Modify: `src/lib/note-create.ts`（或等价）
- Modify: `src-tauri` 创建文件逻辑 / `file_create` 标题推导
- Modify: 标签栏组件使用 `title` 优先

**参考设计 doc §2.2。**

---

## Task 9: 文档同步 ✅

**Files:**

- [x] Modify: `ARCHITECTURE.md` 版本节 — 与双层、kind、折叠 UI 一致
- [x] Modify: `ROADMAP.md` v0.3 勾选说明（若验收完成）
- [x] Modify: `CHANGELOG.md`、`docs/README.md` 索引

---

## Task 10: 全量验证 ✅

```bash
cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
cd .. && pnpm run lint && pnpm run typecheck && pnpm run test
```

**2026-05-26 结果：** Rust 78 passed（含 `version::*`、`migration_006`）；前端 71 passed。修复 `index_file_extracts_wiki_links`（wikilink 按 `files.title`/stem 解析，测试改用 `[[a]]`）。`cargo fmt` 格式化 `commands/file.rs`。

---

## 依赖顺序

```
Task 1 → Task 2 → Task 3 → Task 4 → Task 5 → Task 6 → Task 7
                                              ↘ Task 8 可并行
Task 9、10 最后
```
