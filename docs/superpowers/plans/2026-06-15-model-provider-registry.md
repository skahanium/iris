# Model Provider Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the Management Center “模型与供应商” page as a provider/model registry that separates provider health, model discovery, model validation, and capability routing.

**Architecture:** Keep `settings.llm_routing` as the source of final capability routes, and add a separate local model registry cache for provider-discovered/manual model metadata. Backend IPC exposes provider tests, model refresh, model validation, and capability confirmation; the React page consumes those contracts and filters route candidates by verified capability.

**Tech Stack:** Tauri 2.x, Rust, SQLite migrations, React 19, TypeScript, TailwindCSS + shadcn/ui, Vitest, Cargo tests.

---

## File Structure

- `src-tauri/migrations/029_model_registry.sql`: create local model registry cache table.
- `src-tauri/migrations/029_model_registry.down.sql`: rollback table creation.
- `src-tauri/src/storage/migrate.rs`: register migration 029.
- `src-tauri/src/llm/model_registry.rs`: registry storage API, DTOs, capability filtering, legacy `enabledModels` merge.
- `src-tauri/src/llm/mod.rs`: expose the new `model_registry` module.
- `src-tauri/src/llm/providers.rs`: hide Ollama from settings-facing external provider lists while preserving backend allowance.
- `src-tauri/src/commands/llm_config_commands.rs`: extend `llm_config_get`; add provider test, model refresh, model validation, and capability confirmation commands.
- `src-tauri/src/commands/mod.rs`: expose any new command module only if split out; otherwise unchanged.
- `src-tauri/src/lib.rs`: register new IPC commands.
- `src/types/llm.ts`: add model registry DTOs and validation types.
- `src/lib/ipc.ts`: add typed wrappers for new IPC commands.
- `src/components/settings/LlmRoutingSection.tsx`: refactor UI into provider list, model catalog, capability route sections.
- `tests/model-provider-registry.test.ts`: frontend static/contract tests for the new page and IPC shape.
- `tests/phase3-model-persona-routing.test.ts`: update old assertions that currently expect per-model `llmConfigTest(provider.id, model.id)`.
- Rust unit tests inside `model_registry.rs`, `providers.rs`, `llm_config_commands.rs`, and `storage/migrate.rs`.
- `docs/llm-routing.md`, `docs/design-system.md`, `ROADMAP.md`: align docs with the registry design.

## Task 1: Contract Tests For Registry Page And IPC

**Files:**
- Create: `tests/model-provider-registry.test.ts`
- Modify: `tests/phase3-model-persona-routing.test.ts`

- [ ] **Step 1: Write the failing frontend contract test**

Create `tests/model-provider-registry.test.ts`:

```ts
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model provider registry contract", () => {
  it("splits provider health, model catalog, and capability routing", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("data-section=\"llm-providers\"");
    expect(section).toContain("data-section=\"llm-model-catalog\"");
    expect(section).toContain("data-section=\"llm-capability-routing\"");
    expect(section).toContain("测试供应商");
    expect(section).toContain("刷新模型列表");
    expect(section).toContain("验证模型");
    expect(section).toContain("视觉测试");
  });

  it("does not expose Ollama in the external provider settings panel", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).not.toContain('name: "Ollama"');
    expect(section).not.toContain('provider.id === "ollama"');
    expect(section).not.toContain('keyless: providerId === "ollama"');
  });

  it("adds typed IPC wrappers for registry operations", () => {
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/llm.ts");
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(types).toContain("ModelRegistryEntry");
    expect(types).toContain("ModelValidationKind");
    expect(ipc).toContain("llmConfigTestProvider");
    expect(ipc).toContain("llmModelRegistryRefresh");
    expect(ipc).toContain("llmModelValidate");
    expect(ipc).toContain("llmModelConfirmCapability");
    expect(rust).toContain("llm_config_test_provider");
    expect(rust).toContain("llm_model_registry_refresh");
    expect(rust).toContain("llm_model_validate");
    expect(rust).toContain("llm_model_confirm_capability");
  });

  it("filters capability route candidates by verified capability", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("modelsForSlot");
    expect(section).toContain("supportsModelForSlot");
    expect(section).toContain("userConfirmedCapabilities");
    expect(section).toContain("visionVerifiedAt");
    expect(section).not.toContain("routeModelsForProvider(providerId)");
  });
});
```

- [ ] **Step 2: Update the old Phase 3 assertion to the new probe shape**

In `tests/phase3-model-persona-routing.test.ts`, replace the old per-model diagnostic assertions:

```ts
expect(section).toContain("llmConfigTest(provider.id, model.id)");
expect(section).not.toContain("llmConfigTest(provider.id, defaultModel)");
```

with:

```ts
expect(section).toContain("llmConfigTestProvider(provider.id)");
expect(section).toContain("llmModelValidate(provider.id, model.id");
expect(section).not.toContain("llmConfigTest(provider.id, model.id)");
expect(section).not.toContain("llmConfigTest(provider.id, defaultModel)");
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
npm run test -- tests/model-provider-registry.test.ts tests/phase3-model-persona-routing.test.ts
```

Expected: FAIL because the new test file references strings and wrappers that do not exist yet.

- [ ] **Step 4: Commit the failing contract tests**

