---
title: 多模态模型路由与图片输入管道设计
created: 2026-06-14
revision: 2026-06-14-2
scope: ai_types, llm, ai_runtime/model_gateway, ai_commands, assistant_commands, session_messages, ai-composer, AiMessageBubble, useAssistantTasks, assistant-routing, ipc, config
---

# 多模态模型路由与图片输入管道设计

## 摘要

Iris 的 AI 模型路由引擎（CapabilitySlot 架构）已完全实现：九槽位路由、降级链、Vision 意图检测均正确工作。但图片从用户输入到 LLM API 的整个数据管道完全缺失——AiComposer 无图片输入能力、IPC 不支持图片传输、`LlmMessage` 类型为纯文本、API Body 只序列化字符串。

本设计打通五层管道，同时优化默认配置（Vision 槽位默认模型改为 MiMo-V2.5），实现用户期望的"有图片自动切 vision 模型、无图片自动切回 fast 模型"。

所有改动为增量添加，纯文本消息路径完全不受影响。

---

## Cluster A：类型系统改造（底层基础）

### A1. Rust — `MessageContent` 与 `ContentPart`

**改动文件**：`src-tauri/src/ai_types/mod.rs`

新增多模态内容类型（约 L1150 附近，`LlmMessage` 定义处）：

```rust
/// 消息内容：纯文本或混合多模态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本（向后兼容）
    Text(String),
    /// 多模态内容数组
    Parts(Vec<ContentPart>),
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self { MessageContent::Text(s) }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self { MessageContent::Text(s.to_string()) }
}

/// 内容片段（遵循 OpenAI multimodal 格式，可转换为 Anthropic 格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrlPayload,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlPayload {
    pub url: String,           // "data:image/png;base64,xxxxx" 或 HTTP URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>, // "auto" | "low" | "high"
}
```

`LlmMessage` 改动 — `content` 字段类型变化：

```rust
// Before:
pub struct LlmMessage {
    pub content: String,
    // ...
}

// After:
pub struct LlmMessage {
    pub content: MessageContent,
    // ...
}
```

**向后兼容**：`MessageContent::Text(String)` 序列化为 JSON 字符串，与现有格式完全一致。`#[serde(untagged)]` 确保反序列化时纯字符串自动映射为 `Text` 变体。

**调用处适配**：所有构造 `LlmMessage { content: "xxx".into(), ... }` 的位置需改为 `content: MessageContent::from("xxx")` 或直接 `"xxx".into()`。

### A2. Rust — `ImageAttachmentDto` IPC 类型

**改动文件**：`src-tauri/src/commands/ai_commands.rs`（靠近 `AiSendRoutingOverride` 结构体处）

```rust
/// 前端传入的图片附件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachmentDto {
    pub id: String,            // UUID
    pub data_base64: String,   // 不含 data:xxx;base64, 前缀的纯 base64
    pub mime_type: String,     // "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    pub file_name: Option<String>,
    pub size_bytes: u64,
}

impl ImageAttachmentDto {
    /// 构造 data URL（用于 LLM API）
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime_type, self.data_base64)
    }

    /// 单张图片转换为 ContentPart
    pub fn to_content_part(&self) -> ContentPart {
        ContentPart::ImageUrl {
            image_url: ImageUrlPayload {
                url: self.data_url(),
                detail: Some("auto".into()),
            },
        }
    }
}
```

### A3. TypeScript — 类型同步

**文件**：`src/types/ai.ts`

在 `AgentIntent` / `CapabilitySlot` 区域附近新增：

```typescript
/** 消息内容：纯文本字符串或多模态片段数组 */
export type MessageContent = string | ContentPart[];

export type ContentPart =
  | { type: "text"; text: string }
  | {
      type: "image_url";
      image_url: { url: string; detail?: "auto" | "low" | "high" };
    };
```

**文件**：`src/components/ai/AiMessageList.tsx` — `ChatLine` 接口扩展：

