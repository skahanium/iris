# Agent Skills 安装与管理 — 设计规格

**日期**: 2026-06-06  
**状态**: 已实现

## 目标

用户在 AI 侧栏用自然语言（含 SkillHub 推荐话术）即可安装、查看、启停、卸载 Skills；与 Skills 面板共用同一 Rust 后端；工具确认后续聊稳定（`reasoning_content` 回传）。

## 架构

```text
用户 → Harness Agent
  ├─ skills_list（只读，自动执行）
  ├─ skills_install / uninstall / toggle（需确认）
  │    └─ SkillInstallService（IPC 与 tool dispatch 共用）
  │         ├─ skill_registry（SkillHub → InstallSpec）
  │         ├─ ~/.iris/skills 或 vault/.iris/skills
  │         ├─ skill_install_sources / skill_activation_index
  │         └─ emit skills:changed
  └─ ToolConfirmDialog preview → 用户批准 → checkpoint 续聊（含 reasoning_content）
```

## reasoning_content

- `LlmMessage.reasoning_content` 持久化于 checkpoint
- `messages_for_api` 在 assistant + tool_calls 时回传该字段
- `GatewayRequest.thinking` 为 true 时发送 `{ "thinking": { "type": "enabled" } }`

## SkillInstallService

| 函数 | 说明 |
|------|------|
| `list_skills` | 扫描 global + vault |
| `install_skill` | url / git / local / registry |
| `uninstall_skill` | 删除目录 + DB 记录 |
| `toggle_skill` | skills-config.json |
| `preview_install` | 确认框预解析 |

安装成功后：写入 `skill_install_sources`、刷新 `skill_activation_index`、默认启用、emit `skills:changed`。

## SkillHub 注册表

- API 基址：`https://api.skillhub.tencent.com`
- 输入：skill 名、`https://skillhub.cn/skills/{slug}`；拒绝 `/install/` 指南页
- 解析：优先 `repo_url` → git；否则 `GET /api/v1/skills/{slug}/file?path=SKILL.md` → url 安装

## Agent 工具

| 工具 | 确认 | access_level |
|------|------|--------------|
| skills_list | 否 | ReadIndex |
| skills_install | 是 | ManageSkills |
| skills_uninstall | 是 | ManageSkills |
| skills_toggle | 是 | ManageSkills |
| skills_read_resource | 否 | ReadIndex |

Meta 工具不受 skill allowlist 限制；registry 安装不受 `web_search_enabled` 限制。

## Skills 运行时语义

| 用户可见 | Harness 实际行为 |
|----------|------------------|
| SkillsPanel「已启用」 | `skills-config.json` 未 disable |
| 「本场景注入」 | `rank_skills_for_scene` score>0 且 enabled → prompt 注入 + tool 扩权 |
| `skill_activation_index` | 安装时写入 keywords/embedding；运行时优先用于匹配 |
| `allowed-tools` | 与 ToolCatalog 求交；未识别工具 UI 警告 |
| `references/` 等资源 | 通过 `skills_read_resource` 按需读取，不预加载 |

**能力边界**：Skills 通过 Markdown 指令引导模型使用 **ToolCatalog 已有**工具；不动态注册 Rust 工具、不执行任意脚本。依赖 catalog 外工具名的 skill 安装后会在 UI 显示 warning。

## 手工验收

1. SkillHub 话术 → Agent 调 `skills_install(registry)`，非仅 `fetch_web_page`
2. 确认框展示解析来源 → 批准 → Skills 面板与 `skills_list` 一致；侧栏出现「已安装 Skill…」提示
3. 批准 `fetch_web_page` 或 `skills_install` 后 harness 续聊无 400
4. `skills_uninstall` / `skills_toggle` 确认后生效
5. 拒绝安装后 Agent 正常继续
6. 联网关闭时 registry 安装仍可用（HTTPS 抓取不受 web_search 开关影响）

## 自动化测试覆盖

- `build_chat_completions_body`：tool confirm 后续聊 body 含 `reasoning_content` + `thinking`
- `SkillInstallService`：local 安装写入 `skill_install_sources` / `skill_activation_index`
- `skill_registry`：SkillHub JSON fixture + `SkillRegistryAdapter` 注册表
- `tool_policy`：meta skills 工具在 web_search 关闭时仍可用
- TS：`skill-install-notice.test.ts`、`tool-confirm-dialog` preview UI
