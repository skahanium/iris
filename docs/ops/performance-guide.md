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