```typescript
export interface ImageAttachment {
  id: string;
  dataBase64: string;
  mimeType: string;
  fileName?: string;
  sizeBytes: number;
}

export interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
  /** 多模态原始数据（传给后端）；纯文本时为 undefined */
  contentParts?: ContentPart[];
  /** 前端渲染用图片列表 */
  images?: ImageAttachment[];
  seq?: number;
  created_at?: string;
  toolCalls?: ToolCallInfo[];
  kind?: "research";
  research?: ResearchFocusPayload;
}
```

**文件**：`src/types/ipc.ts` — IPC 参数扩展：

```typescript
/** ai_send_message / assistant_execute 图片附件 */
export interface ImageAttachmentDto {
  id: string;
  dataBase64: string;
  mimeType: string;
  fileName?: string;
  sizeBytes: number;
}
```

---

## Cluster B：会话存储改造

### B1. 数据库 Migration

**新文件**：`src-tauri/migrations/028_multimodal_messages.sql`（当前最大 migration 为 027）

```sql
-- 为 session_messages 新增多模态内容列
-- content 保留为文本摘要/占位，content_parts 存储完整的 ContentPart[] JSON
ALTER TABLE session_messages ADD COLUMN content_parts TEXT;

-- content_parts 为 NULL 时消息视为纯文本（向后兼容）
-- content_parts 有值时消息内容以 content_parts 为准
```

对应 down migration：`028_multimodal_messages.down.sql`

```sql
-- 回滚：删除 content_parts 列（SQLite 不支持 DROP COLUMN，仅做无操作）
-- 旧数据不受影响，content_parts 为 NULL 即为纯文本
```

### B2. Rust 数据库层适配

**文件**：`src-tauri/src/ai_runtime/session_manager.rs`（或相应 session 管理模块）

`SessionMessage` 结构体扩展：

```rust
pub struct SessionMessage {
    pub role: String,
    pub content: String,                   // 纯文本摘要
    pub content_parts: Option<String>,     // JSON: Vec<ContentPart>
    pub tool_calls: Option<String>,
    // ... 其余字段
}
```

存储逻辑（`append_message`）：

```rust
// 生成 content 摘要
let text_summary = if let Some(ref parts) = content_parts_json {
    // 提取 text parts 拼接为摘要
    extract_text_summary(parts)
} else {
    user_message.clone()
};

// 写入数据库
conn.execute(
    "INSERT INTO session_messages (session_id, role, content, content_parts, ...) VALUES (?, ?, ?, ?, ...)",
    params![session_id, "user", text_summary, content_parts_json, ...],
)?;
```

读取逻辑：

```rust
// 读取历史消息时，优先使用 content_parts 重建 LlmMessage
let content: MessageContent = if let Some(ref parts_json) = row.content_parts {
    let parts: Vec<ContentPart> = serde_json::from_str(parts_json)?;
    MessageContent::Parts(parts)
} else {
    MessageContent::Text(row.content)
};
```

---

## Cluster C：IPC 传输管道

### C1. `ai_send_message` 命令扩展

**文件**：`src-tauri/src/commands/ai_commands.rs`

```rust
#[tauri::command]
pub async fn ai_send_message(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    scene: String,
    session_id: Option<i64>,
    message: String,
    images: Option<Vec<ImageAttachmentDto>>,   // 新增
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScope>,
    web_search: Option<bool>,
    new_session: Option<bool>,
) -> AppResult<serde_json::Value> {
    execute_ai_send_message_with_routing(
        &state, &app_handle, AiSendMessageInput {
            scene, session_id, message, images, // 透传
            selected_packet_ids, note_path, context_scope, web_search, new_session,
        }
    ).await
}
```

`AiSendMessageInput` 内部结构同样扩展 `images: Option<Vec<ImageAttachmentDto>>`。

### C2. `AssistantExecuteRequest` 扩展

**文件**：`src-tauri/src/commands/assistant_commands.rs`

```rust
pub struct AssistantExecuteRequest {
    pub message: String,
    pub images: Option<Vec<ImageAttachmentDto>>,  // 新增
    // ... 其余字段不变
}
```

内部路由函数（`route_assistant_execute`）透传 `images` 给下游。

### C3. 前端 IPC 封装

**文件**：`src/lib/ipc.ts`