```bash
git add tests/model-provider-registry.test.ts tests/phase3-model-persona-routing.test.ts
git commit -m "test(ai): 补充模型供应商注册中心契约"
```

## Task 2: Model Registry Migration And Storage API

**Files:**
- Create: `src-tauri/migrations/029_model_registry.sql`
- Create: `src-tauri/migrations/029_model_registry.down.sql`
- Create: `src-tauri/src/llm/model_registry.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Modify: `src-tauri/src/storage/migrate.rs`

- [ ] **Step 1: Add the failing Rust storage tests**

Create `src-tauri/src/llm/model_registry.rs` with the DTOs and tests first:

```rust
//! Local cache for provider-discovered and manually confirmed LLM models.

use serde::{Deserialize, Serialize};

use crate::ai_types::CapabilitySlot;
use crate::error::AppResult;
use crate::storage::db::Database;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelRegistrySource {
    BuiltIn,
    ProviderDiscovered,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRegistryEntry {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub source: ModelRegistrySource,
    pub stale: bool,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub last_refreshed_at: Option<String>,
    pub text_verified_at: Option<String>,
    pub vision_verified_at: Option<String>,
    pub user_confirmed_capabilities: Vec<CapabilitySlot>,
}

pub fn upsert_provider_discovered_models(
    _db: &Database,
    _provider_id: &str,
    _model_ids: &[String],
) -> AppResult<()> {
    unimplemented!("implemented in Task 2 Step 4")
}

pub fn list_registry_entries(_db: &Database) -> AppResult<Vec<ModelRegistryEntry>> {
    unimplemented!("implemented in Task 2 Step 4")
}

pub fn confirm_capability(
    _db: &Database,
    _provider_id: &str,
    _model_id: &str,
    _slot: CapabilitySlot,
) -> AppResult<ModelRegistryEntry> {
    unimplemented!("implemented in Task 2 Step 4")
}

pub fn supports_model_for_slot(entry: &ModelRegistryEntry, slot: CapabilitySlot) -> bool {
    entry.user_confirmed_capabilities.contains(&slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_discovered_model_does_not_support_vision_by_default() {
        let entry = ModelRegistryEntry {
            provider_id: "deepseek".into(),
            model_id: "new-model".into(),
            display_name: "new-model".into(),
            source: ModelRegistrySource::ProviderDiscovered,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: None,
            vision_verified_at: None,
            user_confirmed_capabilities: vec![],
        };

        assert!(!supports_model_for_slot(&entry, CapabilitySlot::Vision));
    }

    #[test]
    fn user_confirmation_allows_specific_capability() {
        let entry = ModelRegistryEntry {
            provider_id: "custom".into(),
            model_id: "custom-vision".into(),
            display_name: "custom-vision".into(),
            source: ModelRegistrySource::Manual,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: None,
            vision_verified_at: None,
            user_confirmed_capabilities: vec![CapabilitySlot::Vision],
        };

        assert!(supports_model_for_slot(&entry, CapabilitySlot::Vision));
        assert!(!supports_model_for_slot(&entry, CapabilitySlot::LongContext));
    }
}
```

Add `pub mod model_registry;` to `src-tauri/src/llm/mod.rs`.

- [ ] **Step 2: Run the focused Rust test and verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml llm::model_registry
```

Expected: FAIL because the new module contains `unimplemented!()` and the migration/table are not registered yet once storage tests are added.

- [ ] **Step 3: Add migration 029**

Create `src-tauri/migrations/029_model_registry.sql`:

```sql
CREATE TABLE IF NOT EXISTS llm_model_registry (
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('built_in', 'provider_discovered', 'manual')),
  stale INTEGER NOT NULL DEFAULT 0,
  first_seen_at TEXT,
  last_seen_at TEXT,
  last_refreshed_at TEXT,
  text_verified_at TEXT,
  vision_verified_at TEXT,
  user_confirmed_capabilities TEXT NOT NULL DEFAULT '[]',
  PRIMARY KEY (provider_id, model_id)
);

CREATE INDEX IF NOT EXISTS idx_llm_model_registry_provider
  ON llm_model_registry(provider_id, stale, source);
```

Create `src-tauri/migrations/029_model_registry.down.sql`:

```sql
DROP INDEX IF EXISTS idx_llm_model_registry_provider;
DROP TABLE IF EXISTS llm_model_registry;
```

- [ ] **Step 4: Register migration 029**

In `src-tauri/src/storage/migrate.rs`, add constants after migration 028:

```rust
const MIGRATION_029_UP: &str = include_str!("../../migrations/029_model_registry.sql");
const MIGRATION_029_DOWN: &str = include_str!("../../migrations/029_model_registry.down.sql");
```

Add `("029_model_registry", MIGRATION_029_UP)` to the migration list after `028_multimodal_messages`, and add `("029_model_registry", MIGRATION_029_DOWN)` to the reverse/down list before migration 028.

Add a unit test near existing migration tests:

```rust
#[test]
fn migration_029_creates_model_registry() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'llm_model_registry'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(count, 1);
}
```

- [ ] **Step 5: Implement the storage functions**

Replace the `unimplemented!()` functions in `model_registry.rs`:

```rust
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn encode_capabilities(slots: &[CapabilitySlot]) -> AppResult<String> {
    Ok(serde_json::to_string(slots)?)
}

fn decode_capabilities(raw: String) -> Vec<CapabilitySlot> {
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn upsert_provider_discovered_models(
    db: &Database,
    provider_id: &str,
    model_ids: &[String],
) -> AppResult<()> {
    let now = now_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE llm_model_registry
             SET stale = 1, last_refreshed_at = ?2
             WHERE provider_id = ?1 AND source = 'provider_discovered'",
            rusqlite::params![provider_id, now],
        )?;

        for model_id in model_ids {
            let id = model_id.trim();
            if id.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT INTO llm_model_registry
                 (provider_id, model_id, display_name, source, stale, first_seen_at, last_seen_at, last_refreshed_at)
                 VALUES (?1, ?2, ?2, 'provider_discovered', 0, ?3, ?3, ?3)
                 ON CONFLICT(provider_id, model_id) DO UPDATE SET
                   display_name = excluded.display_name,
                   stale = 0,
                   last_seen_at = excluded.last_seen_at,
                   last_refreshed_at = excluded.last_refreshed_at",
                rusqlite::params![provider_id, id, now],
            )?;
        }
        Ok(())
    })
}

pub fn list_registry_entries(db: &Database) -> AppResult<Vec<ModelRegistryEntry>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT provider_id, model_id, display_name, source, stale,
                    first_seen_at, last_seen_at, last_refreshed_at,
                    text_verified_at, vision_verified_at, user_confirmed_capabilities
             FROM llm_model_registry
             ORDER BY provider_id, display_name",
        )?;
        let rows = stmt.query_map([], |row| {
            let source: String = row.get(3)?;
            let caps: String = row.get(10)?;
            let source = match source.as_str() {
                "built_in" => ModelRegistrySource::BuiltIn,
                "manual" => ModelRegistrySource::Manual,
                _ => ModelRegistrySource::ProviderDiscovered,
            };
            Ok(ModelRegistryEntry {
                provider_id: row.get(0)?,
                model_id: row.get(1)?,
                display_name: row.get(2)?,
                source,
                stale: row.get::<_, i64>(4)? != 0,
                first_seen_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                last_refreshed_at: row.get(7)?,
                text_verified_at: row.get(8)?,
                vision_verified_at: row.get(9)?,
                user_confirmed_capabilities: decode_capabilities(caps),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

pub fn confirm_capability(
    db: &Database,
    provider_id: &str,
    model_id: &str,
    slot: CapabilitySlot,
) -> AppResult<ModelRegistryEntry> {
    let mut caps = vec![slot];
    caps.sort();
    caps.dedup();
    let raw = encode_capabilities(&caps)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO llm_model_registry
             (provider_id, model_id, display_name, source, user_confirmed_capabilities)
             VALUES (?1, ?2, ?2, 'manual', ?3)
             ON CONFLICT(provider_id, model_id) DO UPDATE SET
               user_confirmed_capabilities = ?3",
            rusqlite::params![provider_id, model_id, raw],
        )?;
        Ok(())
    })?;
    list_registry_entries(db)?
        .into_iter()
        .find(|entry| entry.provider_id == provider_id && entry.model_id == model_id)
        .ok_or_else(|| crate::error::AppError::msg("model registry entry not found after confirm"))
}
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml model_registry
cargo test --manifest-path src-tauri/Cargo.toml migration_029_creates_model_registry
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/migrations/029_model_registry.sql src-tauri/migrations/029_model_registry.down.sql src-tauri/src/llm/model_registry.rs src-tauri/src/llm/mod.rs src-tauri/src/storage/migrate.rs
git commit -m "feat(ai): 添加模型注册表缓存"
```

## Task 3: Backend Provider Refresh And Validation IPC

**Files:**
- Modify: `src-tauri/src/commands/llm_config_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/llm/providers.rs`

- [ ] **Step 1: Add backend tests for provider list and validation semantics**

In `src-tauri/src/llm/providers.rs`, add:

```rust
pub fn list_external_providers_from_routing(routing: &LlmRoutingConfig) -> Vec<LlmProviderInfo> {
    list_providers_from_routing(routing)
        .into_iter()
        .filter(|provider| provider.id != "ollama")
        .collect()
}
```

Add test:

```rust
#[test]
fn settings_external_providers_hide_ollama() {
    let routing = crate::llm::config::deepseek_defaults();
    let ids: Vec<_> = list_external_providers_from_routing(&routing)
        .into_iter()
        .map(|provider| provider.id)
        .collect();

    assert!(!ids.contains(&"ollama".to_string()));
    assert!(ids.contains(&"deepseek".to_string()));
    assert!(ids.contains(&"mimo".to_string()));
}
```

- [ ] **Step 2: Replace settings-facing provider list**

In `llm_config_get`, change:

```rust
providers: list_providers_from_routing(&routing),
```

to:

```rust
providers: crate::llm::providers::list_external_providers_from_routing(&routing),
```

Keep `is_allowed_provider("ollama")` and existing backend defaults unchanged.

- [ ] **Step 3: Add response/request DTOs**

Add to `src-tauri/src/commands/llm_config_commands.rs` near `LlmConfigTestResult`:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelRegistryRefreshResult {
    pub provider_id: String,
    pub model_count: usize,
    pub message: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelValidationKind {
    Text,
    Vision,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilityConfirmRequest {
    pub provider_id: String,
    pub model_id: String,
    pub slot: crate::ai_types::CapabilitySlot,
}
```

- [ ] **Step 4: Split provider test from model validation**

Rename the existing command body into provider-only semantics:

```rust
#[tauri::command]
pub async fn llm_config_test_provider(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<LlmConfigTestResult> {
    let resolved = config::resolve_for_provider(&state.db, &provider_id, None)?;
    let api_key = api_key_for_probe(&provider_id, resolved.api_key)?;
    let client = probe_client()?;
    let probe_url = models_probe_url(&provider_id, &resolved.base_url);
    let mut req = client.get(&probe_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    match req.send().await {
        Ok(response) if response.status().is_success() => Ok(LlmConfigTestResult {
            ok: true,
            message: "供应商可连接".into(),
        }),
        Ok(response) if response.status().as_u16() == 401 => Ok(LlmConfigTestResult {
            ok: false,
            message: "API Key 无效或未授权（401）".into(),
        }),
        Ok(response) => {
            let status = response.status();
            let body = truncate_error_text(&response.text().await.unwrap_or_default());
            Ok(LlmConfigTestResult {
                ok: false,
                message: format!("供应商探测 HTTP {status}: {body}"),
            })
        }
        Err(e) => Ok(LlmConfigTestResult {
            ok: false,
            message: format!("网络错误：{e}"),
        }),
    }
}
```

Keep the old `llm_config_test` as a compatibility wrapper:

```rust
#[tauri::command]
pub async fn llm_config_test(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    model: Option<String>,
) -> AppResult<LlmConfigTestResult> {
    match model {
        Some(model_id) => {
            llm_model_validate(state, provider_id, model_id, ModelValidationKind::Text).await
        }
        None => llm_config_test_provider(state, provider_id).await,
    }
}
```

- [ ] **Step 5: Add model refresh command**

Add helper:

```rust
async fn fetch_provider_model_ids(
    client: &reqwest::Client,
    provider_id: &str,
    base_url: &str,
    api_key: &str,
) -> AppResult<Vec<String>> {
    let mut req = client.get(models_probe_url(provider_id, base_url));
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    let response = req.send().await.map_err(|e| AppError::msg(format!("{e}")))?;
    let status = response.status();
    let json: serde_json::Value = response.json().await.map_err(|e| {
        AppError::msg(format!("模型列表不是有效 JSON（HTTP {status}）：{e}"))
    })?;
    if !status.is_success() {
        return Err(AppError::msg(format!("模型列表 HTTP {status}")));
    }
    let ids = json
        .get("data")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(ids)
}
```

Add command:

```rust
#[tauri::command]
pub async fn llm_model_registry_refresh(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<LlmModelRegistryRefreshResult> {
    if provider_id == "ollama" {
        return Err(AppError::msg("Ollama 不在外部供应商模型注册中心中刷新"));
    }
    let resolved = config::resolve_for_provider(&state.db, &provider_id, None)?;
    let api_key = api_key_for_probe(&provider_id, resolved.api_key)?;
    let client = probe_client()?;
    let ids = fetch_provider_model_ids(&client, &provider_id, &resolved.base_url, &api_key).await?;
    crate::llm::model_registry::upsert_provider_discovered_models(
        &state.db,
        &provider_id,
        &ids,
    )?;
    Ok(LlmModelRegistryRefreshResult {
        provider_id,
        model_count: ids.len(),
        message: format!("已刷新 {} 个模型", ids.len()),
    })
}
```

- [ ] **Step 6: Add model validation command**

Add:

```rust
#[tauri::command]
pub async fn llm_model_validate(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    model_id: String,
    kind: ModelValidationKind,
) -> AppResult<LlmConfigTestResult> {
    let resolved = config::resolve_for_provider(&state.db, &provider_id, Some(&model_id))?;
    let api_key = api_key_for_probe(&provider_id, resolved.api_key)?;
    let client = probe_client()?;
    match kind {
        ModelValidationKind::Text => {
            match fetch_provider_model_ids(&client, &provider_id, &resolved.base_url, &api_key).await {
                Ok(ids) if !ids.iter().any(|id| id == &model_id) => Ok(LlmConfigTestResult {
                    ok: false,
                    message: "供应商模型列表中未找到该模型".into(),
                }),
                Ok(_) => match probe_chat_minimal(
                    &client,
                    &provider_id,
                    &resolved.base_url,
                    &model_id,
                    &api_key,
                )
                .await
                {
                    Ok(()) => Ok(LlmConfigTestResult {
                        ok: true,
                        message: "模型文字请求验证通过".into(),
                    }),
                    Err(err) => Ok(LlmConfigTestResult {
                        ok: false,
                        message: format!("模型文字请求失败：{err}"),
                    }),
                },
                Err(_) => match probe_chat_minimal(
                    &client,
                    &provider_id,
                    &resolved.base_url,
                    &model_id,
                    &api_key,
                )
                .await
                {
                    Ok(()) => Ok(LlmConfigTestResult {
                        ok: true,
                        message: "模型文字请求验证通过".into(),
                    }),
                    Err(err) => Ok(LlmConfigTestResult {
                        ok: false,
                        message: format!("模型文字请求失败：{err}"),
                    }),
                },
            }
        }
        ModelValidationKind::Vision => probe_vision_minimal(
            &client,
            &resolved.base_url,
            &model_id,
            &api_key,
        )
        .await
        .map(|_| LlmConfigTestResult {
            ok: true,
            message: "视觉测试通过".into(),
        })
        .or_else(|err| {
            Ok(LlmConfigTestResult {
                ok: false,
                message: format!("视觉测试失败：{err}"),
            })
        }),
    }
}
```

Add a minimal vision probe:

```rust
async fn probe_vision_minimal(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    api_key: &str,
) -> AppResult<()> {
    let url = chat_completions_url(base_url);
    let body = serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "Reply with ok." },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                        "detail": "low"
                    }
                }
            ]
        }],
        "max_tokens": 1,
        "stream": false
    });
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("{e}")))?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let text = truncate_error_text(&response.text().await.unwrap_or_default());
    Err(AppError::msg(format!("HTTP {status}: {text}")))
}
```

- [ ] **Step 7: Add capability confirmation command**

```rust
#[tauri::command]
pub fn llm_model_confirm_capability(
    state: State<'_, Arc<AppState>>,
    request: ModelCapabilityConfirmRequest,
) -> AppResult<crate::llm::model_registry::ModelRegistryEntry> {
    crate::llm::model_registry::confirm_capability(
        &state.db,
        &request.provider_id,
        &request.model_id,
        request.slot,
    )
}
```

- [ ] **Step 8: Register IPC commands**

In `src-tauri/src/lib.rs`, add to `tauri::generate_handler!` next to existing LLM config commands:

```rust
commands::llm_config_commands::llm_config_test_provider,
commands::llm_config_commands::llm_model_registry_refresh,
commands::llm_config_commands::llm_model_validate,
commands::llm_config_commands::llm_model_confirm_capability,
```

- [ ] **Step 9: Run focused Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml settings_external_providers_hide_ollama
cargo test --manifest-path src-tauri/Cargo.toml llm_config_commands
```

