# Iris

> 本地优先的 Markdown 笔记应用 — 笔记归你，AI 在编辑器里帮忙

**Iris**（虹膜）是一个完全运行在本地的桌面笔记应用。你的笔记以标准 `.md` 文件存储在磁盘上，可用 VS Code、Typora 或任何编辑器打开——永远不会被锁定在专有格式中。SQLite 负责笔记索引（可从 `.md` 重建）和应用状态（AI 会话、网页缓存）。

在此基础上，Iris 将 AI 集成在编辑器内部（内联改写、`/` 命令、助手面板），而非独立的聊天窗口。引用带来源，写入需你确认。

**当前版本**：v1.2.2。功能建设已超越原 v0.x 规划，现聚焦 v1.2.2 稳定发布（国际化、无障碍、性能基准、完整图片工作流）。

---

## 为什么选择 Iris

| 特性        | Iris                                  | Obsidian           | Notion             |
| ----------- | ------------------------------------- | ------------------ | ------------------ |
| 数据格式    | `.md` 纯文本                          | `.md` 纯文本       | 专有格式           |
| 本地优先    | :white_check_mark:                    | :white_check_mark: | :x:                |
| 打包体积    | ~10MB                                 | ~200MB             | N/A                |
| 混合搜索    | :white_check_mark: 关键词 + 语义      | 关键词为主         | :white_check_mark: |
| 版本历史    | :white_check_mark: 快照 + 双栏对比    | 插件实现           | 有限               |
| AI 能力扩展 | Skills 提示词包（用户显式安装）+ 配置 | 通用插件市场       | 封闭生态           |
| 开源许可证  | AGPL-3.0                              | 专有               | 专有               |

Iris **不做** Obsidian 式通用插件 API 或插件市场；AI 行为可通过 [Skills](src/components/ai/SkillsPanel.tsx)（Claude 兼容 `SKILL.md`，支持 URL / Git / 本地安装）与声明式配置扩展。深度定制请 fork 或向上游提 PR。

---

## 核心能力

### 数据自主权

每个笔记是独立的 `.md` 文件。文件是数据，SQLite 是缓存。外部修改有 L1/L2/L3 冲突处理。

### 知识网络

`[[双向链接]]`、反向链接面板（`Ctrl+Shift+B`）、`#标签` 聚合（`Ctrl+Shift+T`）、知识图谱（`Ctrl+Shift+G`）。笔记之间的连接自动维护。

### 混合搜索

FTS5 关键词 + 向量语义 + 链接/标签融合。用自然语言找笔记——「上个月关于性能优化的会议记录」，而不只是关键词匹配。

### 版本历史

层 1：`Ctrl+S` 保存当前 `.md`；层 2：`Ctrl+Shift+S` 或空闲 10 分钟自动检查点、定稿快照。双栏对比时间线（`Ctrl+Shift+V`），恢复前强制保护快照。

### 编辑器内 AI

- **内联**：选中文本 → 改写 / 扩写 / 翻译 / 简化，接受 / 重试 / 回退
- **`/` 命令**：总结、大纲、头脑风暴等，结果流式写入编辑器
- **助手面板**（`Ctrl+Shift+A`）：按场景自动路由（写作、整理、研究…），引用带来源，所有写入需确认
- **Skills**：全局或 Vault 级安装 `SKILL.md` 提示词包，注入 AI 行为（命令面板 →「管理 AI Skills」）

### 编辑体验

TipTap WYSIWYG Markdown、多标签页、章节折叠、悬浮大纲（`Ctrl+Shift+O`）、Zen 模式（`Ctrl+.`）。界面取向 Notion 式扁平编辑，详见 [docs/design-system.md](docs/design-system.md)。

---

## 技术栈

```
渲染层    React 19 + TailwindCSS + shadcn/ui + TipTap (Prosemirror)
───────────────────────────────────────────────
逻辑层    Rust (Tauri 2.x)
          ├─ 文件系统操作、混合检索
          └─ LLM API 编排 + AI Runtime
───────────────────────────────────────────────
存储层    .md 文件 (数据) + SQLite (索引/缓存)
```

- **桌面框架**: Tauri 2.x — 约 5–10MB 打包体积，50–100MB 内存占用
- **编辑器**: TipTap (Prosemirror) — 结构化文档节点树
- **搜索**: FTS5 + 向量检索（默认 Rust cosine fallback 可用；sqlite-vec vec0 为 optional/experimental，当前 Windows 构建有阻塞）+ fastembed (384-dim)
- **AI**: 兼容 OpenAI API 格式，支持远程 HTTPS API 与自定义 OpenAI-compatible HTTPS 端点

架构细节见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

---

## 快速开始

### 环境要求

- [Rust](https://rustup.rs/) 1.80+（见 `src-tauri/Cargo.toml`）
- [Node.js](https://nodejs.org/) 20+
- npm 10+
- Windows 10+ / macOS 13+ / Linux (X11/Wayland)

### 开发

```bash
git clone https://github.com/skahanium/iris.git
cd iris

npm ci              # 安装前端依赖
npm run tauri dev   # 启动开发模式（热更新）
npm run tauri build # 构建生产版本
```

### 配置 AI

1. 启动应用并选择笔记目录（Vault）
2. 右栏 AI 面板选择提供商（DeepSeek / OpenAI-compatible 自定义端点）
3. 填入 API Key（存入操作系统凭据管理器，不落盘）
4. 可选：设置页配置 LLM 路由、联网搜索、提示词偏好

---

## 项目结构

```
iris/
├── src/                    # React 前端源码
├── src-tauri/              # Rust / Tauri 后端
├── tests/                  # 前端与 E2E 测试
├── docs/                   # 文档索引、设计系统、历史记录
├── scripts/                # 维护脚本
├── package.json
├── vite.config.ts
└── index.html
```

---

## 设计哲学

- **文件即数据，数据库即缓存** — 笔记永远是 `.md` 纯文本，SQLite 只做索引加速。
- **本地优先** — 笔记、索引与基础编辑完全离线可用；AI 可选接入远程或用户自配的 OpenAI-compatible HTTPS 端点。
- **AI 在编辑器里，不在聊天窗** — 内联、`/` 命令与助手面板均围绕当前文档，写入需确认。
- **速度是功能** — Tauri + Rust 后端，轻量内存占用。
- **开源就是安全** — AGPL-3.0，代码可审计，数据在你自己手里。
- **克制的产品边界** — 无通用插件 API / 插件市场 / 移动端 / 实时协作；AI 行为靠 Skills 与配置扩展，深度定制靠 fork / PR。

---

## 文档

| 你想…                 | 阅读                                                            |
| --------------------- | --------------------------------------------------------------- |
| 使用指南（用户向）    | Notion 官方文档站（v1.1.0 交付，URL 待发布）                    |
| 看路线图与里程碑      | [ROADMAP.md](./ROADMAP.md)                                      |
| 改界面 / token / 组件 | [docs/design-system.md](docs/design-system.md)                  |
| 查架构、IPC、数据流   | [ARCHITECTURE.md](./ARCHITECTURE.md)                            |
| 参与开发              | [CONTRIBUTING.md](./CONTRIBUTING.md) · [AGENTS.md](./AGENTS.md) |
| 全部文档列表          | [docs/README.md](docs/README.md)                                |

---

## 许可证

Iris 采用 [GNU Affero General Public License v3.0](./LICENSE) 开源。

版权所有 (C) 2026 Iris 贡献者。
