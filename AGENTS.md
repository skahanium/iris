# AGENTS.md — AI 开发规范

本文档是 Iris 项目中所有 AI 辅助开发（包括 GitHub Copilot、Claude、Cursor、以及其他 AI 编码工具）必须遵守的行为规范。人类开发者同样适用。

违反本规范的代码不得合并。

---

## 一、硬约束

以下规则不可绕过，无例外。

### 1.1 技术栈锁定

| 层级     | 技术                    | 禁止替代                                        |
| -------- | ----------------------- | ----------------------------------------------- |
| 桌面框架 | Tauri 2.x               | 禁止 Electron、Flutter Desktop、.NET MAUI       |
| 后端语言 | Rust                    | 禁止 Node.js 后端、Python 后端                  |
| 前端框架 | React 19                | 禁止 Vue、Svelte、Solid、Angular                |
| 编辑器   | TipTap (Prosemirror)    | 禁止 Slate.js、Lexical、Quill、自研编辑器       |
| 样式     | TailwindCSS + shadcn/ui | 禁止 CSS Modules、styled-components、Ant Design |
| 数据库   | SQLite + sqlite-vec     | 禁止 PostgreSQL、LanceDB、Qdrant                |

### 1.2 许可合规

- 所有新增依赖必须与 AGPL-3.0 兼容
- 禁止引入任何 GPL-incompatible 或商业授权组件
- Rust crate 检查其 `license` 字段，npm 包检查 `license` 字段

### 1.3 数据原则

用户的 `.md` 文件是笔记知识的最权威来源。SQLite 承担双重角色：

- **笔记索引层**：`files`、`chunks`、`links` 等表的数据派生自 `.md` 文件，删除后可从 `.md` 完全重建。
- **应用状态层**：AI 会话记录、网页缓存、收件箱条目等属于应用运行时状态或外部获取的补充数据，不以 `.md` 为重建来源，删除后不影响笔记完整性。

- 禁止将笔记内容存储为专有格式
- 禁止在不经用户明确同意的情况下修改 `.md` 文件内容
- `.md` 文件必须始终保持为合法 UTF-8 编码的纯文本

### 1.4 安全红线

- **API Key**: 禁止写入任何明文文件、日志、数据库或环境变量。必须使用操作系统凭据管理器存储（Windows Credential Manager / macOS Keychain / Linux Secret Service）。
- **日志**: 禁止在日志、调试输出、错误消息中输出 API Key、Token、用户笔记内容、加密密码
- **硬编码**: 禁止在源代码中硬编码任何凭证、密钥、端点 URL 或敏感配置
- **传输**: 所有 LLM API 请求必须走 HTTPS，禁止 HTTP 明文传输

### 1.5 Rust `unsafe`

- 禁止使用 `unsafe` 代码块，除非同时满足以下条件：
  1. 在代码注释中明确说明为何 `safe` 替代方案不可行
  2. 在 PR 描述中标注 "含 unsafe 代码" 并说明必要性
  3. 经过至少一个 maintainer 的专门评审

## 二、代码规范

### 2.1 Rust

```bash
# 所有提交前必须通过：
cargo fmt --all -- --check      # 格式检查（标准 Rust 风格）
cargo clippy --all-targets -- -D warnings  # Lint 检查，警告即错误
```

- clippy 级别: `clippy::all`
- 未使用的 import、变量、mut 标记视为错误
- 公共 API 函数必须有 `///` 文档注释
- 模块内部优先使用 `pub(crate)` 而不是 `pub`
- 异步运行时统一使用 `tokio`，禁止混用其他运行时

### 2.2 TypeScript / React

```bash
# 所有提交前必须通过：
npm run lint            # ESLint
npm run format:check    # Prettier
npm run typecheck       # TypeScript 类型检查
```

- 组件文件: PascalCase（如 `AiPanel.tsx`）
- 组件目录: kebab-case（如 `components/ai/`）
- Hooks 文件: `use` 前缀（如 `useFileList.ts`）
- 类型定义: 优先 `interface`，联合类型用 `type`
- 禁止 `any`，必须使用 `unknown` + 类型守卫
- 组件 Props 必须显式定义接口，不使用 `React.FC`
- IPC 调用必须通过 `src/lib/ipc.ts` 的类型安全封装，禁止直接 `invoke()`

### 2.3 文件命名

| 上下文           | 约定            | 示例           |
| ---------------- | --------------- | -------------- |
| React 组件文件   | PascalCase      | `AiPanel.tsx`  |
| React 组件目录   | kebab-case      | `ai/`          |
| Rust 源文件      | snake_case      | `file_ops.rs`  |
| Rust 类型/结构体 | PascalCase      | `FileMetadata` |
| Rust 函数/变量   | snake_case      | `read_file()`  |
| SQLite 表名      | snake_case 复数 | `file_tags`    |
| SQLite 列名      | snake_case      | `created_at`   |

### 2.4 Commit 规范

使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)，消息语言使用**中文**。

格式：

```
<类型>(<范围>): <描述>

<可选的详细描述>

<可选的脚注>
```

类型：`feat` | `fix` | `docs` | `style` | `refactor` | `perf` | `test` | `chore` | `ci`

范围（常用）：`editor` | `ai` | `storage` | `search` | `crypto` | `ui` | `ipc` | `docs`

示例：

```
feat(editor): 添加 / 命令唤起 AI 功能菜单

Closes #42
```

## 三、施工纪律

### 3.1 修改前必读上下文

修改任何函数、类型、组件之前：

1. 阅读该文件的完整内容
2. 搜索该函数/组件的所有调用处
3. 理解它在整体数据流中的位置

