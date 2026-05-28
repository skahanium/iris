# 贡献指南

感谢你有意为 Iris 做出贡献。本文档将帮助你搭建开发环境、了解项目规范并提交高质量的代码。

---

## 文档体系

贡献前请知悉 **[docs/README.md](./docs/README.md)** 中的层级：

- **排期（唯一）**：[ROADMAP.md](./ROADMAP.md) — 功能与体验同一里程碑
- **界面**：[docs/design-system.md](./docs/design-system.md) — 纸墨 B / 命令 C
- **实现**：[ARCHITECTURE.md](./ARCHITECTURE.md)
- **v0.1.0 历史补齐**：[docs/v0.1.0-completion-prs.md](./docs/v0.1.0-completion-prs.md)（已冻结）
- **当前 Epic**：[docs/v0.1.1-epic.md](./docs/v0.1.1-epic.md)

修改 UI 时同步 design-system + ROADMAP 对应版本 checklist；修改 IPC 时见下文与 AGENTS.md §4.2。

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

| 工具        | 最低版本          | 安装方式                                                          |
| ----------- | ----------------- | ----------------------------------------------------------------- |
| Rust        | 1.75+             | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js     | 20+               | [nodejs.org](https://nodejs.org/) 或 `fnm` / `nvm`                |
| npm 或 pnpm | npm 10+ / pnpm 9+ | 仓库含 `package-lock.json`，推荐 `npm ci`；pnpm 亦可              |

### Windows 额外要求

- [Microsoft Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（选择"使用 C++ 的桌面开发"工作负载）
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 已预装，Windows 10 可能需要手动安装）

### Linux 额外要求

```bash
# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel \
  librsvg2-devel

# Arch
sudo pacman -S webkit2gtk-4.1 base-devel openssl libappindicator-gtk3 librsvg
```

### 启动开发

```bash
# 克隆仓库
git clone https://github.com/skahanium/iris.git
cd iris

# 安装前端依赖
npm ci

# 启动 Tauri 开发模式（含热更新）
npm run tauri dev

# 仅启动前端开发服务器（不启动 Rust 后端，用于 UI 调试）
npm run dev
```

## 项目结构

```
iris/
├── src/                    # React 前端源码
│   ├── components/
│   │   ├── editor/         # TipTap 编辑器扩展（自定义节点、Mark；非第三方插件）
│   │   ├── ai/             # AI 交互相关组件（面板、内联操作按钮）
│   │   ├── layout/         # 布局组件（侧边栏、标签栏、主区域）
│   │   └── ui/             # shadcn/ui 基础组件（按钮、输入框、对话框等）
│   ├── hooks/              # 自定义 React Hooks
│   ├── lib/                # 纯函数工具
│   │   ├── ipc.ts          # Tauri IPC 封装（类型安全的 invoke 包装）
│   │   └── utils.ts        # 通用工具函数
│   ├── types/              # TypeScript 类型定义
│   │   └── ipc.ts          # IPC 命令的请求/响应类型
│   ├── styles/             # 全局样式
│   └── App.tsx             # 应用根组件
├── src-tauri/              # Rust 后端源码
│   ├── src/
│   │   ├── main.rs         # Tauri 应用入口
│   │   ├── lib.rs          # 库入口，注册所有 modules
│   │   ├── commands/       # IPC 命令处理函数
│   │   ├── storage/        # SQLite 数据库层（连接管理、migration、CRUD）
│   │   ├── llm/            # LLM API 编排（请求构建、流式处理、提供商适配）
│   │   ├── embedding/      # 向量嵌入生成与检索
│   │   ├── indexer/        # Markdown 文件解析与索引更新
│   │   ├── crypto/         # AES-256-GCM 加密/解密
│   │   └── watcher/        # 文件系统事件监听
│   ├── migrations/         # SQLite 数据库迁移文件
│   ├── Cargo.toml
│   └── tauri.conf.json
├── docs/                   # 文档索引、Epic、design-system、eval
│   ├── README.md           # 全库文档入口
│   ├── design-system.md    # 纸墨 B / 命令 C
│   ├── v0.1.0-epic.md
│   ├── v0.1.0-completion-prs.md  # 已冻结
│   └── v0.1.1-epic.md      # 当前 Epic
├── AGENTS.md               # AI 开发规范
├── ARCHITECTURE.md         # 技术架构
├── ROADMAP.md              # 版本路线图（排期唯一来源）
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
cargo fmt --all -- --check        # 格式检查
cargo clippy --all-targets -- -D warnings  # Lint 检查（警告即错误）
cargo test                         # 运行 Rust 测试
cargo audit                        # 依赖安全审计

# 前端（npm；pnpm 将 `npm run` 换为 `pnpm` 即可）
npm run lint
npm run format:check
npm run typecheck
npm run test

# E2E（端到端）
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

### 范围（可选）

常用的范围包括：`editor`、`ai`、`storage`、`search`、`crypto`、`ui`、`ipc`、`docs`

### 示例

```
feat(editor): 添加 / 命令唤起 AI 功能菜单

实现了输入 / 时弹出 AI 命令选择面板，包括总结、翻译、改写等选项。
选中命令后自动在编辑器中插入 ai-stream 节点并开始流式请求。

关联 Issue: #42
```

```
fix(search): 修复中文分词后的搜索召回率下降

使用 jieba-rs 替代简单的字符级切分，IndexDB 重建命令: pnpm run index:rebuild

Closes #128
```

## 代码风格

### Rust

- 遵循 `cargo fmt` 的默认格式
- `cargo clippy` 警告视为错误，必须在提交前清除
- 禁止使用 `unsafe` 除非在代码注释中说明必要性，并在 PR 中标注待评审
- 公共 API 函数必须有文档注释（`///`）
- 模块内部使用 `pub(crate)` 而非 `pub`，除非确实需要对外暴露
- 异步函数使用 `tokio`，避免混用多个运行时

### TypeScript / React

- 遵循 ESLint 和 Prettier 配置（项目根目录的 `.eslintrc.cjs` 和 `.prettierrc`）
- 组件文件使用 PascalCase 命名，目录使用 kebab-case（例：`components/ai-panel/AiPanel.tsx`）
- Hooks 文件以 `use` 前缀命名（例：`useFileList.ts`）
- 类型定义优先使用 `interface`，联合类型使用 `type`
- 避免 `any`，必要时使用 `unknown` + 类型守卫
- 组件 props 必须显式定义接口，不使用 `React.FC` 泛型
- IPC 调用必须通过 `src/lib/ipc.ts` 中的类型安全封装，禁止直接 `invoke()`

### 命名约定

| 上下文           | 约定            | 示例           |
| ---------------- | --------------- | -------------- |
| React 组件       | PascalCase      | `AiPanel.tsx`  |
| 组件目录         | kebab-case      | `ai-panel/`    |
| Rust 源文件      | snake_case      | `file_ops.rs`  |
| Rust 类型/结构体 | PascalCase      | `FileMetadata` |
| Rust 函数/变量   | snake_case      | `read_file()`  |
| SQLite 表名      | snake_case 复数 | `file_tags`    |
| SQLite 列名      | snake_case      | `created_at`   |

## 测试

### 测试要求

- **新功能**：必须包含对应的测试
- **Bug 修复**：必须先写复现该 bug 的测试，确认测试失败后再修复，修复后测试通过
- **重构**：重构前后测试必须全绿

### Rust 测试

```bash
# 运行所有测试
cargo test

# 运行特定模块的测试
cargo test storage

# 运行包含特定关键词的测试
cargo test markdown_roundtrip
```

### 前端测试

```bash
npm run test
npm run test:watch
npm run test:coverage
```

### 端到端测试

```bash
npm run tauri build -- --debug
npm run test:e2e
```

## Pull Request 流程

1. 确保你的分支基于最新的 `main` 分支（先 `rebase`，不要 `merge`）
2. 通过所有本地检查（格式、Lint、类型检查、测试）
3. 在 GitHub 创建 Pull Request，使用 [PR 模板](./.github/PULL_REQUEST_TEMPLATE.md)
4. 在 PR 描述中关联相关 Issue（`Closes #123` 或 `Related to #123`）
5. 等待 CI 流水线通过
6. 至少需要一个维护者的 Code Review 批准
7. 批准后由维护者合并

### PR 标题格式

与 Commit 规范一致：`类型(范围): 描述`

示例：`feat(ai): 实现流式渲染 AI 生成内容`

### 新增依赖

在 PR 描述中说明新增依赖的理由和替代方案考虑。不设硬性审批门禁，但 maintainer 可能要求讨论。

## Issue 规范

- **Bug 报告**：使用 [Bug 报告模板](./.github/ISSUE_TEMPLATE/bug_report.md)
- **功能请求**：使用 [功能请求模板](./.github/ISSUE_TEMPLATE/feature_request.md)
- **安全漏洞**：**不要公开提交 Issue**，请参阅 [安全策略](./SECURITY.md)

## 问题求助

如果在开发过程中遇到问题：

1. 先搜索已有的 Issue 和 Discussion
2. 在 Discussion 中发起问答
3. 或在相应的 Issue 下评论

感谢你的贡献。
