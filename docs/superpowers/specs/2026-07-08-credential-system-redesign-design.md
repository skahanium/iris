---
title: Iris 统一凭证体系重构设计
created: 2026-07-08
scope: credentials, llm-providers, mcp-providers, settings-ui, local encrypted credential store, security
status: ready-for-planning
---

# Iris 统一凭证体系重构设计

## 背景

Iris 当前的 API Key 体系把几件事情耦合在一起：本地加密凭据中的真实 Key、SQLite 中的 configured marker、LLM provider 配置生命周期、MCP provider 绑定、macOS user-presence 解锁策略，以及旧的 bundle 存储。这个组合已经导致了两个用户可见问题：

- 删除或重建 provider 配置时，可能顺带删除 API Key，用户误以为只是改配置。
- UI 只看 marker，不验证真实凭据条目；marker 和本地加密凭据一旦漂移，就会显示“Key 已配置”但运行失败，或反过来误报“凭据不存在”。

本设计把凭据体系重做成一个清晰、安全、低打扰的基础设施层。Iris 仍然必须在运行时把 API Key 作为 HTTP header 或 MCP 环境变量读取出来使用，但任何普通 UI、IPC、日志、Agent 工具、session、checkpoint 都不得查看、返回、保存或复制明文 Key。

## 目标

- LLM 与 MCP API Key 采用统一 Credential Manager 管理，但按 service 独立存储。
- 每个 provider 一个独立本地加密凭据条目：
  - LLM：`iris.llm.<providerId>`
  - MCP：`iris.mcp.<providerId>`
- 不再使用统一 API Key bundle，也不再使用 macOS `system password prompt` / Touch ID/system password prompt 作为读取门槛。
- 不提供“查看密钥”“复制密钥”“导出密钥”能力；保存后只能覆盖、检测、清除。
- 保存新 Key 是 upsert 覆盖语义：同一 service 的旧 Key 被新 Key 替换，不保留历史版本。
- 删除 provider 配置时不默认删除 Key；只有二次确认或显式“清除 Key”才删除凭据。
- 不背历史迁移债务。旧 bundle、旧 fallback、旧系统解锁分支、旧 marker 兼容逻辑应被清理，而不是继续延续。

## 非目标

- 不从旧 `iris.dev.api_keys` / `iris.api_keys` bundle 自动迁移历史 Key。
- 不尝试恢复已经丢失、已从服务商网站删除或已被本地加密凭据删除的 Key。
- 不新增“显示 API Key 明文”的高级入口。
- 不把凭据写入 SQLite、配置 JSON、日志、trace、session message 或 checkpoint。
- 不改变 LLM/MCP provider 的业务配置模型，除非它们依赖危险的凭据删除语义。

## 凭据模型

Credential Manager 以 service 为唯一主键：

```rust
pub enum CredentialKind {
    Llm,
    Mcp,
}

pub struct CredentialService {
    pub kind: CredentialKind,
    pub provider_id: String,
    pub service: String,
}
```

服务名规则：

- `llm_credential_service("deepseek") == "iris.llm.deepseek"`
- `mcp_credential_service("anysearch") == "iris.mcp.anysearch"`
- service 必须只允许 ASCII 小写字母、数字、`.`、`_`、`-`，并且必须以 `iris.llm.` 或 `iris.mcp.` 开头。
- 前端不得手写 service 拼接逻辑散落在组件中；统一走 `llmCredentialService(providerId)` / `mcpCredentialService(providerId)`。

凭据状态只表达可操作事实，不暴露明文：

```ts
type CredentialStatus = "available" | "missing";

interface CredentialStatusDto {
  service: string;
  status: CredentialStatus;
  configured: boolean;
  checkedAt: string;
}
```

状态含义：

- `available`：本地加密凭据条目存在，且 Iris 后端能读取到非空值。
- `missing`：本地加密凭据条目不存在，或值为空。
- 不再把 SQLite marker 当作权威状态。若仍保留 marker，只能作为 UI 加速缓存；任何状态检查必须以本地加密凭据条目为准。

## 存储策略

每个 API Key 是一个独立本地加密凭据条目：

| 类型       | 示例 service         | 存储位置                         | SQLite 内容                              |
| ---------- | -------------------- | -------------------------------- | ---------------------------------------- |
| LLM        | `iris.llm.deepseek`  | Iris local encrypted credentials | 可选非敏感状态缓存                       |
| LLM custom | `iris.llm.custom`    | Iris local encrypted credentials | 可选非敏感状态缓存                       |
| MCP        | `iris.mcp.anysearch` | Iris local encrypted credentials | provider 绑定 JSON 中只保存 service 引用 |

macOS 策略：

- 使用普通 local encrypted credential store generic password 条目。
- 不设置 `system password prompt`，避免每次调用 LLM/MCP 都弹密码或指纹。
- 不提供明文读取 IPC，降低本机恶意 UI 或 Agent 通过 Iris 偷看 Key 的风险。

替换策略：

- `credential_set(service, value)` 是 upsert。
- 若 service 已存在，旧值被新值覆盖。
- 旧值不写入历史、不写入回收站、不写入日志。

删除策略：

