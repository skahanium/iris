# Iris

> 本地优先的 Markdown 笔记应用：笔记归你，AI 在编辑器里协作。

Iris 是基于 Tauri 2、Rust 和 React 的桌面 Markdown 笔记应用。用户笔记始终是标准 UTF-8 `.md` 文件；SQLite 只保存可从笔记重建的索引，以及会话、缓存等运行时状态。

**当前开发版本**：v1.2.11。 版本排期的唯一来源是 [ROADMAP.md](./ROADMAP.md)。

## 核心能力

- 本地 Markdown 编辑：TipTap 编辑器、GFM、链接、标签、版本快照、文件树与全文搜索。
- 本地知识检索：FTS5、向量嵌入、显式链接与规则索引共同为搜索和 AI 上下文服务。
- 编辑器内 AI：选区操作、`/` 命令、统一助手面板；所有写入都经过用户确认。
- 可审计的数据边界：笔记不被转为专有格式；远程模型请求只使用 HTTPS。

## 安全与数据边界

API Key 不写入笔记、SQLite、日志或环境变量。Iris 采用本地 **AES-256-GCM** 加密凭据存储：主密钥和密文分置于平台配置目录与应用数据目录，解密值由 `Zeroizing` 承载并在释放时清零。项目刻意不使用操作系统凭据管理器，以避免系统密码弹窗打断正常使用。细节见 [SECURITY.md](./SECURITY.md)。

Iris 不提供通用插件 API、插件市场或任意代码执行能力。Skills 仅是由 Iris 创建、用户确认后启用的 prompt-only `SKILL.md` 行为包；不安装 URL/Git/本地包，不承载 MCP、脚本或依赖。

## 技术栈

- 桌面：Tauri 2.x + Rust
- 前端：React 19 + TailwindCSS + shadcn/ui
- 编辑：TipTap / ProseMirror
- 数据：SQLite、FTS5 与可选 sqlite-vec 加速
- 嵌入：fastembed（当前基线为 AllMiniLML6V2；v1.2.6 计划迁移至 BGE-small-zh-v1.5）

## 开发

```bash
npm ci
npm run tauri dev
npm run tauri build
```

提交前执行：

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

发布版本必须使用受控脚本：

```bash
npm run version:set -- 1.2.6
npm run version:check
```

## 文档

- [路线图](./ROADMAP.md)：唯一版本排期来源
- [架构](./ARCHITECTURE.md)：模块、数据流和安全边界
- [变更记录](./CHANGELOG.md)：已完成版本的用户可见变更
- [文档索引](./docs/README.md)
- [贡献规范](./AGENTS.md)

## 许可

[GNU Affero General Public License v3.0](./LICENSE)，Copyright (C) 2026 Iris contributors。
