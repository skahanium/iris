# 贡献指南

感谢你有意为 Iris 做出贡献。本文档将帮助你搭建开发环境、了解项目规范并提交高质量的代码。

---

## 文档体系

贡献前请知悉 **[docs/README.md](./docs/README.md)** 中的层级：

- **排期（唯一）**：[ROADMAP.md](./ROADMAP.md) — 功能与体验同一里程碑
- **界面**：[docs/design-system.md](./docs/design-system.md) — Notion N / 命令 C
- **实现**：[ARCHITECTURE.md](./ARCHITECTURE.md)
- 修改 UI 时同步 design-system + ROADMAP 对应 checklist；修改 IPC 时见 AGENTS.md §4.2。

## 目录

- [行为准则](#行为准则)
- [开发环境](#开发环境)
- [项目结构](#项目结构)
- [开发流程](#开发流程)
- [Commit 规范](#commit-规范)
- [代码风格](#代码风格)
- [测试](#测试)
- [Pull Request 流程](#pull-request-流程)
- [Issue 规范](#issue-规范)

## 行为准则

参与本项目即表示你同意遵守 [贡献者行为准则](./CODE_OF_CONDUCT.md)。

## 开发环境

### 必需工具

| 工具    | 最低版本 | 安装方式                                                                                       |
| ------- | -------- | ---------------------------------------------------------------------------------------------- |
| Rust    | 1.80+    | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh`（见 `src-tauri/Cargo.toml`） |
| Node.js | 20+      | [nodejs.org](https://nodejs.org/) 或 `fnm` / `nvm`                                             |
| npm     | 10+      | 随 Node.js 安装；仓库使用 `package-lock.json`，请 `npm ci`                                     |

### Windows 额外要求

- [Microsoft Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（选择"使用 C++ 的桌面开发"工作负载）
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 已预装）

### Linux 额外要求

```bash
# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel

# Arch
sudo pacman -S webkit2gtk-4.1 base-devel openssl libappindicator-gtk3 librsvg
```

### 启动开发

```bash
git clone https://github.com/skahanium/iris.git
cd iris

npm ci                    # 安装前端依赖
npm run tauri dev         # 启动 Tauri 开发模式（含热更新）
npm run dev               # 仅启动前端开发服务器（用于 UI 调试）
```

## 项目结构

```
iris/
├── src/                    # React 前端源码
│   ├── components/
│   │   ├── editor/         # TipTap 编辑器及扩展
│   │   ├── ai/             # AI 交互组件（面板、气泡、证据包等）
│   │   │   └── assistant/  # 研究专注态、文档检查等子视图
│   │   ├── layout/         # 布局组件（壳层、标签栏、状态栏）
│   │   ├── file/           # 文件管理（QuickOpen、搜索、版本、回收站）
│   │   │   └── version/    # 版本时间线子组件
│   │   ├── settings/       # 设置面板
│   │   ├── graph/          # 知识图谱
│   │   ├── tag/            # 标签聚合
│   │   ├── brand/          # 品牌图标
│   │   ├── common/         # 通用对话框
│   │   └── ui/             # shadcn/ui 基础组件 + 共享 UI 原语
│   ├── hooks/              # 自定义 React Hooks
│   ├── lib/                # 纯函数工具
│   │   ├── ai/             # AI 相关工具（citation、session、context 等）
│   │   ├── ipc.ts          # Tauri IPC 类型安全封装
│   │   └── utils.ts        # 通用工具函数
│   ├── types/              # TypeScript 类型定义
│   └── styles/             # 全局样式
├── src-tauri/              # Rust 后端源码
│   ├── src/
│   │   ├── main.rs         # Tauri 应用入口
│   │   ├── lib.rs          # 模块注册
│   │   ├── commands/       # IPC 命令处理
│   │   ├── ai_runtime/     # AI Runtime 管线
│   │   ├── llm/            # LLM API 编排
│   │   ├── embedding/      # 向量嵌入生成与检索
│   │   ├── indexer/        # Markdown 解析与索引
│   │   ├── knowledge/      # 知识索引（锚点、法规、文种模板）
│   │   ├── storage/        # SQLite 数据库层
│   │   ├── version/        # 版本快照系统
│   │   ├── watcher/        # 文件系统事件监听
│   │   ├── security/       # 安全删除
│   │   ├── network/        # 证书固定
│   │   ├── recycle/        # 回收站
│   │   └── credentials.rs  # OS 凭据管理器
│   ├── migrations/         # SQLite 迁移文件
│   ├── tests/              # Rust 集成测试
│   ├── benches/            # 性能基准测试
│   ├── Cargo.toml
│   └── tauri.conf.json
├── tests/                  # 前端测试
│   └── e2e/                # E2E 测试
├── docs/                   # 文档
│   ├── design-system/      # 设计系统细则
│   ├── eval/               # 语义搜索评测
│   └── history/            # 历史版本 Epic 与审计
├── scripts/                # 维护脚本
│   └── assets/             # 品牌源图
├── AGENTS.md               # AI 开发规范
├── ARCHITECTURE.md         # 技术架构
├── ROADMAP.md              # 版本路线图
└── README.md               # 项目门面
```

## 开发流程

1. **Fork** 本仓库
2. 为你的功能或修复创建一个 **feature branch**：`feat/功能描述` 或 `fix/问题描述`
3. 在本地开发，遵循代码风格和测试要求
4. 提交前运行本地检查
5. 推送到你的 Fork 并创建 **Pull Request**

### 本地检查命令

```bash
# Rust 侧
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit

# 前端
npm run lint
npm run format:check
npm run typecheck
npm run test

# E2E
npm run tauri build -- --debug
npm run test:e2e
```

## Commit 规范

本项目使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 规范。所有 commit 消息使用**中文**。

### 格式

```
<类型>(<范围>): <描述>

<可选的详细描述>

<可选的脚注>
```

### 类型

| 类型       | 说明                                       |
| ---------- | ------------------------------------------ |
| `feat`     | 新功能                                     |
| `fix`      | 漏洞修复                                   |
| `docs`     | 仅文档变更                                 |
| `style`    | 不影响代码含义的变更（空格、格式、分号等） |
| `refactor` | 既不修复 bug 也不增加功能的代码变更        |
| `perf`     | 提升性能的代码变更                         |
| `test`     | 添加或修正测试                             |
| `chore`    | 构建过程或辅助工具的变更                   |
| `ci`       | CI 配置文件和脚本的变更                    |

### 范围（常用）

`editor` | `ai` | `storage` | `search` | `crypto` | `ui` | `ipc` | `docs`

### 示例

```
feat(editor): 添加 / 命令唤起 AI 功能菜单

实现了输入 / 时弹出 AI 命令选择面板，包括总结、翻译、改写等选项。
选中命令后自动在编辑器中插入 ai-stream 节点并开始流式请求。

Closes #42
```

## 代码风格

### Rust

- 遵循 `cargo fmt` 的默认格式
- `cargo clippy` 警告视为错误
- 禁止使用 `unsafe` 除非在代码注释中说明必要性，并在 PR 中标注待评审
- 公共 API 函数必须有文档注释（`///`）
- 模块内部使用 `pub(crate)` 而非 `pub`
- 异步函数使用 `tokio`，避免混用多个运行时

### TypeScript / React

- 遵循 ESLint 和 Prettier 配置
- 组件文件 PascalCase，目录 kebab-case
- Hooks 文件以 `use` 前缀命名
- 类型定义优先使用 `interface`，联合类型使用 `type`
- 避免 `any`，必要时使用 `unknown` + 类型守卫
- 组件 props 必须显式定义接口，不使用 `React.FC`
- IPC 调用必须通过 `src/lib/ipc.ts` 中的类型安全封装，禁止直接 `invoke()`

### 命名约定

| 上下文           | 约定            | 示例           |
| ---------------- | --------------- | -------------- |
| React 组件       | PascalCase      | `AiPanel.tsx`  |
| 组件目录         | kebab-case      | `ai/`          |
| Rust 源文件      | snake_case      | `file_ops.rs`  |
| Rust 类型/结构体 | PascalCase      | `FileMetadata` |
| Rust 函数/变量   | snake_case      | `read_file()`  |
| SQLite 表名      | snake_case 复数 | `file_tags`    |
| SQLite 列名      | snake_case      | `created_at`   |

## 测试

### 测试要求

- **新功能**：必须包含对应的测试
- **Bug 修复**：必须先写复现该 bug 的测试，确认测试失败后再修复
- **重构**：重构前后测试必须全绿

### 运行测试

```bash
# Rust
cargo test
cargo test <模块名>

# 前端
npm run test
npm run test:watch
npm run test:coverage

# E2E
npm run tauri build -- --debug
npm run test:e2e
```

### 手工回归（关闭路径）

窗口关闭与进程退出需本地验证，清单见 [docs/testing/app-close-manual-checklist.md](./docs/testing/app-close-manual-checklist.md)。涉及 `useTauriCloseSave` / `app_close` 的 PR 应在 Test plan 中勾选该清单。

## Pull Request 流程

1. 确保你的分支基于最新的 `main` 分支（先 `rebase`，不要 `merge`）
2. 通过所有本地检查（格式、Lint、类型检查、测试）
3. 在 GitHub 创建 Pull Request，使用 PR 模板
4. 在 PR 描述中关联相关 Issue（`Closes #123` 或 `Related to #123`）
5. 等待 CI 流水线通过
6. 至少需要一个维护者的 Code Review 批准
7. 批准后由维护者合并

### PR 标题格式

与 Commit 规范一致：`类型(范围): 描述`

### 新增依赖

在 PR 描述中说明新增依赖的理由和替代方案考虑。不设硬性审批门禁，但 maintainer 可能要求讨论。

## Issue 规范

- **Bug 报告**：使用 Bug 报告模板
- **功能请求**：使用功能请求模板
- **安全漏洞**：**不要公开提交 Issue**，请参阅 [安全策略](./SECURITY.md)

## 问题求助

1. 先搜索已有的 Issue 和 Discussion
2. 在 Discussion 中发起问答
3. 或在相应的 Issue 下评论

感谢你的贡献。
