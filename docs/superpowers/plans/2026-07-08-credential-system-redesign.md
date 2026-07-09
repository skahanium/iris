# Iris Credential System Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Iris's bundled/marker-based API Key handling with a clean per-service credential system for LLM and MCP providers.

**Architecture:** Store every LLM/MCP API Key as its own Iris local encrypted credential entry keyed by `iris.llm.<providerId>` or `iris.mcp.<providerId>`. SQLite stores only provider config and non-sensitive status hints; runtime code reads secrets only for outbound LLM/MCP calls through a zeroizing internal API. UI offers save/overwrite, status, clear, and delete-provider-with-optional-key-delete flows, but never view/copy/export.

**Tech Stack:** Tauri 2.x, Rust, local encrypted credential store crate, SQLite existing settings/provider tables, React 19, TypeScript, TailwindCSS + shadcn/ui, Vitest, Cargo tests.

---

## File Structure

- Modify `src-tauri/src/credentials.rs`: replace API key bundle/user-presence code with per-service local encrypted credential store entries, status DTOs, zeroizing runtime reads, and delete/upsert semantics.
- Modify `src-tauri/src/commands/settings.rs`: replace marker-only `credential_has` with real `credential_status`; make set/delete return status.
- Modify `src-tauri/src/security/ipc_policy.rs`: tighten credential service validation to canonical `iris.llm.*` and `iris.mcp.*` IDs.
- Modify `src-tauri/src/llm/config.rs`, `src-tauri/src/llm/engine.rs`, and `src-tauri/src/ai_runtime/mcp_host_runtime.rs`: read runtime secrets through the new zeroizing API.
- Modify `src-tauri/src/commands/llm_config_commands.rs`: deleting provider config must accept an explicit `deleteCredential` flag.
- Modify `src-tauri/src/ai_runtime/tool_catalog/boundary.rs` and `src-tauri/src/ai_runtime/tool_dispatch/boundary.rs`: remove plaintext secret tool declarations and make `secret_exists` use real status.
- Modify `src/lib/credentials.ts`, `src/lib/ipc.ts`, `src/types/ipc.ts`: add typed status APIs and MCP service helpers.
- Modify `src/components/settings/LlmRoutingSection.tsx` and `src/components/ai/skills/McpProfilesPanel.tsx`: use status API, expose service IDs, clarify overwrite/clear/delete behavior.
- Add/modify tests under `tests/` and Rust module tests in touched files.

## Task 1: Write Failing Credential Core Tests

**Files:**

- Modify: `src-tauri/src/credentials.rs`
- Modify: `src-tauri/src/security/ipc_policy.rs`

- [ ] **Step 1: Add service validation tests**

Add tests that lock down canonical service IDs:

```rust
#[test]
fn credential_service_accepts_only_canonical_llm_and_mcp_ids() {
    validate_credential_service("iris.llm.deepseek").unwrap();
    validate_credential_service("iris.llm.custom_2").unwrap();
    validate_credential_service("iris.mcp.anysearch").unwrap();
    assert!(validate_credential_service("iris/llm/deepseek").is_err());
    assert!(validate_credential_service("iris.llm.").is_err());
    assert!(validate_credential_service("iris.llm.deepseek secret").is_err());
    assert!(validate_credential_service("evil.llm.deepseek").is_err());
}
```

- [ ] **Step 2: Add credentials unit tests**

Add tests in `credentials.rs` using a fake in-memory backend helper introduced inside `#[cfg(test)]`:

```rust
#[test]
fn set_api_key_overwrites_existing_service_without_history() {
    let store = TestCredentialStore::default();
    let first = store.set_api_key("iris.llm.deepseek", "old-key");
    assert!(first.is_ok());
    let second = store.set_api_key("iris.llm.deepseek", "new-key");
    assert!(second.is_ok());
    assert_eq!(store.get_runtime_secret("iris.llm.deepseek").unwrap().as_str(), "new-key");
    assert_eq!(store.entry_count("iris.llm.deepseek"), 1);
}

#[test]
fn delete_api_key_removes_only_requested_service() {
    let store = TestCredentialStore::default();
    store.set_api_key("iris.llm.deepseek", "deepseek-key").unwrap();
    store.set_api_key("iris.llm.custom", "custom-key").unwrap();
    store.delete_api_key("iris.llm.deepseek").unwrap();
    assert_eq!(store.status("iris.llm.deepseek").status, CredentialState::Missing);
    assert_eq!(store.status("iris.llm.custom").status, CredentialState::Available);
}
```

- [ ] **Step 3: Run tests and confirm they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml credential_service_accepts_only_canonical_llm_and_mcp_ids --lib
cargo test --manifest-path src-tauri/Cargo.toml set_api_key_overwrites_existing_service_without_history --lib
cargo test --manifest-path src-tauri/Cargo.toml delete_api_key_removes_only_requested_service --lib
```

Expected: FAIL because the new status/store abstractions do not exist yet and slash-style legacy IDs are still accepted.

## Task 2: Replace Bundle Storage With Per-Service local encrypted credential store Entries

**Files:**

- Modify: `src-tauri/src/credentials.rs`

- [ ] **Step 1: Remove old API Key bundle code**

Delete these concepts from `credentials.rs`:

```rust
ApiKeyBundle
API_KEY_BUNDLE_SERVICE
API_KEY_BUNDLE_CACHE
read_api_key_bundle_uncached
read_api_key_bundle_cached
store_api_key_bundle
get_legacy_api_key_secret
macos_protected_local encrypted credential store
```

Keep CAS encryption secret helpers if they still use generic `set_secret/get_secret`; this task only removes LLM/MCP API key bundle behavior.

- [ ] **Step 2: Add public status types**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Available,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusDto {
    pub service: String,
    pub status: CredentialState,
    pub configured: bool,
    pub checked_at: String,
}
```

- [ ] **Step 3: Implement canonical service validation helper**

Add:

```rust
fn canonical_service_id(service: &str) -> AppResult<String> {
    crate::security::ipc_policy::validate_credential_service(service)?;
    Ok(service.trim().to_string())
}
```

Do not replace `/` with `.`. Non-canonical IDs must fail.

- [ ] **Step 4: Implement per-service operations**

Implement:

```rust
pub fn set_api_key(service: &str, value: &str) -> AppResult<CredentialStatusDto> {
    let service = canonical_service_id(service)?;
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::msg("API Key 不能为空"));
    }
    entry_canonical(&service)?.set_password(value)?;
    credential_status(&service)
}

pub fn get_runtime_secret(service: &str) -> AppResult<Zeroizing<String>> {
    let service = canonical_service_id(service)?;
    let value = entry_canonical(&service)?.get_password()?;
    if value.trim().is_empty() {
        return Err(AppError::msg(format!("凭据不存在: {service}")));
    }
    Ok(Zeroizing::new(value))
}

pub fn credential_status(service: &str) -> AppResult<CredentialStatusDto> {
    let service = canonical_service_id(service)?;
    let status = match entry_canonical(&service)?.get_password() {
        Ok(value) if !value.trim().is_empty() => CredentialState::Available,
        Ok(_) | Err(local encrypted credential store::Error::NoEntry) => CredentialState::Missing,
        Err(_) => CredentialState::Missing,
    };
    Ok(CredentialStatusDto {
        service,
        configured: status == CredentialState::Available,
        status,
        checked_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn delete_api_key(service: &str) -> AppResult<CredentialStatusDto> {
    let service = canonical_service_id(service)?;
    let _ = entry_canonical(&service)?.delete_credential();
    credential_status(&service)
}
```

- [ ] **Step 5: Run core tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml credentials security::ipc_policy --lib
```

Expected: PASS.

## Task 3: Update IPC Contract

**Files:**

- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src/lib/ipc.ts`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/credentials.ts`

- [ ] **Step 1: Change backend commands**

Update commands:

```rust
#[tauri::command]
pub fn credential_set(
    state: State<'_, Arc<AppState>>,
    service: String,
    value: String,
) -> AppResult<credentials::CredentialStatusDto> {
    let _ = state;
    credentials::set_api_key(&service, &value)
}