- `credential_delete(service)` 只删除该 service 的当前值。
- 删除 LLM/MCP provider 配置时，UI 必须给独立二次确认：
  - 默认：只移除 provider 配置，保留 Key。
  - 勾选或二次确认：同时清除对应 Key。
- 若保留 Key 后 provider 已不存在，凭据管理区显示为“未绑定凭据”，允许用户手动清理。

## API 与边界

后端保留或重命名现有 IPC，但语义必须收敛：

```rust
#[tauri::command]
pub fn credential_set(service: String, value: String) -> AppResult<CredentialStatusDto>;

#[tauri::command]
pub fn credential_status(service: String) -> AppResult<CredentialStatusDto>;

#[tauri::command]
pub fn credential_delete(service: String) -> AppResult<CredentialStatusDto>;
```

废弃：

- `credential_has` 的 marker-only 语义。
- `credential_unlock_session` / `credential_lock_session` 对 API Key bundle 的作用。
- `secret_read_plaintext` 工具能力声明。
- bundle 读写、bundle cache、legacy secret fallback。

内部读取 API 仅供 LLM/MCP 调用路径使用：

```rust
pub fn get_runtime_secret(service: &str) -> AppResult<Zeroizing<String>>;
```

要求：

- 返回值必须使用 `Zeroizing<String>` 或等价零化包装。
- 调用者只能把值放入请求 header、MCP env 或 provider runtime config。
- 错误消息不得包含 Key 值、请求 header 值或 provider 返回的敏感 body。

Agent 工具边界：

- `secret_exists` 改为 `credential_status` 的只读状态检查，不读取明文。
- Agent 不支持创建、更新、读取、复制密钥。
- 如果未来允许 Agent 帮用户配置凭据，也只能打开 UI 或生成待确认表单，不能接收 Key 明文。

## UI 行为

LLM 设置页：

- 每个 provider 卡显示：
  - provider 名称、service id、Key 状态。
  - password 输入框只用于保存新 Key。
  - “保存 Key”覆盖当前 service 的 Key。
  - “清除 Key”独立危险按钮，需确认。
  - “移除配置”不再叫 `Delete`，并显示“是否同时清除 Key”的二次确认。
- 内置 provider 和 custom provider 行为一致；custom provider 的 label 不影响 service id。
- 例如 label 为 “Minimax” 的 custom provider 仍显示 service `iris.llm.custom`，避免用户误以为它使用 `iris.llm.minimax`。

MCP 设置页：

- provider 表单保存 credential refs 时，只保存 `credential://iris.mcp.<providerId>` 或等价 service 字符串。
- 保存 MCP provider 前先写本地加密凭据，再写 provider 配置；如果 provider 配置保存失败，UI 提示 Key 已保存但 provider 配置失败，并提供清理按钮。
- 诊断显示 credential status，不显示 Key。

全局凭据管理区：

- 列出 LLM/MCP 已知 service 的状态。
- 显示未绑定凭据，允许清除。
- 不提供查看或复制。

## 安全约束

- API Key 明文只允许存在于：
  - 用户输入框的当前 React state / DOM password field。
  - 后端 `credential_set` 入参调用期间。
  - 后端 `get_runtime_secret` 返回的 zeroizing 内存。
  - 单次 HTTP request header 或 MCP child process env 构造期间。
- 禁止出现在：
  - SQLite。
  - Tauri logs / tracing。
  - IPC 返回值。
  - Agent tool output。
  - session message、checkpoint、trace、tool audit。
  - crash-safe diagnostic payload。
- 测试必须扫描常见泄漏路径，确认不会写入示例 Key。

## 错误处理

- `missing`：提示“未保存 Key”，允许用户保存。
- - provider API 返回 401：提示“API Key 无效或未授权”，但不自动删除本地 Key。
- 保存新 Key 后应立即做 status check；若 status 不是 `available`，保存动作返回失败。

## 清理旧体系

实现时应删除旧架构，而不是兼容保留：

- 删除 `ApiKeyBundle`、`API_KEY_BUNDLE_SERVICE`、`API_KEY_BUNDLE_CACHE`。
- 删除 macOS `macos_protected_local encrypted credential store` user-presence 分支。
- 删除 `get_legacy_api_key_secret`。
- 删除 slash legacy service fallback，service 必须 canonical。
- 删除 marker-only `credential_has` 调用路径，改为真实 status。
- 删除 `credential_unlock_session` / `credential_lock_session` 中与 API Key 相关的逻辑；它们可继续服务 CAS/涉密 vault，但不能再影响 LLM/MCP Key。

## 验收标准

- DeepSeek 与 custom/Minimax 各自使用不同 service，状态互不影响。
- 重新保存 DeepSeek Key 会覆盖 `iris.llm.deepseek` 的旧值。
- 删除 DeepSeek provider 配置默认保留 `iris.llm.deepseek`；选择“同时清除 Key”才删除。
- LLM 和 MCP provider 都能使用对应 Key 发起请求，但 UI/IPC/日志无法查看 Key。
- macOS 上保存后调用 LLM/MCP 不再反复要求输入系统密码或指纹。
- 完整测试通过；安全扫描确认没有示例 Key 泄漏到 SQLite、日志、session 或 trace。
