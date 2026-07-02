# Iris 性能指南

本文档包含开发环境性能建议和 CPU/性能基线采样指南。

---

## 开发环境性能建议

### 启动

- 只保留**一个** `npm run dev:desktop`（或 `npm run tauri dev`）终端。
- 不要同时用浏览器单独打开 `http://127.0.0.1:1420`，避免重复 Vite 实例。
- 开发时避免并行 `npm run test:watch`，会叠加第二套文件监听。

### Vault 目录

- 笔记库放在**本地非同步**路径（勿用 iCloud Desktop、OneDrive 根目录）。
- 大库首次打开会触发增量索引；等待状态栏「笔记库已同步」后再密集使用 AI 检索。

### 可选环境变量

| 变量                     | 作用                                                          |
| ------------------------ | ------------------------------------------------------------- |
| `VITE_SKIP_AUTO_INDEX=1` | 开发时跳过启动自动 `index_rescan`（需手动在命令面板重建索引） |

### 与生产差异

开发态会额外运行 **Node（Vite）** 与 **Cargo 监视重编译**；Activity Monitor 里 `node` 偏高属正常。验证生产负载请使用 `npm run tauri build` 后的 `.app`。

---

## CPU / 性能基线采样指南

用于验证严苛审计与 CPU 治理改动前后的 Activity Monitor 表现。采样时关闭无关重型应用。

### 环境准备

| 场景 | 启动方式                                        | 预期进程                                                       |
| ---- | ----------------------------------------------- | -------------------------------------------------------------- |
| 开发 | `npm run dev:desktop`（或 `npm run tauri dev`） | `node`（Vite）+ `iris`（或 target/debug 二进制）+ 可能 `cargo` |
| 生产 | `npm run tauri build` 后运行 `.app`             | 仅 `iris`，**不应**有 `node`                                   |

### 采样步骤

1. 打开「活动监视器」，按 CPU 排序。
2. 记录空闲 30s 的 `% CPU`（`iris`、`node`、`kernel_task`）。
3. 执行下表操作各一次，记录峰值与恢复时间（降至 &lt;5% 所需秒数）。

| 操作                               | 开发态关注          | 生产态关注                    |
| ---------------------------------- | ------------------- | ----------------------------- |
| 冷启动应用                         | `node` + `iris`     | 仅 `iris`                     |
| 选择/切换 vault                    | `iris` 持续高位时长 | 全库索引应在数十秒内回落      |
| 连续编辑保存 1 分钟（1200ms 防抖） | `iris`              | **单次**嵌入尖峰/文件，非双重 |
| 打开知识图谱面板                   | 前端主线程          | `GraphView` rAF               |
| AI 助手检索/对话                   | `iris` + 网络       | 大库避免长时间全表 cosine     |

### 对比判据（治理后）

- 同一 vault 二次启动：CPU 尖峰明显低于首次（增量索引 / hash 短路）。
- 保存同一笔记：Activity Monitor 中 `iris` 嵌入相关尖峰约 **1 次/保存**，非 2–3 次。
- 生产包无 `node` 进程。
- `kernel_task` 仅在磁盘 I/O 密集时短暂升高，不应长期占满 CPU。

### 可选：采样命令

```bash
# 查看 iris 相关进程（macOS）
ps aux | egrep 'iris|vite|node.*1420' | grep -v grep

# Rust 侧日志（索引跳过/嵌入队列）
RUST_LOG=iris_lib::indexer=info,iris_lib::embedding=info npm run dev:desktop
```

## Document Open Runtime

Budgets:

- Hot mounted tab activation: <= 16ms visible commit, no disk read.
- Warm prepared open: <= 50ms visible commit after selection.
- Cold open: loading surface visible within 100ms.
- Cold 50KB Markdown note: first editor frame within 1000ms on a normal development machine.