Expected: PASS. If `llm_config_commands` has no isolated module test yet, run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml llm::providers
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands/llm_config_commands.rs src-tauri/src/lib.rs src-tauri/src/llm/providers.rs
git commit -m "feat(ai): 拆分供应商与模型验证命令"
```

## Task 4: TypeScript Types And IPC Wrappers

**Files:**
- Modify: `src/types/llm.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add TypeScript DTOs**

In `src/types/llm.ts`, add:

```ts
export type ModelRegistrySource =
  | "built_in"
  | "provider_discovered"
  | "manual";

export type ModelValidationKind = "text" | "vision";

export interface ModelRegistryEntry {
  providerId: string;
  modelId: string;
  displayName: string;
  source: ModelRegistrySource;
  stale: boolean;
  firstSeenAt: string | null;
  lastSeenAt: string | null;
  lastRefreshedAt: string | null;
  textVerifiedAt: string | null;
  visionVerifiedAt: string | null;
  userConfirmedCapabilities: CapabilitySlot[];
}

export interface LlmModelRegistryRefreshResult {
  providerId: string;
  modelCount: number;
  message: string;
}

export interface ModelCapabilityConfirmRequest {
  providerId: string;
  modelId: string;
  slot: CapabilitySlot;
}
```

Extend `LlmConfigGetResponse`:

```ts
export interface LlmConfigGetResponse {
  routing: LlmRoutingConfig;
  providers: { id: string; name: string; default_model: string }[];
  catalog: ModelCatalogEntry[];
  registry: ModelRegistryEntry[];
}
```

- [ ] **Step 2: Add IPC wrappers**

In `src/lib/ipc.ts`, extend the `@/types/llm` import:

```ts
  LlmModelRegistryRefreshResult,
  ModelCapabilityConfirmRequest,
  ModelValidationKind,
```

Add wrappers after `llmConfigTest`:

```ts
export async function llmConfigTestProvider(
  providerId: string,
): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("llm_config_test_provider", {
    providerId,
  });
}

export async function llmModelRegistryRefresh(
  providerId: string,
): Promise<LlmModelRegistryRefreshResult> {
  return invoke<LlmModelRegistryRefreshResult>("llm_model_registry_refresh", {
    providerId,
  });
}

export async function llmModelValidate(
  providerId: string,
  modelId: string,
  kind: ModelValidationKind = "text",
): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("llm_model_validate", {
    providerId,
    modelId,
    kind,
  });
}

export async function llmModelConfirmCapability(
  request: ModelCapabilityConfirmRequest,
): Promise<ModelRegistryEntry> {
  return invoke<ModelRegistryEntry>("llm_model_confirm_capability", {
    request,
  });
}
```