```typescript
export async function aiSendMessage(params: {
  scene: AiScene;
  session_id: number | null;
  message: string;
  images?: ImageAttachmentDto[]; // 新增
  note_path?: string | null;
  selected_packet_ids?: string[];
  context_scope?: ContextScope | null;
  web_search?: boolean;
}): Promise<AiSendMessageResult> {
  return invoke("ai_send_message", { ...params });
}

export async function assistantExecute(
  request: AssistantExecuteRequest & { images?: ImageAttachmentDto[] },
): Promise<AssistantExecuteResponse> {
  return invoke("assistant_execute", { request });
}
```

---

## Cluster D：API Body 构造

### D1. 消息序列化 — `messages_for_api()`

**文件**：`src-tauri/src/ai_runtime/model_gateway/messages.rs`

核心改动在消息转 JSON 函数（当前约 L193-197）：

```rust
fn llm_message_to_api_json(msg: &LlmMessage) -> serde_json::Value {
    let role = role_to_api_str(msg.role);

    match &msg.content {
        MessageContent::Text(text) => {
            serde_json::json!({
                "role": role,
                "content": text,
            })
        }
        MessageContent::Parts(parts) => {
            serde_json::json!({
                "role": role,
                "content": parts,  // 直接序列化 Vec<ContentPart>
            })
        }
    }
}
```

对于 OpenAI 兼容端点（DeepSeek、MiMo、OpenAI），输出格式：

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "这张图片里有什么？" },
    {
      "type": "image_url",
      "image_url": { "url": "data:image/png;base64,iVBOR...", "detail": "auto" }
    }
  ]
}
```

### D2. Anthropic 格式转换

**文件**：`src-tauri/src/ai_runtime/model_gateway/body.rs`（`build_anthropic_messages_body_inner`）

当 `EndpointFamily` 为 `AnthropicMessages` 时，需将 `ImageUrl` 转换为 Anthropic 格式：

```rust
fn content_part_to_anthropic(part: &ContentPart) -> serde_json::Value {
    match part {
        ContentPart::Text { text } => {
            json!({ "type": "text", "text": text })
        }
        ContentPart::ImageUrl { image_url } => {
            // 从 "data:image/png;base64,xxxxx" 解析
            let (media_type, data) = parse_data_url(&image_url.url);
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": data,
                }
            })
        }
    }
}

/// 解析 data URL: "data:image/png;base64,xxxxx" → ("image/png", "xxxxx")
fn parse_data_url(url: &str) -> (&str, &str) {
    // "data:image/png;base64,xxxxx"
    let after_data = url.strip_prefix("data:").unwrap_or(url);
    let comma_pos = after_data.find(',').unwrap_or(after_data.len());
    let media_type = &after_data[..comma_pos - ";base64".len()];
    let data = &after_data[comma_pos + 1..];
    (media_type, data)
}
```

Anthropic 输出格式：

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "..." },
    {
      "type": "image",
      "source": {
        "type": "base64",
        "media_type": "image/png",
        "data": "iVBOR..."
      }
    }
  ]
}
```

### D3. Prompt Builder — 初始消息组装

**文件**：`src-tauri/src/ai_runtime/prompt_builder.rs`（`build_initial_messages` 函数）

用户消息组装逻辑改动（约 L188-195）：

```rust
// 构建用户消息的 content
let user_content = if let Some(images) = &run_input.images {
    let mut parts = vec![ContentPart::Text {
        text: run_input.user_message.clone(),
    }];
    for img in images {
        parts.push(img.to_content_part());
    }
    MessageContent::Parts(parts)
} else {
    MessageContent::Text(run_input.user_message.clone())
};

messages.push(LlmMessage {
    role: MessageRole::User,
    content: user_content,
    tool_call_id: None,
    tool_calls: None,
    reasoning_content: None,
});
```

### D4. Harness Run Input 扩展

**文件**：`src-tauri/src/ai_harness/harness/run.rs` 或相关结构体定义处

```rust
pub struct HarnessRunInput {
    pub user_message: String,
    pub images: Option<Vec<ImageAttachmentDto>>,  // 新增
    // ... 其余字段
}
```

---

## Cluster E：前端 UI

### E1. AiComposer — 图片输入