禁止只读 30 行上下文就开始修改。

### 3.2 禁止凭空造轮子

引入新依赖或手写工具函数之前：

1. 搜索代码库中是否已存在类似实现
2. 检查现有依赖是否已提供该功能
3. 在 PR 描述中说明为何复用不可行

### 3.3 测试先行

- **新功能**: 必须先写测试，测试失败后再写实现，测试通过后功能才算完成
- **Bug 修复**: 必须先写复现该 bug 的测试，确认测试确实失败，修复后测试通过
- **重构**: 重构前后测试必须全部通过

### 3.4 验证后才能声称完成

在声称任何 "完成了" "修复了" "通过了" 之前，必须：

1. 运行完整的 lint / format / typecheck / test 命令
2. 确认所有命令的输出无错误
3. 如果 CI 配置了额外的检查，等待 CI 通过

禁止凭感觉声称完成。

### 3.5 新增依赖

- 在 PR 描述中明确说明为何需要新增该依赖
- 列举考虑过的替代方案及不选择的原因
- 不设硬性审批门禁，但 maintainer 可能要求讨论

## 四、项目特定规则

### 4.1 编辑器同步逻辑

修改 TipTap schema（节点类型、Mark 类型、序列化/反序列化）时：

- 必须同步更新 Markdown 往返测试套件（round-trip tests）
- 新增节点类型必须覆盖 `parse → Node Tree → serialize` 的完整往返
- 如果序列化输出与标准 Markdown 有差异，必须在 schema 文档注释中说明

### 4.2 IPC 接口契约

修改 Tauri `#[tauri::command]` 的函数签名（参数名、类型、返回值）时：

- 必须同步更新 `src/types/ipc.ts` 中的 TypeScript 类型定义
- 必须同步更新 `src/lib/ipc.ts` 中的类型安全封装函数
- 前后端类型不一致会导致编译通过但运行时崩溃

### 4.3 数据库迁移

- SQLite schema 变更必须走增量 migration
- Migration 文件放在 `src-tauri/migrations/` 目录，按版本号命名（如 `002_add_chunks.sql`）
- 禁止手动修改 schema 后要求用户删除数据库重建
- 每次 migration 必须有对应的 `down` 回滚脚本

### 4.4 前端组件组织

- `components/ui/`: 仅存放 shadcn/ui 基础组件和共享 UI 原语，不得包含业务逻辑
- `components/editor/`: 编辑器相关组件
- `components/ai/`: AI 交互相关组件
- `components/layout/`: 布局组件
- 禁止在 UI 基础组件中添加业务逻辑（如 API 调用、数据库查询）
- 禁止在业务组件中重复造 shadcn/ui 已有的轮子

### 4.5 Markdown 解析器兼容性

升级 Markdown 解析器（`markdown-it`、`remark` 等）版本时：

- 必须确保旧格式笔记可正常打开和渲染
- 必须通过完整的 round-trip test suite
- 如果新版本引入了 breaking change，必须在 PR 中注明并提供迁移方案

### 4.6 文档

- **版本排期唯一来源**：[ROADMAP.md](./ROADMAP.md)
- **界面 token 与组件规范**：[docs/design-system.md](./docs/design-system.md)
- **文档索引**：[docs/README.md](./docs/README.md)
- 修改 UI：先 design-system + ROADMAP 对应节，再 `src/styles/globals.css` 与组件
- 勿在 `ARCHITECTURE.md` 中新增与 ROADMAP 冲突的版本承诺

## 五、命令速查

### 开发

```bash
npm run tauri dev          # 启动完整开发环境（Rust 后端 + React 前端热更新）
npm run dev                # 仅启动前端开发服务器
npm run tauri build        # 构建生产版本
npm run tauri build --debug # 构建 Debug 版本（用于 E2E 测试）
```

### Rust 质量检查

```bash
cargo fmt --all -- --check               # 格式检查
cargo fmt --all                          # 自动格式化
cargo clippy --all-targets -- -D warnings # Lint 检查
cargo clippy --all-targets --fix         # 自动修复
cargo test                               # 运行所有 Rust 测试
cargo audit                              # 依赖安全审计
```

### 前端质量检查

```bash
npm run lint            # ESLint 检查
npm run lint:fix        # ESLint 自动修复
npm run format:check    # Prettier 格式检查
npm run format          # Prettier 自动格式化
npm run typecheck       # TypeScript 类型检查
npm run test            # 运行前端测试
npm run test:watch      # 监听模式
npm run test:coverage   # 覆盖率报告
```

### 其他

```bash
npm audit                           # 前端依赖安全审计
# 数据库迁移在应用启动时自动执行（storage::migrate::migrate_up）
# 重建索引：应用内 search_reindex 或 npm run index:rebuild 说明
npm run test:e2e                    # 运行端到端测试
```

## 六、AGENTS.md 修订规则

本文档的修改遵循以下流程：

1. **发起**: 任何贡献者可以在 Issue 中提出修改建议，说明修改理由、影响范围和预期效果
2. **讨论**: 修改提议需要经过社区讨论，至少达到粗略共识
3. **执行**: PR 关联对应的 Issue，描述中引用 Issue 编号
4. **批准**:
   - 仅有一位 maintainer 时：maintainer 自行 approve
   - 有多位 maintainer 后：至少一位其他 maintainer approve
5. **记录**: 每次修订在 commit 中说明变更内容，CHANGELOG 中记录

---

_最后更新: 2026 年 5 月（v1.0.0-alpha；文档体系与当前基线对齐）_
