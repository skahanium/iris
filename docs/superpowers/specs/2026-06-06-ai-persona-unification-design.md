# AI 人格体系统一设计

**日期**: 2026-06-06  
**状态**: 已实施

## 背景

AI 助手相关配置原先分散在三处：

- `AssistantIdentity`（localStorage）：侧栏称呼/头像，不影响模型
- `PromptProfile`（SQLite）：人格描述/写作风格，注入 system prompt
- `AgentStatusBadge`（侧栏按钮）：只读 Skills/安全策略，误标为「人格」

用户无法在侧栏配置人格，且展示与行为人格不同源。

## 决策

1. **单一数据源**：扩展 `PromptProfile`，增加 `display_name`、`avatar_emoji`
2. **侧栏只读**：头像 + 称呼 + Agent 状态条；历史 / 新对话按钮
3. **配置入口**：设置 →「打开人格配置」→ 独立 `PersonaSettingsPanel` 浮层
4. **工具审计**：侧栏移除按钮；活跃 harness 时在全局底栏显示「工具审计」链接

## 数据模型

```typescript
interface PromptProfileDto {
  display_name: string;      // 默认「砚」
  avatar_emoji: string | null;
  persona: string;
  writing_style: string;
  custom_rules: string[];
  language: string;
}
```

### persona_resolver 规则

- `persona` 为空：默认 identity 使用 `display_name`（空则「砚」）
- `persona` 非空：用户 `persona` 文本为主，`display_name` 仅驱动侧栏 UI

## 迁移

首次 `usePromptProfile` 加载时，若 `display_name` 仍为默认且 localStorage 存在 `iris-assistant-identity`，合并后写入 SQLite 并清除 legacy key。

## 组件

| 组件 | 职责 |
|------|------|
| `AssistantPersonaDisplay` | 侧栏只读头像 + 称呼 |
| `AgentStatusStrip` | 侧栏只读 Skills 数 + 联网状态 |
| `PersonaSettingsPanel` | 统一人格配置浮层 |
| `usePromptProfile` | 读写 PromptProfile + 事件同步 |

## 废弃

- `AgentStatusBadge.tsx`
- `AssistantIdentitySection.tsx`
- `PromptProfileSection.tsx`
- `useAssistantIdentity.ts`
- `assistant-identity.ts`
