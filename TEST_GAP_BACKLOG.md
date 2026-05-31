# 测试缺口补录清单

最后更新：2026-05-31

本文档只记录“最该先补、且可以直接开写”的具体测试场景，不写宽泛功能域。

排序口径：

1. 用户主路径影响大
2. 回归后容易伤数据或直接不可用
3. 当前测试保护薄弱或只有源码字符串契约
4. 编写成本相对可控

## Top 10

| 优先级 | 具体测试场景 | 建议层级 | 建议新增测试文件 | 目标代码 |
| --- | --- | --- | --- | --- |
| 1 | `file_create` 通过 TS IPC 封装创建新笔记后，磁盘生成 UTF-8 `.md`，返回路径可被 `file_read` 立即读回 | Rust 集成 + TS 契约 | `src-tauri/tests/file_create_read_contract.rs` / `tests/ipc-file-create.test.ts` | `src-tauri/src/commands/file.rs`, `src/lib/ipc.ts` |
| 2 | `file_rename` 重命名已打开笔记后，返回的新路径能同步驱动 `path_sync` 建议结果，旧路径不再可读 | Rust 集成 + TS 契约 | `src-tauri/tests/file_rename_path_sync.rs` / `tests/path-sync-rename.test.ts` | `src-tauri/src/commands/file.rs`, `src/lib/path-sync.ts`, `src/lib/ipc.ts` |
| 3 | `file_delete` 删除存在版本历史的笔记后，条目进入回收站；`recycle_restore` 恢复后，正文与最近版本链保持可用 | Rust 集成 | `src-tauri/tests/file_delete_recycle_restore.rs` | `src-tauri/src/commands/file.rs`, `src-tauri/src/commands/recycle.rs`, `src-tauri/src/recycle/mod.rs`, `src-tauri/src/version/mod.rs` |
| 4 | `version_restore_cmd` 恢复历史版本时，先自动生成一条 `pre_restore` 快照，再把目标内容返回给前端 | Rust 集成 + TS 契约 | `src-tauri/tests/version_restore_command.rs` / `tests/ipc-version-restore.test.ts` | `src-tauri/src/commands/version.rs`, `src-tauri/src/version/mod.rs`, `src/lib/ipc.ts` |
| 5 | `search_keyword` 在 `file_write` 后能搜到正文、标题和 frontmatter tag；笔记重命名后旧路径命中消失、新路径命中存在 | Rust 集成 | `src-tauri/tests/search_keyword_reindex.rs` | `src-tauri/src/commands/search.rs`, `src-tauri/src/indexer/scan.rs`, `src-tauri/src/indexer/frontmatter.rs` |
| 6 | sqlite-vec 不可用时，`search_semantic` 仍走 fallback 路径并返回结果，而不是直接报错或空数组 | Rust 集成 | `src-tauri/tests/search_semantic_fallback.rs` | `src-tauri/src/commands/search.rs`, `src-tauri/src/storage/db.rs`, `src-tauri/src/embedding/store.rs` |
| 7 | `QuickOpen` 中键盘输入过滤后，方向键移动高亮，按 Enter 会触发打开目标笔记，Esc 关闭面板 | React 组件集成 | `tests/quick-open-keyboard-flow.test.tsx` | `src/components/file/QuickOpen.tsx` |
| 8 | `VaultNavigator` 中新建文件夹后树立即出现节点；展开目录、点击文件、重命名文件三步不会丢失当前选中态 | React 组件集成 | `tests/vault-navigator-tree-flow.test.tsx` | `src/components/file/VaultNavigator.tsx` |
| 9 | `UnifiedAssistantPanel` 在“有选中文本”时发送改写请求，会走 `assistantExecute`，进入 loading，收到 patch 响应后渲染 `PatchPreview` | React 组件集成 | `tests/unified-assistant-selection-patch-flow.test.tsx` | `src/components/ai/UnifiedAssistantPanel.tsx`, `src/lib/ipc.ts` |
| 10 | `ToolConfirmDialog` 在拒绝工具调用时，会回调 `tool_confirm(false)`，关闭弹窗，并且不会把工具结果写进消息流 | React 组件集成 | `tests/tool-confirm-deny-flow.test.tsx` | `src/components/ai/ToolConfirmDialog.tsx`, `src/components/ai/UnifiedAssistantPanel.tsx`, `src/lib/ipc.ts` |