When investigating regressions, check runtime traces by source (`welcome`, `quick-open`, `file-tree`, `tab`, `startup`, `search`, `graph`, `outline`, `ai`, `management`, `recycle`, `classified`) and cache state (`hit`, `miss`, `write`, `none`). Trace output must not include note paths, titles, Markdown body, frontmatter, prompts, selections, credentials, or decrypted classified content.

## Iris Performance Optimization Baselines

本节用于执行 `2026-06-28-iris-perf-optimization-design.md` 的改前 / 改后量测。没有改前和改后记录，不得声称性能优化完成。

### 记录模板

| 字段          | 内容                                                             |
| ------------- | ---------------------------------------------------------------- |
| 日期          | YYYY-MM-DD                                                       |
| 分支 / commit | 记录当前 commit hash                                             |
| 构建模式      | development / production                                         |
| 机器          | CPU、内存、系统版本                                              |
| 数据集        | vault 文件数、样本文档大小                                       |
| 场景          | 离线冷启动 / AI 长流式输出 / 已打开 Tab 快速回切 / 10000+ 文件库 |
| 改前          | 关键指标、trace 名称、截图路径                                   |
| 改后          | 关键指标、trace 名称、截图路径                                   |
| 结论          | 是否达到本节预算                                                 |

### 必测场景

| 场景                | 步骤                                                    | 关注指标                                                                 |
| ------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------ |
| 离线冷启动          | 断开网络，启动 Iris，打开 DevTools Network              | 外部字体请求为 0，首屏不等待字体 CSS                                     |
| AI 长流式输出       | 使用 3000-8000 字输出样本，包含段落、表格、引用、代码块 | DevTools Performance 中 >50ms long task 数量，React Profiler commit 分布 |
| 已打开 Tab 快速回切 | 打开 5 个 Markdown 文档后连续切换                       | 已打开 tab 不重新 ingest Markdown，不重新显示冷打开 loading surface      |
| 10000+ 文件库       | 选择大 vault，等待索引稳定后打开搜索、文件树、编辑器    | 首次可交互时间、索引结束后 CPU 回落时间                                  |

### 隐私要求

性能 trace、截图、日志和文档摘要不得包含用户笔记正文、frontmatter、prompt、API Key、Token、解密后的涉密内容或凭据材料。需要说明笔记规模时，只记录文件数、字节数、路径 hash 或合成 fixture 名称。具体而言：不得包含用户笔记正文；不得包含 API Key；不得包含解密后的涉密内容。

### Editor Surface Cache Check

Use this check before changing `AppEditorWorkspace` caching behavior.

1. Open five Markdown notes with different sizes.
2. Switch between the five tabs twice.
3. Confirm already opened tabs do not show the cold-open loading surface again.
4. Record whether `data-editor-visibility="hidden"` surfaces remain bounded by `READY_SURFACE_RETAIN_LIMIT`.
5. Confirm hidden editor surfaces do not trigger repeated expensive outline, stats, or ingest work while inactive.

If a regression is observed, prefer a small adjustment inside `AppEditorWorkspace` before introducing a new cache owner. Do not add `EditorViewCache.tsx` or a WeakRef editor pool.

## Frontend Audit Optimization Pass

本节用于记录 `FRONTEND-AUDIT.md` 派生的前端优化批次。执行时必须记录改前 / 改后数据；没有数据不得声称性能优化完成。

| 场景                 | 记录项                                                                              |
| -------------------- | ----------------------------------------------------------------------------------- |
| AI 面板懒加载        | `index-*.js` 体积、AI 面板 chunk 名称与体积、冷启动未打开 AI 面板时是否加载该 chunk |
| 大纲虚拟化           | 100+ heading 文档下目录 DOM 节点数、滚动与 active heading 高亮是否正常              |
| Graph reduced-motion | 系统 reduced-motion 开启时是否停止持续 rAF；大图谱节点数与可交互时间                |
| Overlay fallback     | Management Center、版本记录、知识图谱懒加载期间是否显示非空 loading surface         |

记录不得包含用户笔记正文、真实路径、prompt、API Key、Token 或解密后的涉密内容。