- [ ] **Step 3: Run type-focused tests**

Run:

```bash
npm run test -- tests/model-provider-registry.test.ts
npm run typecheck
```

Expected: the contract test still fails on UI strings until Task 5; `typecheck` may fail until backend response normalization is reflected in the UI. Record exact failures before continuing.

- [ ] **Step 4: Commit**

```bash
git add src/types/llm.ts src/lib/ipc.ts
git commit -m "feat(ai): 添加模型注册中心前端契约"
```

## Task 5: Backend Config Response Includes Registry And Legacy Models

**Files:**
- Modify: `src-tauri/src/commands/llm_config_commands.rs`
- Modify: `src-tauri/src/llm/model_registry.rs`

- [ ] **Step 1: Add merge function test**

In `model_registry.rs`, add:

```rust
pub fn entries_from_builtin_and_routing(
    _routing: &crate::llm::config::LlmRoutingConfig,
    _registry: Vec<ModelRegistryEntry>,
) -> Vec<ModelRegistryEntry> {
    unimplemented!("implemented in Task 5 Step 2")
}
```

Add test:

```rust
#[test]
fn legacy_enabled_models_are_exposed_as_manual_entries() {
    let mut routing = crate::llm::config::deepseek_defaults();
    routing.providers.insert(
        "deepseek".into(),
        crate::llm::config::ProviderOverride {
            base_url: None,
            label: None,
            default_model: None,
            enabled_models: Some(vec!["custom-deepseek-model".into()]),
        },
    );

    let entries = entries_from_builtin_and_routing(&routing, vec![]);

    assert!(entries.iter().any(|entry| {
        entry.provider_id == "deepseek"
            && entry.model_id == "custom-deepseek-model"
            && entry.source == ModelRegistrySource::Manual
    }));
}
```