## 每条场景的最小通过标准

### 1. 创建后立即可读

- 调 `fileCreate()` 后返回的路径以 `.md` 结尾
- 对应磁盘文件存在且是合法 UTF-8
- 紧接着 `fileRead()` 读取成功
- 不允许出现“创建成功但索引/读取层还不可见”的状态

### 2. 重命名后路径同步正确

- `fileRename()` 返回的新路径存在
- 旧路径 `fileRead()` 失败
- `suggestPathSync()` 不再把旧路径当成有效目标
- 前端 tab/打开态后续可据此补测试，但第一步先锁定命令与路径层

### 3. 删除进入回收站且可恢复

- 删除前先制造至少 1 条版本快照
- 删除后 `recycle_list` 能看见条目
- 恢复后正文内容正确
- 恢复后 `version_list` 仍能列出历史，而不是链路断裂

### 4. 恢复前强制生成保护快照

- 构造“当前正文”和“历史正文”不同的场景
- 调 `version_restore_cmd`
- 断言新生成的最近一条版本类型为 `pre_restore`
- 返回给前端的正文等于被恢复的历史正文

### 5. 关键词索引跟随写入和重命名变化

- 写入包含正文关键字、标题关键字、frontmatter tags 的笔记
- `search_keyword` 三类信息都能命中
- 重命名后旧路径结果消失
- 新路径结果保留，避免脏索引

### 6. 语义搜索降级仍可工作

- 模拟 `sqlite-vec` 不可加载
- `search_semantic` 不 panic、不返回内部扩展错误
- fallback 结果至少包含相关笔记
- 命中排序不要求完全稳定，但必须“有结果且可解释”

### 7. Quick Open 键盘主流程

- 输入关键字后列表收缩
- `ArrowDown` 改变高亮项
- `Enter` 调用打开回调且参数为当前高亮项
- `Escape` 关闭并清理临时查询

### 8. VaultNavigator 树状态保持

- 新建文件夹后，无需整页刷新即可见
- 展开父目录后子节点保留
- 重命名当前选中文件后，选中态迁移到新路径
- 不允许树刷新后跳到错误节点或丢失展开状态

### 9. 统一助手“选中改写 -> PatchPreview”

- 有 selection quote 时发送消息
- 触发 `assistantExecute` 而不是普通 chat 分支
- loading 态出现
- 响应含 patch proposal 时渲染 `PatchPreview`
- 原消息列表与 patch 预览同时存在，不互相覆盖

### 10. 工具拒绝分支必须干净回收

- 打开 `ToolConfirmDialog`
- 点击拒绝后调用 `toolConfirm` 且 `approved=false`
- 弹窗关闭
- 消息流中不出现伪造的成功工具结果
- 后续还能继续发送下一条消息，不残留 pending 状态

## 暂不放入 Top 10，但应排在下一批

- `SearchPanel` 查询后点击结果打开笔记
- `SessionHistoryDropdown` 的 `session_rename` / `session_load`
- `credential_set` / `credential_delete` 不落盘且错误信息脱敏
- `graph_data` 在新增 `[[wikilink]]` 后更新节点和边
- 外部文件修改冲突对话框 `ConflictDialog` 的保留本地/重载磁盘分支

## 现状备注

- 当前 `tests/e2e/*` 主要还是源码契约与选择器稳定性检查，不是真正的 Tauri 全链路 UI 驱动。
- 当前前端覆盖率运行结果为 `22.25%` statements，离 `ROADMAP.md` 的 `> 80%` 目标差距很大。
- 当前 Rust `cargo test` 因 `sqlite_vec` 引用问题无法完整跑通；补测前应先修复测试入口可用性。