#[tauri::command]
pub fn credential_status(service: String) -> AppResult<credentials::CredentialStatusDto> {
    credentials::credential_status(&service)
}

#[tauri::command]
pub fn credential_delete(service: String) -> AppResult<credentials::CredentialStatusDto> {
    credentials::delete_api_key(&service)
}
```

Remove `credential_has` from frontend wrappers and settings UI call sites. Keep the backend command registered for one release only if other Rust command registration code still expects it; if kept, implement it by calling `credential_status` and returning `configured`, not by reading marker state.

- [ ] **Step 2: Add frontend types**

In `src/types/ipc.ts` add:

```ts
export type CredentialStatus = "available" | "missing";

export interface CredentialStatusDto {
  service: string;
  status: CredentialStatus;
  configured: boolean;
  checkedAt: string;
}
```

- [ ] **Step 3: Update IPC wrappers**

In `src/lib/ipc.ts`:

```ts
export async function credentialSet(
  service: string,
  value: string,
): Promise<CredentialStatusDto> {
  return invoke<CredentialStatusDto>("credential_set", { service, value });
}

export async function credentialStatus(
  service: string,
): Promise<CredentialStatusDto> {
  return invoke<CredentialStatusDto>("credential_status", { service });
}

export async function credentialDelete(
  service: string,
): Promise<CredentialStatusDto> {
  return invoke<CredentialStatusDto>("credential_delete", { service });
}
```

- [ ] **Step 4: Add service helpers**

In `src/lib/credentials.ts`:

```ts
export function llmCredentialService(provider: string): string {
  return `iris.llm.${provider}`;
}

export function mcpCredentialService(provider: string): string {
  return `iris.mcp.${provider}`;
}
```

- [ ] **Step 5: Run typecheck**

Run:

```bash
npm run typecheck
```

Expected: FAIL until call sites are updated in the next tasks.

## Task 4: Update LLM Settings UI And Provider Delete Semantics

**Files:**

- Modify: `src/components/settings/LlmRoutingSection.tsx`
- Modify: `src-tauri/src/commands/llm_config_commands.rs`
- Modify: `src/lib/ipc.ts`
- Modify: `src/types/llm.ts`
- Test: `tests/llm-reasoning-routing.test.ts`
- Test: `tests/model-provider-registry.test.ts`

- [ ] **Step 1: Write failing UI contract tests**

Add assertions:

```ts
expect(section).toContain("credentialStatus(llmCredentialService(id))");
expect(section).toContain("service id");
expect(section).toContain("移除配置");
expect(section).toContain("同时清除 Key");
expect(section).not.toContain("credentialHas(");
expect(section).not.toContain("Delete");
```

- [ ] **Step 2: Change delete provider request**

In Rust command DTO:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProviderRequest {
    pub provider_id: String,
    pub delete_credential: bool,
}
```

Change command:

```rust
#[tauri::command]
pub fn llm_config_delete_provider(
    state: State<'_, Arc<AppState>>,
    request: DeleteProviderRequest,
) -> AppResult<LlmRoutingConfig> {
    delete_provider_inner(&state.db, &request.provider_id, request.delete_credential)
}
```

Inside `delete_provider_inner`, call `credentials::delete_api_key(&credential_service(provider_id))` only when `delete_credential` is true.

- [ ] **Step 3: Update frontend delete flow**

In `LlmRoutingSection.tsx`, replace direct delete confirm with a two-step confirmation using existing UI patterns:

```ts
const deleteProvider = async (provider: VisibleProvider) => {
  const deleteConfig = confirm(
    `移除 ${provider.name} 配置？模型列表和验证记录会移除。API Key 默认保留。`,
  );
  if (!deleteConfig || !data) return;
  const deleteCredential = confirm(
    `是否同时清除 ${provider.name} 的 API Key？选择“取消”会保留 Key。`,
  );
  const nextRouting = normalizeRouting(
    await llmConfigDeleteProvider({
      providerId: provider.id,
      deleteCredential,
    }),
  );
  applyRouting(nextRouting);
  await refreshKeyStatus([provider.id]);
};
```