- [ ] **Step 2: Implement merge function**

```rust
pub fn entries_from_builtin_and_routing(
    routing: &crate::llm::config::LlmRoutingConfig,
    mut registry: Vec<ModelRegistryEntry>,
) -> Vec<ModelRegistryEntry> {
    let mut seen: std::collections::HashSet<(String, String)> = registry
        .iter()
        .map(|entry| (entry.provider_id.clone(), entry.model_id.clone()))
        .collect();

    for model in crate::llm::model_catalog::catalog_for_settings() {
        let key = (model.provider_id.to_string(), model.id.to_string());
        if seen.insert(key.clone()) {
            registry.push(ModelRegistryEntry {
                provider_id: key.0,
                model_id: key.1.clone(),
                display_name: model.display_name.to_string(),
                source: ModelRegistrySource::BuiltIn,
                stale: false,
                first_seen_at: None,
                last_seen_at: None,
                last_refreshed_at: None,
                text_verified_at: Some("built_in".into()),
                vision_verified_at: model.supports_vision.then(|| "built_in".into()),
                user_confirmed_capabilities: vec![],
            });
        }
    }

    for (provider_id, override_row) in &routing.providers {
        if let Some(models) = &override_row.enabled_models {
            for model_id in models {
                let key = (provider_id.clone(), model_id.clone());
                if seen.insert(key.clone()) {
                    registry.push(ModelRegistryEntry {
                        provider_id: key.0,
                        model_id: key.1.clone(),
                        display_name: key.1,
                        source: ModelRegistrySource::Manual,
                        stale: false,
                        first_seen_at: None,
                        last_seen_at: None,
                        last_refreshed_at: None,
                        text_verified_at: None,
                        vision_verified_at: None,
                        user_confirmed_capabilities: vec![],
                    });
                }
            }
        }
    }

    registry.sort_by(|a, b| {
        a.provider_id
            .cmp(&b.provider_id)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
    registry
}
```

- [ ] **Step 3: Extend config response**

In `LlmConfigGetResponse`, add:

```rust
pub registry: Vec<model_catalog::ModelCatalogEntry>,
```

Then replace it with the actual registry type:

```rust
pub registry: Vec<crate::llm::model_registry::ModelRegistryEntry>,
```

In `llm_config_get`:

```rust
let registry = crate::llm::model_registry::entries_from_builtin_and_routing(
    &routing,
    crate::llm::model_registry::list_registry_entries(&state.db)?,
);
Ok(LlmConfigGetResponse {
    providers: crate::llm::providers::list_external_providers_from_routing(&routing),
    catalog: model_catalog::catalog_for_settings(),
    registry,
    routing,
})
```

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml legacy_enabled_models_are_exposed_as_manual_entries
npm run test -- tests/model-provider-registry.test.ts
```

Expected: Rust test PASS; frontend contract still may fail until UI task is complete.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/llm_config_commands.rs src-tauri/src/llm/model_registry.rs
git commit -m "feat(ai): 合并内置与手动模型目录"
```

## Task 6: React Registry UI Refactor

**Files:**
- Modify: `src/components/settings/LlmRoutingSection.tsx`

- [ ] **Step 1: Add helper functions in the component**

Inside `LlmRoutingSection.tsx`, add helper functions near existing model helpers:

```ts
function modelRegistryKey(providerId: string, modelId: string): string {
  return `${providerId}:${modelId}`;
}

function supportsModelForSlot(
  model: ModelRegistryEntry,
  slot: CapabilitySlot,
  catalog: ModelCatalogEntry | undefined,
): boolean {
  if (slot === "fast" || slot === "writer") {
    return Boolean(catalog) || Boolean(model.textVerifiedAt) || model.source === "manual";
  }
  if (slot === "vision") {
    return (
      Boolean(catalog?.supportsVision) ||
      Boolean(model.visionVerifiedAt) ||
      model.userConfirmedCapabilities.includes("vision")
    );
  }
  if (slot === "long_context") {
    return (
      Boolean(catalog && catalog.contextWindow >= 128_000) ||
      model.userConfirmedCapabilities.includes("long_context")
    );
  }
  if (slot === "reasoner") {
    return (
      Boolean(catalog?.supportsThinking || catalog?.supportsTools) ||
      model.userConfirmedCapabilities.includes("reasoner")
    );
  }
  return false;
}
```

Add a component-local selector:

```ts
const registryForProvider = (providerId: string): ModelRegistryEntry[] =>
  data?.registry.filter((entry) => entry.providerId === providerId) ?? [];

const modelsForSlot = (slot: CapabilitySlot, providerId: string) =>
  registryForProvider(providerId).filter((entry) =>
    supportsModelForSlot(entry, slot, modelById(entry.modelId)),
  );
```

- [ ] **Step 2: Replace imports**

Replace `llmConfigTest` import with:

```ts
  llmConfigTestProvider,
  llmModelConfirmCapability,
  llmModelRegistryRefresh,
  llmModelValidate,
```

Import `ModelRegistryEntry` from `@/types/llm`.

- [ ] **Step 3: Add provider-level handlers**

Add state:

```ts
const [providerResults, setProviderResults] = useState<
  Record<string, { ok: boolean; message: string }>
>({});
const [refreshingProvider, setRefreshingProvider] = useState<string | null>(null);
```

Add handlers:

```ts
const testProvider = async (providerId: string) => {
  setTesting(`provider:${providerId}`);
  try {
    const result = await llmConfigTestProvider(providerId);
    setProviderResults((prev) => ({ ...prev, [providerId]: result }));
  } catch (err) {
    setProviderResults((prev) => ({
      ...prev,
      [providerId]: { ok: false, message: invokeErrorMessage(err) },
    }));
  } finally {
    setTesting(null);
  }
};

const refreshProviderModels = async (providerId: string) => {
  setRefreshingProvider(providerId);
  try {
    const result = await llmModelRegistryRefresh(providerId);
    setMessage(result.message);
    await load();
  } catch (err) {
    setMessage(`刷新模型列表失败：${invokeErrorMessage(err)}`);
  } finally {
    setRefreshingProvider(null);
  }
};
```

- [ ] **Step 4: Add model validation handlers**

```ts
const validateModel = async (
  providerId: string,
  modelId: string,
  kind: ModelValidationKind = "text",
) => {
  const key = `${kind}:${modelRegistryKey(providerId, modelId)}`;
  setTesting(key);
  setTestResults((prev) => {
    const next = { ...prev };
    delete next[key];
    return next;
  });
  try {
    const result = await llmModelValidate(providerId, modelId, kind);
    setTestResults((prev) => ({ ...prev, [key]: result }));
    if (result.ok) await load();
  } catch (err) {
    setTestResults((prev) => ({
      ...prev,
      [key]: { ok: false, message: invokeErrorMessage(err) },
    }));
  } finally {
    setTesting(null);
  }
};

const confirmCapability = async (
  providerId: string,
  modelId: string,
  slot: CapabilitySlot,
) => {
  await llmModelConfirmCapability({ providerId, modelId, slot });
  await load();
};
```

- [ ] **Step 5: Refactor JSX sections**

Replace the current top-level section names with:

```tsx
<div className="space-y-5" data-section="ai-connection">
  <section data-section="llm-providers" className="space-y-2">
    {/* provider rows */}
  </section>
  <section data-section="llm-model-catalog" className="space-y-2">
    {/* registry models for selected/configured providers */}
  </section>
  <section data-section="llm-capability-routing" className="space-y-2">
    {/* capability slots */}
  </section>
</div>
```

Provider row actions must include:

```tsx
<Button type="button" size="sm" variant="outline" onClick={() => void testProvider(provider.id)}>
  测试供应商
</Button>
<Button
  type="button"
  size="sm"
  variant="outline"
  disabled={refreshingProvider === provider.id}
  onClick={() => void refreshProviderModels(provider.id)}
>
  {refreshingProvider === provider.id ? "刷新中..." : "刷新模型列表"}
</Button>
```

Model row actions must include:

```tsx
<Button
  type="button"
  size="sm"
  variant="secondary"
  disabled={testing === `text:${modelRegistryKey(entry.providerId, entry.modelId)}`}
  onClick={() => void validateModel(entry.providerId, entry.modelId, "text")}
>
  验证模型
</Button>
<Button
  type="button"
  size="sm"
  variant="outline"
  disabled={testing === `vision:${modelRegistryKey(entry.providerId, entry.modelId)}`}
  onClick={() => void validateModel(entry.providerId, entry.modelId, "vision")}
>
  视觉测试
</Button>
```

For unknown capability confirmation, add a small select or inline buttons for `vision`, `long_context`, and `reasoner`; the button calls `confirmCapability(entry.providerId, entry.modelId, "vision")`.

- [ ] **Step 6: Update capability routing model selection**

Replace:

```ts
const models = routeModelsForProvider(providerId);
```

with:

```ts
const models = modelsForSlot(slot, providerId);
```

Replace model select item IDs:

```tsx
{models.map((model) => (
  <SelectItem key={model.modelId} value={model.modelId}>
    {model.displayName}
  </SelectItem>
))}
```

- [ ] **Step 7: Run frontend tests**

Run:

```bash
npm run test -- tests/model-provider-registry.test.ts tests/phase3-model-persona-routing.test.ts
npm run typecheck
```

Expected: PASS for the two test files; `typecheck` PASS.

- [ ] **Step 8: Commit**

```bash
git add src/components/settings/LlmRoutingSection.tsx tests/model-provider-registry.test.ts tests/phase3-model-persona-routing.test.ts
git commit -m "feat(ui): 重构模型供应商注册中心"
```