**文件**：`src/components/ui/ai-composer.tsx`

新增 Props：

```typescript
interface AiComposerProps {
  // ... 现有 props ...
  images?: ImageAttachment[];
  onImagesChange?: (images: ImageAttachment[]) => void;
}
```

新增功能：

**1. 粘贴图片** — 在 `<textarea>` 上添加 `onPaste` 处理：

```typescript
const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
  const items = Array.from(e.clipboardData.items);
  const imageFiles = items
    .filter((item) => item.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .filter((f): f is File => f !== null);

  if (imageFiles.length > 0) {
    e.preventDefault();
    processImageFiles(imageFiles);
  }
};
```

**2. 拖拽图片** — 在 composer 容器上添加 `onDrop`：

```typescript
const handleDrop = (e: React.DragEvent) => {
  const files = Array.from(e.dataTransfer.files).filter((f) =>
    f.type.startsWith("image/"),
  );
  if (files.length > 0) {
    e.preventDefault();
    processImageFiles(files);
  }
};

const handleDragOver = (e: React.DragEvent) => {
  if (Array.from(e.dataTransfer.types).includes("Files")) {
    e.preventDefault();
  }
};
```

**3. 文件选择按钮** — 在发送按钮旁添加附件按钮：

```typescript
const fileInputRef = useRef<HTMLInputElement>(null);

// JSX:
<input
  ref={fileInputRef}
  type="file"
  accept="image/*"
  multiple
  className="hidden"
  onChange={(e) => {
    const files = Array.from(e.target.files || []);
    if (files.length > 0) processImageFiles(files);
    e.target.value = ""; // reset 以支持重复选择同一文件
  }}
/>
<Button
  type="button"
  size="icon"
  variant="ghost"
  className="h-8 w-8 shrink-0"
  onClick={() => fileInputRef.current?.click()}
  aria-label="添加图片"
>
  <Paperclip className="h-4 w-4" />
</Button>
```

**4. 图片处理函数**（组件内部）：

```typescript
async function processImageFiles(files: File[]) {
  const newImages: ImageAttachment[] = [];
  for (const file of files) {
    // 大小限制：单张最大 20MB（OpenAI 限制）
    if (file.size > 20 * 1024 * 1024) continue;
    // MIME 类型白名单
    if (
      !["image/png", "image/jpeg", "image/webp", "image/gif"].includes(
        file.type,
      )
    )
      continue;

    const dataBase64 = await fileToBase64(file);
    newImages.push({
      id: crypto.randomUUID(),
      dataBase64,
      mimeType: file.type,
      fileName: file.name,
      sizeBytes: file.size,
    });
  }
  onImagesChange?.([...(images || []), ...newImages]);
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      // 去掉 "data:image/png;base64," 前缀
      const result = (reader.result as string).split(",")[1];
      resolve(result);
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}
```

**5. 图片 Pills** — 在 textarea 上方展示已附加图片的缩略图：

```typescript
{images && images.length > 0 && (
  <div className="flex flex-wrap gap-1.5 mb-2">
    {images.map((img) => (
      <div
        key={img.id}
        className="relative group h-10 w-10 rounded-md overflow-hidden border border-border/50"
      >
        <img
          src={`data:${img.mimeType};base64,${img.dataBase64}`}
          className="h-full w-full object-cover"
          alt={img.fileName || ""}
        />
        <button
          type="button"
          className="absolute -top-1 -right-1 h-4 w-4 rounded-full bg-destructive text-destructive-foreground text-[10px] opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center"
          onClick={() => onImagesChange?.(images.filter((i) => i.id !== img.id))}
          aria-label="移除图片"
        >
          ×
        </button>
      </div>
    ))}
  </div>
)}
```

### E2. useAssistantTasks — 状态与意图联动

**文件**：`src/components/ai/hooks/useAssistantTasks.ts`

新增状态：

```typescript
const [images, setImages] = useState<ImageAttachment[]>([]);
```

`send()` 函数改动（约 L821-832）：

