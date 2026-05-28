# Iris

> 本地优先、AI 原生的 Markdown 笔记软件

**Iris**（虹膜）是一个完全运行在本地的桌面笔记应用。你的笔记以标准 `.md` 文件存储在你的磁盘上，永远不会被锁定在专有格式中。AI 不是附加功能，而是编辑器的一部分——像键盘一样自然。

---

## 为什么选择 Iris

| 特性           | Iris                       | Obsidian           | Notion             | Logseq             |
| -------------- | -------------------------- | ------------------ | ------------------ | ------------------ |
| 数据格式       | `.md` 纯文本               | `.md` 纯文本       | 专有格式           | `.md`              |
| 本地优先       | :white_check_mark:         | :white_check_mark: | :x:                | :white_check_mark: |
| AI 原生编辑器  | :white_check_mark: 内联 AI | :x: 插件实现       | :white_check_mark: | :x:                |
| 打包体积       | ~10MB                      | ~200MB             | N/A                | ~300MB             |
| 开源许可证     | AGPL-3.0                   | 专有               | 专有               | AGPL-3.0           |
| 向量语义搜索   | :white_check_mark: 内置    | :x:                | :white_check_mark: | :x:                |
| 第三方插件生态 | :x: **不计划**             | :white_check_mark: | :x:                | :x:                |

## 技术栈

```
渲染层    React 19 + TailwindCSS + shadcn/ui + TipTap (Prosemirror)
───────────────────────────────────────────────
逻辑层    Rust (Tauri 2.x)
          ├─ 文件系统操作、加密 (AES-256-GCM)
          ├─ SQLite 元数据索引 + fastembed 语义检索（v0.2 计划 sqlite-vec）
          └─ LLM API 编排 (OpenAI / Claude / Ollama)
───────────────────────────────────────────────
存储层    .md 文件 (数据) + SQLite (索引/缓存)
```

- **桌面框架**: Tauri 2.x — 5-10MB 打包体积，50-100MB 内存占用，Rust 后端天然安全隔离
- **编辑器核心**: TipTap (Prosemirror) — 结构化文档节点树，AI 原生交互的基础
- **向量搜索**: fastembed + SQLite BLOB 嵌入（v0.1）；v0.2 升级为 sqlite-vec 虚拟表（见 [docs/eval/semantic-search.md](docs/eval/semantic-search.md)）
- **AI 集成**: 兼容 OpenAI API 格式，支持远程 API 和本地 Ollama 模型

## 核心能力

### 内联 AI

选中一段文字 → 改写 / 扩写 / 翻译 / 简化。AI 在编辑器的节点层级上操作，而非字符串拼接。结果以带操作按钮的节点插入（接受 / 重试 / 回退），你决定保留什么。

### 语义搜索

用自然语言搜索笔记。"上个月关于性能优化的会议记录" — 不是关键词匹配，是语义理解。基于向量嵌入，Top-K 召回。

### 上下文问答

基于当前笔记和关联笔记的全文内容向 AI 提问。不复制粘贴，不切换窗口。

### / 命令唤起

在编辑器中输入 `/` → AI 命令菜单：总结、生成大纲、头脑风暴、翻译全文、修复语法……可以在设置中自定义。

### 数据自主权

每个笔记是独立的 `.md` 文件。用 VS Code、Typora、任何编辑器都能打开。SQLite 是缓存，文件是数据。

### 界面取向

**Notion 式扁平编辑**（灰阶壳层 + 居中内容栏）为主，**命令优先**（`Ctrl+P` / 可收起 AI 侧栏）为辅；不做第三方插件生态。见 [设计系统](docs/design-system.md) 与 [路线图 · 体验方向](ROADMAP.md#体验方向与路线图绑定)。

## 快速开始

> **当前状态**：**v0.1.0** 已发布；**v0.4.0-ui** 推进 Notion 式界面重建。排期见 [ROADMAP.md](./ROADMAP.md)，变更见 [CHANGELOG.md](./CHANGELOG.md)，文档索引见 [docs/README.md](docs/README.md)。

### 环境要求

- [Rust](https://rustup.rs/) 1.75+
- [Node.js](https://nodejs.org/) 20+
- [Node 包管理器](https://nodejs.org/)：`npm`（仓库含 `package-lock.json`）或 [pnpm](https://pnpm.io/) 9+
- Windows 10+ / macOS 13+ / Linux (X11/Wayland)

### 开发

```bash
# 克隆仓库
git clone https://github.com/skahanium/iris.git
cd iris

# 安装前端依赖
npm ci
# 或：pnpm install

# 启动开发模式（热更新）
npm run tauri dev

# 构建生产版本
npm run tauri build
```

### 配置 AI

1. 启动应用并选择笔记目录（Vault）
2. 右栏 **AI** 面板选择提供商（OpenAI / Claude / Ollama / 自定义）
3. 填入 API Key（存入操作系统凭据管理器，不落盘）
4. 可选：底栏「联网」需在设置中配置 MiniMax Token Plan Key（见「MiniMax 联网检索」）；`Ctrl+Shift+A` 可收起 AI 侧栏以专注写作

## 项目结构

```
iris/
├── src/                    # React 前端源码
├── src-tauri/              # Rust / Tauri 后端
├── tests/                  # 前端与 E2E 测试
├── docs/                   # [文档索引](docs/README.md)：Epic、设计系统、语义评测
├── scripts/                # 维护脚本（图标生成等）
│   └── assets/             # 品牌源图（非根目录）
├── package.json            # 前端依赖（Vite/Tauri 惯例在根目录）
├── vite.config.ts
├── index.html
├── README.md / ROADMAP.md  # 门面文档在根目录
└── …                       # tsconfig、eslint、tailwind 等工具配置
```

图标维护见 [scripts/assets/README.md](./scripts/assets/README.md)。

## 设计哲学

- **文件即数据，数据库即缓存** — 你的笔记是 `.md` 文件，永远如此。SQLite 只做索引加速。
- **AI 是输入方式，不是聊天窗口** — AI 集成在编辑器内部，像键盘一样自然，像鼠标一样精确。
- **结构化是 AI 的前提** — Prosemirror 的文档节点树让 AI 理解文档结构，而非猜测。
- **速度是功能** — Tauri 2.x 的 Rust 后端消除 GC 停顿，50MB 内存运行。
- **开源就是安全** — AGPL-3.0。代码可审计，数据在你自己手里。
- **克制的产品边界** — 无第三方插件、无移动端/实时协作；扩展靠主线版本与 fork（见 [ROADMAP](ROADMAP.md)）。

## 文档

| 你想…                     | 阅读                                                            |
| ------------------------- | --------------------------------------------------------------- |
| 看版本计划与体验排期      | [ROADMAP.md](./ROADMAP.md)                                      |
| 改界面 / token / 纸墨规范 | [docs/design-system.md](docs/design-system.md)                  |
| 查架构、IPC、数据流       | [ARCHITECTURE.md](./ARCHITECTURE.md)                            |
| 参与开发                  | [CONTRIBUTING.md](./CONTRIBUTING.md) · [AGENTS.md](./AGENTS.md) |
| 全部文档列表              | [docs/README.md](docs/README.md)                                |

## 许可证

Iris 采用 [GNU Affero General Public License v3.0](./LICENSE) 开源。

版权所有 (C) 2026 Iris 贡献者。