## Task 7: Background Refresh Scheduler

**Files:**
- Modify: `src/components/settings/LlmRoutingSection.tsx`
- Modify: `src/lib/ipc.ts`
- Modify: `src/types/llm.ts`
- Modify: `src-tauri/src/llm/model_registry.rs`

- [ ] **Step 1: Add refresh preference fields**

Add to `ModelRegistryEntry` in TypeScript and Rust if per-provider persisted metadata is chosen in the same table. Use provider-level registry rows with `model_id = '__provider__'` only if no separate table is added. Preferred small implementation: derive auto-refresh from Key existence and `lastRefreshedAt`, with no separate preference table in this task.

Add helper in `LlmRoutingSection.tsx`:

```ts
function providerNeedsRefresh(entries: ModelRegistryEntry[]): boolean {
  const newest = entries
    .map((entry) => entry.lastRefreshedAt)
    .filter((value): value is string => Boolean(value))
    .sort()
    .at(-1);
  if (!newest) return true;
  return Date.now() - new Date(newest).getTime() > 24 * 60 * 60 * 1000;
}
```

- [ ] **Step 2: Add idle refresh effect**

Inside `LlmRoutingSection`, after data/key status are loaded:

```ts
useEffect(() => {
  if (!open || !data) return;
  const timer = window.setTimeout(() => {
    for (const provider of data.providers) {
      if (!keyConfigured[provider.id]) continue;
      if (provider.id === "ollama") continue;
      const entries = registryForProvider(provider.id);
      if (!providerNeedsRefresh(entries)) continue;
      void refreshProviderModels(provider.id);
    }
  }, 5_000);
  return () => window.clearTimeout(timer);
}, [open, data, keyConfigured]);
```

This implements default background refresh for configured external providers while avoiding startup-time immediate network calls.

- [ ] **Step 3: Add frontend contract test**

In `tests/model-provider-registry.test.ts`, add:

```ts
it("background refresh only targets configured external providers", () => {
  const section = read("src/components/settings/LlmRoutingSection.tsx");

  expect(section).toContain("providerNeedsRefresh");
  expect(section).toContain("keyConfigured[provider.id]");
  expect(section).toContain('provider.id === "ollama"');
  expect(section).toContain("window.setTimeout");
});
```

- [ ] **Step 4: Run frontend tests**

Run:

```bash
npm run test -- tests/model-provider-registry.test.ts
npm run typecheck
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/LlmRoutingSection.tsx tests/model-provider-registry.test.ts
git commit -m "feat(ai): 自动刷新已配置供应商模型"
```

## Task 8: Documentation And Full Verification

**Files:**
- Modify: `docs/llm-routing.md`
- Modify: `docs/design-system.md`
- Modify: `ROADMAP.md`

- [ ] **Step 1: Update LLM routing docs**

In `docs/llm-routing.md`, replace the IPC bullet:

```md
- `llm_config_test`（GET /models，不记录 Key）
```

with:

```md
- `llm_config_test_provider`：验证供应商 Key/Base URL/认证路径，不绑定具体模型。
- `llm_model_registry_refresh`：对已配置的外部供应商刷新 `/models`，写入本地模型目录缓存。
- `llm_model_validate`：验证具体模型 ID；`text` 使用模型列表存在性或最小 chat 请求，`vision` 使用最小多模态请求。
- `llm_model_confirm_capability`：记录用户对未知模型专项能力的显式确认。
- `llm_config_test`：兼容旧入口；带 model 时代理到文字模型验证，不带 model 时代理到供应商测试。
```

- [ ] **Step 2: Update design system docs**

In `docs/design-system.md`, add to the Management Center section:

```md
- “模型与供应商”采用注册中心结构：供应商、模型目录、能力路由三段分离。供应商测试按钮只表达端点健康，模型验证按钮只表达具体模型可用性，专项能力测试需明确标注测试类型。
```

- [ ] **Step 3: Update roadmap**

In `ROADMAP.md`, add an entry under the AI/settings workstream:

```md
- 模型与供应商设置升级为模型注册中心：隐藏 Ollama 外部面板入口，支持已配置供应商后台刷新模型列表、具体模型验证、Vision 专项测试和未知能力手动确认。
```

- [ ] **Step 4: Run required verification**

Run:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands PASS. If `npm run test` fails on an unrelated local fixture, record the exact fixture path and run the focused tests from this plan plus any touched suites before reporting residual risk.

- [ ] **Step 5: Commit docs and final polish**

```bash
git add docs/llm-routing.md docs/design-system.md ROADMAP.md
git commit -m "docs(ai): 同步模型注册中心说明"
```

## Self-Review

- Spec coverage: The tasks cover separated provider/model tests, mixed model sources, background refresh for configured external providers, unknown capability gating, Vision-specific testing, Ollama hidden from the panel, docs, and verification.
- Placeholder scan: No task uses “TBD”, “TODO”, or “implement later”; all behavior-changing steps include concrete snippets or commands.
- Type consistency: Rust uses `ModelRegistryEntry`, `ModelRegistrySource`, `ModelValidationKind`, and `ModelCapabilityConfirmRequest`; TypeScript mirrors those names with camelCase fields from serde.
- Scope check: This is one coherent feature. It touches UI, IPC, storage, and docs, but each task produces independently testable progress.
