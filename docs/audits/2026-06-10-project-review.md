# 2026-06-10 项目审计跟进

本文记录 2026 年 6 月项目审查后的实现跟进。当前事实来源仍为 README、ROADMAP、ARCHITECTURE 与 docs/README；docs/history 与 docs/superpowers 属于历史施工资料，可能与当前实现不一致。

## 本轮已修复

- `.iris` 与 `.classified` 保留目录统一按 ASCII 大小写不敏感方式拦截。
- 笔记读取改为严格 UTF-8，非法字节不再被替换为 U+FFFD。
- MiniMax API Host 保存时规范化为干净 HTTPS origin，并拒绝 userinfo、query、fragment、HTTP、空 host 与额外 path。
- `AppError::Message` 与 `AppError::Embed` 日志细节改为长度加短 SHA-256 摘要。
- 正式注册 migration 024，并新增 migration 025，在不依赖 sqlite-vec 的情况下补齐 scalar 知识表。
- LLM IPC helper 合并到 `src/lib/ipc.ts`，LLM 配置变更事件移动到 `src/lib/llm-events.ts`。
- 默认 HTTP client API 更名为 HTTPS-only/rustls；证书固定只保留为显式 opt-in API。
- 删除过时的 `@types/dompurify` dev dependency 与伪 `index:rebuild` npm script。
- `VaultNavigator` 拆出 dialog 组件与路径/命名纯函数模型，并补充 focused 单元测试。
- 升级 `scraper`、`fastembed`、`notify` 与 `notify-debouncer-full`，移除 `fxhash`、`number_prefix`、`instant` 三个 RustSec warning 来源。
- `fastembed` 改为 `default-features = false`，仅启用文本 embedding 所需的 rustls 下载/模型特性；移除无用 `image` / `ravif` / `rav1e` 依赖链。
- 新增 `.cargo/audit.toml` 作为唯一 RustSec 例外清单；`npm run audit:rust` 以 `--deny warnings` 运行 Cargo audit 并读取该配置。

## 已知剩余项

- `sqlite-vec` 仍为 optional/experimental；当前 Windows 环境下 feature 构建阻塞记录为非门禁 known issue。
- 未应用 `.cargo/audit.toml` 例外时，完整 lockfile 审计仍会报告 18 个 RustSec warning：GTK3 绑定相关 unmaintained、`glib` unsound warning、`paste`、`proc-macro-error` 与 `unic-*`。这些来源分别绑定 Tauri/wry Linux GTK3 backend、fastembed/tokenizers、Tauri `urlpattern 0.3`，当前无法在 Iris 内无风险升级消除。
- `UnifiedAssistantPanel` 与 `App.tsx` 的更深层状态下沉仍待作为单独的行为保持型重构推进；`VaultNavigator` 已完成低风险 dialog/model 拆分。
- `AGENTS.md` 命令速查已同步移除 `npm run index:rebuild` 旧入口，改为应用内 `search_reindex`。