Use `window.confirm` for the first implementation. Do not introduce a new modal component in this refactor.

- [ ] **Step 4: Replace key status loading**

Replace `credentialHas` calls with:

```ts
const status = await credentialStatus(llmCredentialService(id));
configured[id] = status.configured;
```

Keep a separate `keyStatusByProvider` map so UI can display `available` / `missing`.

- [ ] **Step 5: Update labels**

UI copy:

- `保存 Key` remains save/overwrite.
- Add helper text: `保存后会覆盖此 service 当前 Key；Iris 不提供查看或复制。`
- `清除` becomes `清除 Key`.
- `Delete` becomes `移除配置`.
- Show `service: iris.llm.<providerId>` in details.

- [ ] **Step 6: Run tests**

Run:

```bash
npm run test -- tests/llm-reasoning-routing.test.ts tests/model-provider-registry.test.ts
cargo test --manifest-path src-tauri/Cargo.toml delete_provider --lib
```

Expected: PASS.

## Task 5: Update MCP Credential Flow

**Files:**

- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Test: `tests/tool-confirm-dialog.test.tsx`
- Test: Rust tests in `src-tauri/src/ai_runtime/mcp_host_runtime.rs`

- [ ] **Step 1: Use MCP service helper**

Ensure MCP provider presets and forms use:

```ts
mcpCredentialService(provider.id);
```

instead of hand-built strings.

- [ ] **Step 2: Save credentials before provider config**

Keep the existing order, but use the new return type:

```ts
for (const credential of credentialSaves) {
  const status = await credentialSet(credential.service, credential.value);
  if (!status.configured) {
    throw new Error(`凭据保存失败: ${credential.service}`);
  }
}
await webEvidenceProviderUpsert(input);
```

- [ ] **Step 3: Update MCP runtime lookup**

In `mcp_host_runtime.rs`, replace `get_api_key(db, service)` with:

```rust
|service| crate::credentials::get_runtime_secret(service).map(|secret| secret.to_string())
```

Do not log env/header values.

- [ ] **Step 4: Update diagnostics**

`provider_credential_diagnostic_checks` should call `credential_status(service)` and map:

- `available` -> passed.
- `missing` -> failed unless optional.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml mcp_host_runtime provider_credential --lib
npm run test -- tests/tool-confirm-dialog.test.tsx
```

Expected: PASS.

## Task 6: Remove Agent Plaintext Secret Surfaces

**Files:**

- Modify: `src-tauri/src/ai_runtime/tool_catalog/boundary.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/boundary.rs`
- Test: `src-tauri/tests/agent_permission_boundaries.rs`

- [ ] **Step 1: Remove unsupported tool declarations**

Remove `secret_create_update` and `secret_read_plaintext` from the tool catalog.

Keep only:

```rust
ToolCatalogEntry {
    name: "secret_exists",
    description: "检查 named credential 是否存在，不读取明文值",
    ...
}
```

- [ ] **Step 2: Make secret_exists use real status**

Change output:

```rust
let status = crate::credentials::credential_status(service)?;
Ok(serde_json::json!({
    "type": "secret_exists",
    "service": status.service,
    "exists": status.configured,
    "status": status.status,
}))
```

- [ ] **Step 3: Add boundary tests**

Assert:

```rust
assert!(!catalog_names.contains(&"secret_read_plaintext"));
assert!(!catalog_names.contains(&"secret_create_update"));
assert!(catalog_names.contains(&"secret_exists"));
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml agent_permission_boundaries secret_exists --all-targets
```

Expected: PASS.

## Task 7: Add Safety Leak Tests

**Files:**

- Modify: `src-tauri/src/credentials.rs`
- Add or modify: `src-tauri/tests/credential_security.rs`
- Modify: `tests/llm-config.test.ts`

- [ ] **Step 1: Add SQLite no-plaintext test**

Use a sample key:

```rust
const SAMPLE_KEY: &str = "sk-iris-test-secret-never-store";
```

After saving via credential API, query all `settings.value` rows and assert the sample key does not appear.

- [ ] **Step 2: Add IPC no-plaintext test**

Assert `credential_set`, `credential_status`, and `credential_delete` serialized DTOs contain service/status only and never `SAMPLE_KEY`.

- [ ] **Step 3: Add frontend no-view contract**

In TS contract tests, assert settings UI does not contain strings like:

```ts
"查看 Key";
"复制 Key";
"显示 Key";
```

and does contain:

```ts
"Iris 不提供查看或复制";
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml credential_security credentials --all-targets
npm run test -- tests/llm-config.test.ts tests/model-provider-registry.test.ts
```

Expected: PASS.

## Task 8: Clean Old API And Compile

**Files:**

- Modify any remaining references reported by search.

- [ ] **Step 1: Search for removed symbols**

Run:

```bash
rg -n "ApiKeyBundle|API_KEY_BUNDLE|credential_unlock_session\\(|credential_lock_session\\(|credentialHas\\(|api_key_configured\\(|mark_api_key_configured|clear_api_key_configured|get_legacy_api_key_secret|macos_protected_local encrypted credential store|secret_read_plaintext|secret_create_update" src src-tauri tests
```

Expected allowed leftovers:

- CAS/vault session lock functions may remain if unrelated to API Keys.
- Tests may contain negative assertions.
- No production LLM/MCP API Key path may use marker-only functions.

- [ ] **Step 2: Remove or rewrite production leftovers**

Rewrite any remaining LLM/MCP credential access to:

```rust
credentials::credential_status(service)
credentials::get_runtime_secret(service)
credentials::set_api_key(service, value)
credentials::delete_api_key(service)
```

- [ ] **Step 3: Run static checks**

Run:

```bash
npm run typecheck
npm run lint
npm run format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: PASS.

## Task 9: Final Verification

**Files:**

- No new edits unless verification reveals a regression.

- [ ] **Step 1: Run focused credential suites**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml credentials credential_security mcp_host_runtime delete_provider --all-targets
npm run test -- tests/llm-config.test.ts tests/model-provider-registry.test.ts tests/llm-reasoning-routing.test.ts tests/tool-confirm-dialog.test.tsx
```

Expected: PASS.

- [ ] **Step 2: Run broad suites**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run test
```

Expected: Rust lib passes. If full npm suite still fails on the pre-existing `App.impl.tsx` line-count contract, record it separately and do not hide it.

- [ ] **Step 3: Run final leak scan**

Run:

```bash
rg -n "sk-iris-test-secret-never-store|API Key.*\\{|Authorization.*Bearer.*\\{|credential.*value" src src-tauri tests docs
```

Expected: no production leak path. Test fixtures may contain the sample key only in assertions that prove it is absent from persisted/returned data.

- [ ] **Step 4: Commit**

Use a Chinese conventional commit:

```bash
git add src-tauri/src/credentials.rs src-tauri/src/commands/settings.rs src-tauri/src/security/ipc_policy.rs src-tauri/src/llm/config.rs src-tauri/src/llm/engine.rs src-tauri/src/commands/llm_config_commands.rs src-tauri/src/ai_runtime/mcp_host_runtime.rs src-tauri/src/ai_runtime/tool_catalog/boundary.rs src-tauri/src/ai_runtime/tool_dispatch/boundary.rs src/lib/credentials.ts src/lib/ipc.ts src/types/ipc.ts src/components/settings/LlmRoutingSection.tsx src/components/ai/skills/McpProfilesPanel.tsx tests src-tauri/tests
git commit -m "refactor(ai): 重整 LLM 与 MCP 凭据体系"
```

Expected: commit succeeds only after all required checks are accounted for.
