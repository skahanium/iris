# MiniMax 协议与发布门禁加固实施计划

> **执行说明：** 本计划在 `branch-1.2.20` 原地实施；遵循测试先行，不创建 worktree，不修改用户已有的 `.reasonix` 工作区文件。

**目标：** 修正 MiniMax 流式推理续轮、M2/M3 推理控制和隐私净化边界，明确自定义端点的 Agent 能力限制，并让桌面打包强制依赖同一提交的人工发布就绪门禁。

**技术边界：** 沿用现有 Rust model gateway、React 设置页和 GitHub Actions；不新增依赖、IPC、数据库实体或运行时协议。能力判定保持“内置端点显式声明、自定义端点保守降级”。

---

## 任务 1：MiniMax 流式续轮与控制标记净化

**文件：**

- 修改：`src-tauri/src/ai_runtime/model_gateway/streaming.rs`
- 修改：`src-tauri/src/ai_runtime/text_support.rs`

1. 先增加失败测试，覆盖 `reasoning_details` 累积快照不重复、增量片段可合并，以及流结束时部分 `<|minimax|>` 标记不外泄。
2. 运行定向 Rust 测试，确认测试因当前追加逻辑和 `done=true` 分支而失败。
3. 实现基于稳定标识的推理详情合并，并在无稳定标识时保守合并文本；结束路径仍暂存/丢弃部分 provider 控制后缀。
4. 将 `<|minimax|>` 注释改为防御性兼容说明，不宣称它来自公开协议文档。
5. 运行 model gateway 定向测试并确认通过。

## 任务 2：MiniMax M2/M3 能力契约

**文件：**

- 修改：`src-tauri/src/llm/config.rs`
- 修改：`src-tauri/src/llm/model_catalog.rs`
- 修改：`src-tauri/src/llm/provider_contract.rs`
- 修改：`src-tauri/src/commands/llm_config_commands.rs`
- 修改：`src-tauri/src/ai_runtime/model_gateway/body.rs`
- 修改：`src-tauri/src/ai_runtime/model_gateway/tests.rs`
- 修改：`src/components/settings/llmRoutingModelHelpers.ts`
- 修改：`tests/llm-reasoning-routing.test.ts`

1. 先增加失败测试：M3 只支持 `off/auto`，M2.x 为不可关闭的 `on`；自定义端点即使模型名含 MiniMax 也不冒充内置能力；M2 请求不发送无效的 `thinking: disabled`。
2. 运行 Rust 与前端定向测试，确认旧的粗粒度 MiniMax 判定失败。
3. 将能力推断限定为内置 `minimax` provider，并按 M3 与 M2.x 分离模式、默认值和 `disableSupported`。
4. 仅为 M3 发送 `thinking.type`；M2.x 仍发送 `reasoning_split=true` 并保持续轮推理详情回放。
5. 为工具循环测试夹具提供仅在 `cfg(test)` 下存在的显式已验证协议标识，不放宽生产自定义端点的 chat-only 回退。
6. 同步目录、旧配置升级和前端展示契约，运行定向回归测试。

## 任务 3：自定义端点限制的持续可见性

**文件：**

- 修改：`src/components/settings/LlmProviderDetail.tsx`
- 修改：`tests/llm-add-model-regression.test.tsx`
- 修改：`docs/design-system.md`
- 修改：`ROADMAP.md`

1. 先增加失败的组件测试，证明文本/视觉验证成功后限制提示目前会消失。
2. 在设计系统中明确：连通性/模型验证不等于 Agent 工具协议验证，自定义端点提示必须持续显示。
3. 调整模型摘要，使自定义端点在任何验证状态下都显示“仅支持对话；Agent 工具协议尚未验证”。
4. 在路线图记录未来显式端点能力探测计划，保持当前 chat-only 安全回退不变。
5. 运行设置页定向测试、lint 与类型检查。

## 任务 4：人工发布就绪门禁与文档

**文件：**

- 修改：`tests/github-actions-workflows.test.ts`
- 修改：`.github/workflows/ci.yml`
- 修改：`.github/workflows/package-desktop.yml`
- 修改：`CHANGELOG.md`
- 修改：`docs/README.md`

1. 先增加失败测试，要求打包同时找到同一 `main` SHA 的成功 `push` CI 与成功 `workflow_dispatch` 发布就绪运行。
2. 修改打包预检，通过 GitHub Actions API 分别验证两类运行；任一缺失都给出可操作错误并阻止打包。
3. 更新 CI 注释、当前版本变更日志和文档索引，明确人工门禁与安全降级事实。
4. 运行 workflow 契约测试与文档/格式检查。

## 任务 5：完整验证与交付审查

1. 运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `npm run audit:rust`。
2. 运行 `npm run lint`、`npm run format:check`、`npm run typecheck`、`npm run test` 与 `npm audit`。
3. 检查 `git diff --check`、变更范围及工作树，确认 `.reasonix` 用户改动未被触碰。
4. 复核测试证据与残余风险；不自动提交或推送。