```typescript
const send = useCallback(async () => {
  if ((!input.trim() && images.length === 0) || composerDisabled) return;

  const rawMessage = input.trim();
  const intentDetection = detectAgentIntent({
    message: rawMessage,
    hasImage: images.length > 0, // 新增
    hasSelection: Boolean(getWritingContext()?.selection || selectionQuoteText),
    notePath,
    explicitScope:
      contextScope.paths.length > 0 || contextScope.pathPrefixes.length > 0,
  });

  // ...
  setInput("");
  setImages([]); // 发送后清空图片
  appendUserMessage(rawMessage, images); // 新增 images 参数

  // IPC 调用传递 images
  await runKnowledgeChat(rawMessage, intent, {
    startNewSession,
    agentIntent,
    intentDetection,
    images, // 新增
  });
  // ...
});
```

`appendUserMessage` 签名变更：

```typescript
const appendUserMessage = (content: string, imgs?: ImageAttachment[]) => {
  setMessages((prev) => [
    ...prev,
    {
      role: "user",
      content: imgs?.length ? `[图片] ${content}` : content,
      images: imgs, // 新增
    } as ChatLine,
  ]);
};
```

`runKnowledgeChat` 透传 `images` 给 `aiSendMessage()`。

### E3. AiMessageBubble — 用户图片渲染

**文件**：`src/components/ai/AiMessageBubble.tsx`

用户消息气泡新增图片渲染（在文本内容下方）：

```typescript
{chatLine.role === "user" && chatLine.images && chatLine.images.length > 0 && (
  <div className="flex flex-wrap gap-2 mt-2">
    {chatLine.images.map((img) => (
      <img
        key={img.id}
        src={`data:${img.mimeType};base64,${img.dataBase64}`}
        className="max-w-60 max-h-60 rounded-lg border border-border/40 object-contain cursor-pointer hover:opacity-90 transition-opacity"
        alt={img.fileName || "attached image"}
        onClick={() => {
          // 可选：点击放大图片（Lightbox）
        }}
      />
    ))}
  </div>
)}
```

### E4. 模型路由可视化（可选增强）

**文件**：`src/components/ai/ConversationSurface.tsx` 或 `AiPanel.tsx` 底部状态栏

利用已有的 `CapabilityRouteSummary`（通过 `AgentRunPlanSummary.model_route` 传递）：

```typescript
{lastRunPlan?.model_route && (
  <div className="px-3 py-1.5 text-[11px] text-muted-foreground flex items-center gap-1.5">
    <span className="opacity-50">模型</span>
    <span className="font-medium text-foreground/70">
      {lastRunPlan.model_route.providerId}/{lastRunPlan.model_route.model}
    </span>
    {lastRunPlan.model_route.degraded && (
      <span className="text-amber-500" title={lastRunPlan.model_route.reason}>
        (降级)
      </span>
    )}
  </div>
)}
```

---

## Cluster F：默认配置优化

### F1. Vision 槽位默认模型

**文件**：`src-tauri/src/llm/config.rs` — `deepseek_defaults()` 函数（约 L297-304）

```rust
// Before:
slots.insert(
    "vision".into(),
    SlotRoute {
        provider_id: "openai".into(),
        model: "gpt-4o".into(),
        thinking: false,
    },
);

// After:
slots.insert(
    "vision".into(),
    SlotRoute {
        provider_id: "mimo".into(),
        model: "MiMo-V2.5".into(),
        thinking: false,
    },
);
```

### F2. 对应测试更新

**文件**：`src-tauri/src/llm/config.rs` — `defaults_route_vision_to_openai` 测试（约 L1036）

```rust
// 更新断言：
assert_eq!(
    c.slots.get("vision").map(|r| r.provider_id.as_str()),
    Some("mimo")  // was: Some("openai")
);
assert_eq!(
    c.slots.get("vision").map(|r| r.model.as_str()),
    Some("MiMo-V2.5")  // was: Some("gpt-4o")
);
```

---

## 路由验证：端到端场景

用户期望的场景通过以下路径实现：

**场景 A：纯文本对话**

```
用户输入 "帮我总结这篇文章"
→ detectAgentIntent(hasImage=false) → "chat"
→ requested_slot(Chat, ...) → "fast"
→ 路由到 deepseek-v4-flash ✓
```

**场景 B：上传图片后对话**

```
用户粘贴图片 + 输入 "这张图里有什么数据"
→ AiComposer 粘贴处理 → images: [{dataBase64, mimeType}]
→ send() → detectAgentIntent(hasImage=true) → "vision_chat"
→ ai_send_message({message, images}) → Rust 后端
→ resolve_capability_route(VisionChat, has_images=true) → "vision" slot
→ 路由到 MiMo-V2.5 ✓
→ prompt_builder 构建 MessageContent::Parts([Text, ImageUrl])
→ messages_for_api() 生成 OpenAI multimodal JSON
→ POST to MiMo API ✓
```

**场景 C：图片对话后继续纯文本**

```
下一轮：用户输入 "那这个趋势说明什么"（无新图片）
→ detectAgentIntent(hasImage=false) → "chat"
→ requested_slot(Chat, ...) → "fast"
→ 自动切回 deepseek-v4-flash ✓
```

---

## 测试策略

### Rust 测试

| 测试项                          | 文件              | 要点                                     |
| ------------------------------- | ----------------- | ---------------------------------------- |
| `MessageContent` 序列化往返     | `ai_types/mod.rs` | Text ↔ JSON string; Parts ↔ JSON array   |
| `ContentPart` 转 Anthropic 格式 | `body.rs`         | ImageUrl → `{type: "image", source: {}}` |
| Vision 路由降级链               | `config.rs`       | MiMo-V2.5 不可用 → Fallback to Fast      |
| `messages_for_api()` 多模态输出 | `messages.rs`     | Parts 输入 → `content: [...]` 数组       |
| 纯文本消息不受影响              | 整体              | 无图片时行为与改变前一致                 |

### TypeScript 测试

| 测试项                                 | 文件                                | 要点                       |
| -------------------------------------- | ----------------------------------- | -------------------------- |
| `detectAgentIntent` hasImage=true      | `unified-assistant-routing.test.ts` | 已有测试（L163），确认通过 |
| AiComposer 粘贴图片 → `onImagesChange` | 新增组件测试                        | FileReader mock            |
| 图片 Pill 渲染与删除                   | 新增组件测试                        | 渲染验证                   |

---

## 受影响的文件清单

```
src-tauri/src/ai_types/mod.rs          — MessageContent / ContentPart 类型定义
src-tauri/src/llm/config.rs            — deepseek_defaults() vision 默认值
src-tauri/src/llm/model_catalog.rs     — (无需改动，MiMo-V2.5 已正确标记 supports_vision)
src-tauri/src/ai_runtime/prompt_builder.rs  — build_initial_messages() 组装多模态内容
src-tauri/src/ai_runtime/model_gateway/messages.rs  — messages_for_api() 序列化
src-tauri/src/ai_runtime/model_gateway/body.rs  — Anthropic 格式转换
src-tauri/src/ai_harness/harness/run.rs       — HarnessRunInput 扩展
src-tauri/src/commands/ai_commands.rs         — ai_send_message + ImageAttachmentDto
src-tauri/src/commands/assistant_commands.rs   — AssistantExecuteRequest 扩展
src-tauri/src/ai_runtime/session_manager.rs    — SessionMessage 内容字段
src-tauri/migrations/028_multimodal_messages.sql — 新增 migration（含 .down.sql）

src/types/ai.ts                       — MessageContent / ContentPart TS 类型
src/types/ipc.ts                      — ImageAttachmentDto TS 类型
src/lib/ipc.ts                        — aiSendMessage / assistantExecute 参数
src/lib/assistant-routing.ts          — (无需改动，hasImage 已定义)
src/components/ui/ai-composer.tsx     — 粘贴/拖拽/选择图片 + Pills
src/components/ai/AiMessageBubble.tsx — 用户消息图片渲染
src/components/ai/AiMessageList.tsx   — ChatLine + ImageAttachment 类型
src/components/ai/hooks/useAssistantTasks.ts — images 状态 + send() 联动
src/components/ai/ConversationSurface.tsx — (可选) 模型路由标签
```

**总计**：约 18 个文件，改动分散但每层独立、可增量验证。
