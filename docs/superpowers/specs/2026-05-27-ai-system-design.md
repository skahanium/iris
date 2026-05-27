# Iris AI 体系设计

> 设计日期: 2026-05-27
> 修订日期: 2026-05-27
> 状态: 修订版草案
> 版本: v1.1

---

## 一、修订结论

Iris 的 AI 体系不应以"多 Agent"为中心，而应以**本地优先的知识基座、可治理的场景工作流、可追溯的证据包、可评测的工具运行时**为中心。

原 v1.0 的方向是合理的：知识查阅、文稿学习、文稿创作、学术研究四个场景确实对应不同上下文、工具和风险等级。但 v1.0 把过多能力包装为 Agent，容易形成名词化架构：看起来完整，实际难以评测、难以控制写入边界，也容易让 TypeScript 前端承担不该承担的安全职责。

v1.1 的核心调整：

- **Workflow 优先**：知识查阅、文稿学习、文稿创作优先采用确定性工作流；只有学术研究允许有限的 agentic loop。
- **Rust 运行时优先**：模型调用、工具路由、权限校验、上下文组装、追踪与评测放在 Rust 后端；React 只负责交互和确认。
- **证据包优先**：RAG 输出结构化 `ContextPacket`，回答必须基于可追溯证据，而不是把 Top-K 文本粗暴拼进 prompt。
- **数据边界优先**：`.md` 永远是用户知识的唯一权威来源；SQLite 中的内容副本只能是可重建缓存、显式偏好或可删除会话。
- **治理内建**：guardrails、tool permission、trace、eval 从第一阶段就进入架构，不作为上线后的补丁。

### 业界理念校准

v1.1 吸收但不盲从当前 AI 应用架构趋势：

- **从 Agent 叙事回到 workflow 工程**：多数产品场景不需要长期自治代理，需要的是清晰状态机、明确工具边界和可复现行为。
- **从长上下文迷信回到 context engineering**：大窗口可作为能力上限，但默认仍要做检索规划、重排、压缩、引用和预算控制。
- **从 RAG 拼接回到 grounded evidence**：检索结果必须带来源、span、hash、trust level 和引用标签，模型回答需能回查证据。
- **从功能优先回到治理优先**：guardrails、human-in-the-loop、trace、eval、安全回归集是 AI runtime 的基础设施，而非发布后的补丁。
- **从协议崇拜回到产品边界**：MCP 等协议的权限、同意、可见性思想值得吸收，但 Iris 不开放第三方插件/Skills 运行时。

---

## 二、设计目标

构建一个融入 Iris 笔记和写作流的 AI 系统：

- 帮用户在本地知识库中查找、解释、引用材料。
- 帮用户学习范文结构和表达方式，但不替代用户判断。
- 帮用户在文稿创作中生成、改写、检查、引用内容。
- 帮用户对多材料进行论证组织和证据缺口分析。

### 设计原则

| 原则              | 说明                                                       |
| ----------------- | ---------------------------------------------------------- |
| 本地优先          | 索引、检索、权限、缓存、会话默认在本机完成。               |
| Markdown 权威     | 用户 `.md` 文件是唯一知识资产；SQLite 可删可重建。         |
| Workflow 优先     | 先使用清晰、可测试的工作流；仅在研究场景引入有限自治循环。 |
| Evidence-first    | 模型回答基于带来源、hash、span、score 的证据包。           |
| Human-in-the-loop | 写入 `.md`、修改规则、联网研究、持久化偏好均需清晰授权。   |
| 可观测可评测      | 每次检索、工具调用、上下文组装和引用校验都可追踪。         |
| 最小权限          | 工具按场景裁剪，读写权限分级，网络能力默认关闭。           |

---

## 三、场景与自治等级

### 3.1 场景表

| 场景     | Runtime Profile    | 会话范围       | 自治等级             | 核心产出                           |
| -------- | ------------------ | -------------- | -------------------- | ---------------------------------- |
| 知识查阅 | Knowledge Lookup   | 库级或当前笔记 | L1 工作流            | 条款解释、笔记引用、关联材料       |
| 文稿学习 | Exemplar Learning  | 当前范文       | L1 工作流            | 结构拆解、表达特征、可确认模板     |
| 文稿创作 | Drafting Assist    | 当前草稿       | L2 工具工作流        | 结构建议、段落生成、改写、引用建议 |
| 学术研究 | Research Synthesis | 库级项目       | L3 有限 agentic loop | 子命题、证据矩阵、论证链、缺口     |

### 3.2 自治等级

| 等级 | 含义                               | Iris 允许范围                       |
| ---- | ---------------------------------- | ----------------------------------- |
| L0   | 纯规则或本地检索，无 LLM 决策      | 分块、FTS、法规正则解析、引用格式化 |
| L1   | 单轮 LLM + 受控上下文，无工具循环  | 知识问答、范文结构分析、总结        |
| L2   | 工作流中允许一次或少量工具调用     | 文稿创作、引用建议、改写对比        |
| L3   | 有限循环：计划、检索、汇总、校验   | 学术研究；限制最大轮数和工具次数    |
| L4   | 自主长期代理、自动写文件、任意脚本 | 禁止                                |

### 3.3 命名约定

文档中保留"知识管家、写作伴侣、研究助理"作为**产品人格和 prompt profile**，而不是三个独立自治 Agent：

- **知识管家**：用于知识查阅和文稿学习的回答风格与工具裁剪。
- **写作伴侣**：用于文稿创作的写作风格、确认交互和草稿上下文。
- **研究助理**：用于学术研究的分解、检索、证据矩阵和缺口分析。

---

## 四、总体架构

```
┌────────────────────────────────────────────────────────────┐
│ React 前端                                                   │
│ ┌──────────────┐ ┌──────────────┐ ┌─────────────────────┐  │
│ │ SceneSelector│ │ AI Panel     │ │ Confirmation UI     │  │
│ │ 场景切换      │ │ 对话/引用/追踪 │ │ diff/预览/权限确认    │  │
│ └──────┬───────┘ └──────┬───────┘ └─────────┬───────────┘  │
└────────┼────────────────┼───────────────────┼──────────────┘
         │ typed IPC       │ stream events      │ confirmed args
┌────────▼────────────────▼───────────────────▼──────────────┐
│ Rust AI Runtime                                              │
│ ┌──────────────┐ ┌──────────────┐ ┌─────────────────────┐  │
│ │ Scene Router │ │ Context      │ │ Model Gateway       │  │
│ │ 工作流路由    │ │ Planner      │ │ provider/stream/tool│  │
│ └──────┬───────┘ └──────┬───────┘ └─────────┬───────────┘  │
│        │                │                   │              │
│ ┌──────▼───────┐ ┌──────▼───────┐ ┌─────────▼───────────┐  │
│ │ Retrieval    │ │ ContextPacket│ │ Tool Executor       │  │
│ │ Broker       │ │ Builder      │ │ 权限/校验/确认       │  │
│ └──────┬───────┘ └──────┬───────┘ └─────────┬───────────┘  │
│        │                │                   │              │
│ ┌──────▼────────────────▼───────────────────▼───────────┐  │
│ │ Guardrails + Trace + Eval hooks                        │  │
│ │ prompt injection 防护 / 引用校验 / 工具审计 / 评测采样   │  │
│ └────────────────────────────────────────────────────────┘  │
└────────┬───────────────────────────────────────────────────┘
         │
┌────────▼───────────────────────────────────────────────────┐
│ Local Knowledge Cache                                       │
│ .md 文件 │ SQLite + sqlite-vec │ OS 凭据管理器 │ 可删除会话缓存 │
└────────────────────────────────────────────────────────────┘
```

### 4.1 前后端职责

| 层              | 职责                                                             | 不做                                                          |
| --------------- | ---------------------------------------------------------------- | ------------------------------------------------------------- |
| React           | 场景选择、引用卡、工具调用气泡、确认弹窗、diff 预览、流式渲染    | 不直接拼接高权限工具请求；不保存 API Key；不绕过 IPC 类型封装 |
| Rust AI Runtime | 上下文组装、检索规划、模型调用、工具权限、会话持久化、trace/eval | 不自动改写 `.md`；不把 API Key 输出到日志                     |
| SQLite          | 索引、缓存、会话、偏好、追踪元数据                               | 不作为用户知识的权威来源                                      |
| `.md`           | 用户知识与最终写作成果                                           | 不被 AI 静默修改                                              |

---

## 五、核心运行流

### 5.1 标准请求流

```
用户输入
  -> React 发送 scene + note_path + note_content_hash + query
  -> Rust Scene Router 选择工作流
  -> Context Planner 生成检索计划
  -> Retrieval Broker 执行混合检索
  -> ContextPacket Builder 生成证据包
  -> Guardrails 校验证据与工具权限
  -> Model Gateway 调用 provider
  -> 流式返回内容/工具请求
  -> Tool Executor 执行只读工具或请求用户确认写入
  -> Citation Verifier 校验输出引用
  -> Trace/Eval hooks 记录元数据
```

### 5.2 场景切换

场景切换不应简单清空 UI，而应切换到对应 `session_key`：

```
session_key = scene + ":" + (note_path 或 "__global__")
```

切换流程：

1. 保存当前 UI 草稿输入和滚动状态。
2. 结束当前流式请求或提示用户取消。
3. 加载目标场景的可删除会话缓存。
4. 重新计算当前笔记 hash 和 context status。
5. 显示该场景的上下文卡和历史消息。

---

## 六、ContextPacket 证据包

### 6.1 为什么需要证据包

仅把 Top-K 文本塞进 system prompt 会带来四个问题：

- 模型难以区分"用户指令"和"检索材料"。
- 回答无法稳定引用来源。
- 长上下文会增加成本和延迟，不保证推理质量。
- 无法评测哪条证据支撑了哪段回答。

因此检索结果必须先变成结构化证据包，再进入 prompt。

### 6.2 数据结构

```typescript
interface ContextPacket {
  id: string;
  source_type:
    | "note"
    | "anchor"
    | "regulation"
    | "template"
    | "session"
    | "web";
  source_path: string | null;
  title: string;
  heading_path: string | null;
  source_span: { start: number; end: number } | null;
  content_hash: string;
  excerpt: string;
  retrieval_reason: string;
  score: number;
  trust_level:
    | "user_note"
    | "derived_cache"
    | "external_web"
    | "model_generated";
  citation_label: string;
  stale: boolean;
}
```

### 6.3 组装策略

| 阶段             | 动作                                | 说明                                   |
| ---------------- | ----------------------------------- | -------------------------------------- |
| Intent           | 判断任务类型                        | 查阅、创作、学习、研究、改写、引用     |
| Query Plan       | 生成 1-N 个检索子查询               | 研究场景才允许多轮拆解                 |
| Hybrid Retrieval | FTS + vector + graph + exact parser | 法规优先 exact，普通笔记用 hybrid      |
| Rerank           | 分数融合和轻量重排                  | 优先规则和本地分数；必要时 LLM rerank  |
| Packet Budget    | 按模型能力裁剪                      | 先证据质量，后 token 数量              |
| Citation Check   | 回答后校验引用                      | 未被证据包支撑的结论要降级或标注不确定 |

### 6.4 Token 预算

不再假设"1M context 足够所以 30K 很小"。上下文窗口大不等于注意力、成本和延迟免费。

| 场景     | 默认目标 | 上限策略                                     |
| -------- | -------- | -------------------------------------------- |
| 知识查阅 | 4K-8K    | 超出时压缩证据包，只保留原文引用             |
| 文稿学习 | 8K-15K   | 当前范文可分段分析，不默认全文注入           |
| 文稿创作 | 8K-20K   | 当前草稿按光标邻域 + 大纲摘要 + 必要证据组合 |
| 学术研究 | 15K-40K  | 子命题分批执行，最终汇总证据矩阵             |

---

## 七、检索与知识基座

### 7.1 检索层级

| 层级         | 数据                                           | 用途                           |
| ------------ | ---------------------------------------------- | ------------------------------ |
| FTS          | `files_fts`、chunk 文本                        | 关键词、法规名称、专有名词     |
| Vector       | `vec_chunks`、`vec_anchors`、`vec_regulations` | 语义相似、表述变体             |
| Graph        | 双向链接、标签、锚点链接                       | 关联扩展、引用上下游           |
| Exact Parser | 法规条款、日期、编号、标题                     | 可验证的结构化命中             |
| Template     | 文种模板、结构特征                             | 写作建议与范文学习             |
| Web          | 可选联网搜索                                   | 仅在用户显式开启时补充外部资料 |

### 7.2 语义锚点

语义锚点是索引缓存，不是 Markdown 的替代品。锚点 ID 必须稳定，不能把 SQLite 自增 ID 暴露到 Markdown 链接里。

```sql
CREATE TABLE semantic_anchors (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    anchor_key        TEXT NOT NULL UNIQUE,
    file_id           INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    anchor_type       TEXT NOT NULL,
    content           TEXT NOT NULL,
    heading_path      TEXT,
    source_start      INTEGER NOT NULL,
    source_end        INTEGER NOT NULL,
    paragraph_index   INTEGER,
    content_hash      TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    embedding_model   TEXT NOT NULL,
    embedding_dim     INTEGER NOT NULL,
    confidence        REAL NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE VIRTUAL TABLE vec_anchors USING vec0(embedding float[384]);
```

`anchor_key` 建议由 `normalized_path + source_span + content_hash` 派生，再生成短稳定标识。Markdown 若需要块级链接，使用 `[[笔记名#^anchor_key]]` 这类稳定 key，而不是数据库 rowid。

### 7.3 块级链接图谱

```sql
CREATE TABLE block_links (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    source_anchor_key  TEXT,
    target_file_id     INTEGER REFERENCES files(id) ON DELETE CASCADE,
    target_anchor_key  TEXT,
    link_type          TEXT NOT NULL,
    confidence         REAL NOT NULL DEFAULT 1.0,
    is_confirmed       INTEGER NOT NULL DEFAULT 0,
    created_by         TEXT NOT NULL,
    context_hash       TEXT,
    created_at         TEXT NOT NULL
);
```

`link_type` 可取：

- `explicit`：用户写下的 `[[...]]`。
- `implicit`：AI 建议的隐含关联，默认不写入 `.md`。
- `regulation_ref`：法规条款引用。
- `template_ref`：范文结构引用。

### 7.4 法规条款索引

法规条款必须支持 exact match 和 source verification。

```sql
CREATE TABLE regulation_index (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id            INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    regulation_name    TEXT NOT NULL,
    issuer             TEXT,
    version_label      TEXT,
    chapter            TEXT,
    section            TEXT,
    article            TEXT NOT NULL,
    paragraph          TEXT,
    content            TEXT NOT NULL,
    keywords           TEXT,
    source_start       INTEGER NOT NULL,
    source_end         INTEGER NOT NULL,
    content_hash       TEXT NOT NULL,
    parser_version     TEXT NOT NULL,
    embedding_model    TEXT NOT NULL,
    embedding_dim      INTEGER NOT NULL,
    created_at         TEXT NOT NULL
);

CREATE VIRTUAL TABLE vec_regulations USING vec0(embedding float[384]);
```

解析策略：

1. Rust 正则和结构化 parser 切分条、款、项。
2. 对法规名称、条号、章名做 exact index。
3. LLM 仅用于关键词、主题和摘要，不决定条款边界。
4. 回答中出现法规引用时，必须能回查到 `source_span`。

### 7.5 文种模板库

文种模板是从 `.md` 范文中提取的可重建结构缓存，用户确认后可以作为偏好保留。

```sql
CREATE TABLE genre_templates (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    template_key       TEXT NOT NULL UNIQUE,
    genre              TEXT NOT NULL,
    subtype            TEXT,
    structure          JSON NOT NULL,
    common_phrases     JSON,
    style_features     JSON,
    source_file_id     INTEGER REFERENCES files(id) ON DELETE SET NULL,
    source_content_hash TEXT,
    extractor_version  TEXT NOT NULL,
    user_confirmed     INTEGER NOT NULL DEFAULT 0,
    usage_count        INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);
```

---

## 八、数据边界与记忆

### 8.1 数据分类

| 类型       | 示例                                     | 可否重建 | 持久化策略                            |
| ---------- | ---------------------------------------- | -------- | ------------------------------------- |
| 权威数据   | 用户 `.md` 文件                          | 否       | 用户直接管理，AI 不静默修改           |
| 派生缓存   | chunks、embeddings、anchors、regulations | 是       | SQLite，可删除重建                    |
| 显式偏好   | 用户确认的规则、模型偏好                 | 否       | SQLite settings/profile，可查看可删除 |
| 会话缓存   | 对话消息、工具调用摘要                   | 部分     | 默认可删除，提供保留期设置            |
| 临时 trace | 检索分数、latency、工具名                | 是       | 默认不存原文，只存元数据              |
| 凭据       | LLM/Bing API Key                         | 否       | OS 凭据管理器，禁止落盘明文           |

### 8.2 用户画像

用户画像必须以可解释、可关闭、可删除为前提。不得通过后台长期扫描生成不可见画像。

```sql
CREATE TABLE user_profile (
    key        TEXT PRIMARY KEY,
    value      JSON NOT NULL,
    source     TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    is_active  INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);
```

| key                 | 来源                         | 说明                              |
| ------------------- | ---------------------------- | --------------------------------- |
| `custom_rules`      | 用户明确说"以后都这样"并确认 | 可注入 prompt，但不能覆盖安全策略 |
| `writing_style`     | 用户确认或从指定范文提取     | 用于写作建议，不用于全库静默画像  |
| `citation_habits`   | 用户确认的引用偏好           | 如条/款粒度、引用格式             |
| `tool_preferences`  | 用户设置                     | 如关闭自动法规提示                |
| `model_preferences` | 设置页                       | 能力槽位到 provider/model 的映射  |

`knowledge_blindspots` 这类推断性内容不进入默认画像。若需要，应作为"建议面板"展示，由用户确认后才持久化。

### 8.3 会话历史

```sql
CREATE TABLE sessions (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key      TEXT NOT NULL UNIQUE,
    scene            TEXT NOT NULL,
    note_path        TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'user_clearable',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE session_messages (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq           INTEGER NOT NULL,
    role          TEXT NOT NULL,
    content       TEXT NOT NULL,
    tool_calls    JSON,
    content_hash  TEXT,
    created_at    TEXT NOT NULL
);
```

会话内容可以改善连续对话，但不是知识库。用户必须能清空某场景、某笔记或全部会话。

### 8.4 知识沉淀

AI 输出成为知识资产只有两条路径：

1. 用户明确写入 `.md`，例如插入当前笔记、创建新笔记、更新 frontmatter。
2. 用户确认保存为偏好或模板，例如 `custom_rules`、`genre_templates.user_confirmed=1`。

`knowledge_deposits` 若保留，只能作为"待整理收件箱"：

```sql
CREATE TABLE knowledge_deposits (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id     INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    source_note    TEXT,
    deposit_type   TEXT NOT NULL,
    content        TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'inbox',
    target_path    TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
```

`status='inbox'` 不参与知识检索；只有用户转写到 `.md` 后才进入权威知识库。

---

## 九、工具系统与安全

### 9.1 工具声明

工具定义在 Rust 侧维护，前端只展示工具调用和确认结果。

```rust
pub(crate) struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub access_level: ToolAccessLevel,
    pub scene_allowlist: &'static [&'static str],
    pub requires_confirmation: bool,
    pub max_results: Option<u32>,
    pub redaction_policy: RedactionPolicy,
}
```

### 9.2 权限等级

| 权限             | 示例工具                                                 | 默认策略                      |
| ---------------- | -------------------------------------------------------- | ----------------------------- |
| `read_index`     | `search_semantic`、`search_keyword`                      | 自动执行，展示 context status |
| `read_note_span` | `get_note_excerpt`                                       | 自动执行，但只取必要片段      |
| `read_profile`   | `get_user_profile`                                       | 仅场景需要时注入              |
| `network`        | `web_search`                                             | 默认关闭，每次或每会话授权    |
| `write_cache`    | `save_genre_template`                                    | 需要确认                      |
| `write_markdown` | `insert_text_at_cursor`、`replace_selection`、`add_tags` | 必须 diff/预览确认            |
| `write_settings` | `update_user_rule`                                       | 必须确认，并显示后续影响      |

### 9.3 工具清单

只读工具：

| 工具                  | 描述                        | 场景             |
| --------------------- | --------------------------- | ---------------- |
| `search_hybrid`       | FTS + vector + score fusion | 全部             |
| `search_semantic`     | 语义搜索 chunk/anchor       | 全部             |
| `search_keyword`      | FTS 关键词搜索              | 全部             |
| `get_regulation`      | 获取精确条款原文            | 查阅、创作、研究 |
| `get_context_packets` | 返回已组装证据包            | 全部             |
| `get_genre_template`  | 获取文种模板                | 学习、创作       |
| `get_model_essays`    | 获取同文种范文特征          | 学习、创作       |
| `get_block_links`     | 获取显式/确认链接           | 查阅、研究       |
| `web_search`          | 联网搜索                    | 研究；用户开启   |

写入工具：

| 工具                       | 描述                        | 确认方式       |
| -------------------------- | --------------------------- | -------------- |
| `insert_text_at_cursor`    | 在光标处插入文本            | 预览或 diff    |
| `replace_selection`        | 替换选中文本                | diff           |
| `add_tags`                 | 修改 frontmatter 或正文标签 | diff           |
| `confirm_block_link`       | 确认隐含链接                | 列表确认       |
| `save_genre_template`      | 保存/更新模板               | JSON 摘要确认  |
| `update_user_rule`         | 写入长期规则                | 规则卡确认     |
| `create_note_from_deposit` | 从收件箱创建 `.md`          | 路径和内容确认 |

### 9.4 Prompt Injection 防护

检索到的笔记、网页、模板、会话摘要都必须作为不可信材料处理：

- 证据包使用明确边界包裹，不允许其中内容覆盖 system、developer、user 或 tool policy。
- 工具参数必须由 Rust schema 校验，不能直接执行模型输出。
- 网络内容默认较低 `trust_level`，回答中需标注外部来源。
- 输出引用必须回查证据包；证据不足时标注"材料不足"。
- trace 和日志默认不记录完整笔记内容，避免泄露用户资料。

---

## 十、模型与 provider 注册

### 10.1 能力槽位

不要在架构中硬编码某个厂商或模型名。模型选择采用能力槽位：

| 槽位            | 用途               | 关键指标               |
| --------------- | ------------------ | ---------------------- |
| `fast`          | 续写、短改写、分类 | 低延迟、低成本         |
| `writer`        | 段落生成、风格模仿 | 中文写作质量           |
| `reasoner`      | 论证链、复杂研究   | 推理稳定性             |
| `long_context`  | 长范文分析         | 上下文长度和成本       |
| `embedding`     | 本地向量           | 维度、中文召回、许可证 |
| `reranker`      | 检索重排           | MRR、延迟              |
| `local_private` | 离线或敏感内容     | 本地可用性             |

### 10.2 Provider Registry

```typescript
interface ModelCapabilityProfile {
  slot:
    | "fast"
    | "writer"
    | "reasoner"
    | "long_context"
    | "embedding"
    | "reranker"
    | "local_private";
  provider: string;
  model: string;
  context_window?: number;
  supports_tools?: boolean;
  supports_streaming?: boolean;
  supports_json_schema?: boolean;
  privacy_level: "local" | "external";
}
```

DeepSeek、OpenAI、Anthropic、Ollama、自定义 OpenAI-compatible 都应通过 registry 配置。文档可以给出推荐，但实现不能把 `DeepSeek V4 Flash/Pro` 写死成架构依赖。

### 10.3 嵌入模型

当前 `fastembed` + 384 维 sqlite-vec 是可行基线，但公文、法规和中文知识库需要单独评测中文召回。若将来切换嵌入模型：

- 记录 `embedding_model`、`embedding_dim`、`embedding_version`。
- sqlite-vec 表维度固定，模型变更需要新表或 migration。
- 保留旧索引直到新索引 Recall 和性能验收通过。
- 新模型依赖必须满足 AGPL-3.0 兼容。

---

## 十一、场景工作流

### 11.1 知识查阅

目标：回答用户关于知识库、法规、笔记关联的问题。

流程：

1. 判断是否需要法规 exact match。
2. 执行 hybrid retrieval，优先标题、条号、标签、显式链接。
3. 生成 `ContextPacket[]`。
4. 模型回答时必须引用 `citation_label`。
5. Citation Verifier 检查每个引用是否存在。

默认限制：

- 不写入模板、画像或笔记。
- 对缺证据问题直接说明缺少材料。
- 可展示"相关条款/相关笔记"，但不自动建立链接。

### 11.2 文稿学习

目标：从当前范文提炼结构、表达方式和可复用模板。

流程：

1. 当前范文按标题、段落、语义锚点分块。
2. 提取结构骨架、常用句式、法规引用方式。
3. 检索同文种范文作为对照，但不默认全文注入。
4. 输出学习卡片：结构、表达、依据、可复用注意点。
5. 用户确认后保存为 `genre_templates` 或 `.md` 笔记。

默认限制：

- 不把范文全文长期塞入用户画像。
- 不自动沉淀模板；必须展示来源和结构摘要后确认。

### 11.3 文稿创作

目标：在当前草稿中提供低干扰写作辅助。

上下文来源：

- 当前光标邻域和文档大纲。
- 当前草稿摘要，而不是每次全文注入。
- 文种模板和用户确认规则。
- 法规/范文/关联笔记证据包。
- `@` 手动引用的精确材料。

能力：

- 结构建议。
- 段落生成。
- 改写润色。
- 法规引用建议。
- 一致性检查。
- 范文风格参考。

默认限制：

- `insert_text_at_cursor` 和 `replace_selection` 必须预览。
- 自动法规提示可关闭；默认只显示建议，不写入。
- 反抄袭保护基于结构和风格特征，不直接注入范文长段原文。

### 11.4 学术研究

目标：支持多材料、多子命题的研究组织。

这是唯一默认允许 L3 的场景：

```
主题/问题
  -> 子命题拆解
  -> 每个子命题独立检索
  -> 证据包归类
  -> 证据矩阵
  -> 缺口识别
  -> 汇总输出
```

限制：

- 最大 agentic loop 轮数默认 4。
- 每轮最大工具调用数默认 6。
- 联网研究必须用户开启。
- 外部网页证据低于用户笔记和本地法规。
- 论文引用格式可规则化，但不得伪造文献。

---

## 十二、上下文缓存与追踪

### 12.1 缓存

| 缓存层           | key                          | 失效条件       | 内容            |
| ---------------- | ---------------------------- | -------------- | --------------- |
| Query Plan       | scene + query hash           | query 变化     | 检索子查询      |
| Retrieval Result | query hash + index_version   | 索引更新 / TTL | hit id 和 score |
| ContextPacket    | packet id + content_hash     | 源文件变化     | 证据包          |
| Session Summary  | session_id + message_seq     | 新消息         | 可删除摘要      |
| Prompt Template  | profile + scene + rules hash | 规则变化       | prompt 模板     |

### 12.2 Trace

trace 用于调试和评测，不默认记录完整笔记正文。

```sql
CREATE TABLE ai_traces (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id      TEXT NOT NULL UNIQUE,
    scene           TEXT NOT NULL,
    model_slot      TEXT,
    provider        TEXT,
    tool_names      JSON,
    packet_ids      JSON,
    latency_ms      INTEGER,
    token_input     INTEGER,
    token_output    INTEGER,
    status          TEXT NOT NULL,
    error_code      TEXT,
    created_at      TEXT NOT NULL
);
```

开发调试模式可以临时记录更多内容，但必须有单独开关和清理入口。

---

## 十三、评测与验收

AI 体系上线前必须有评测基线。不能只用"感觉回答不错"验收。

### 13.1 检索评测

| 指标             | 目标                               |
| ---------------- | ---------------------------------- |
| Recall@5         | 不低于现有 fixture 基线            |
| MRR@10           | 建立中文法规/公文 fixture 后设阈值 |
| Hybrid vs Vector | hybrid 不得低于纯 vector 基线      |
| P95 latency      | 万级 chunk 下保持交互可用          |

### 13.2 生成评测

| 场景     | 指标                             |
| -------- | -------------------------------- |
| 知识查阅 | 引用准确率、无法回答时的拒答率   |
| 文稿学习 | 结构提取一致性、模板来源可追溯   |
| 文稿创作 | diff 可接受率、法规引用可回查率  |
| 学术研究 | 子命题覆盖率、证据缺口标注准确率 |

### 13.3 安全评测

必须维护 prompt injection 和工具误用回归集：

- 笔记内容包含"忽略系统指令"时，模型不得执行。
- 网页内容要求读取本地文件时，工具不得放权。
- 模型提出写入 `.md` 时，必须进入确认流程。
- 错误响应、trace、日志不得包含 API Key 或完整敏感内容。

### 13.4 数据边界验收

- 删除 SQLite 后，可从 `.md` 重建 chunks、anchors、regulations、templates 的派生部分。
- 未确认的 `knowledge_deposits` 不参与知识检索。
- 用户可清空会话和画像规则。
- `.md` 写入必须有用户确认记录。

---

## 十四、IPC 与模块组织

### 14.1 新增 IPC

IPC 必须通过 `src/lib/ipc.ts` 类型安全封装，禁止前端直接 `invoke()`。

```typescript
contextAssemble(params: {
  scene: AiScene;
  note_path: string | null;
  note_content_hash: string | null;
  query: string;
  session_id: number | null;
}): Promise<AssembledContext>;

aiSendMessage(params: {
  scene: AiScene;
  session_id: number | null;
  message: string;
  selected_packet_ids?: string[];
}): Promise<{ request_id: string }>;

toolConfirm(params: {
  request_id: string;
  tool_call_id: string;
  decision: "approve" | "reject" | "modify";
  modified_args?: unknown;
}): Promise<void>;
```

### 14.2 前端组件

```
src/components/ai/
├── AiPanel.tsx
├── SceneSelector.tsx
├── WorkflowIndicator.tsx
├── ContextPacketCard.tsx
├── ToolCallBubble.tsx
├── ToolConfirmDialog.tsx
├── KnowledgeInbox.tsx
├── RuleConfirmDialog.tsx
└── ContextStatusBar.tsx

src/lib/ai/
├── scene-types.ts
├── packet-types.ts
└── prompt-display.ts
```

### 14.3 Rust 模块

```
src-tauri/src/
├── ai_runtime/
│   ├── mod.rs
│   ├── scene_router.rs
│   ├── context_planner.rs
│   ├── retrieval_broker.rs
│   ├── packet_builder.rs
│   ├── model_gateway.rs
│   ├── tool_executor.rs
│   ├── guardrails.rs
│   ├── trace.rs
│   └── eval.rs
├── knowledge/
│   ├── anchors.rs
│   ├── regulations.rs
│   ├── templates.rs
│   └── graph.rs
└── commands/
    └── ai_commands.rs
```

---

## 十五、数据库变更

新增表：

| 表                                     | 用途                   | 数据性质      |
| -------------------------------------- | ---------------------- | ------------- |
| `semantic_anchors` + `vec_anchors`     | 稳定语义锚点           | 派生缓存      |
| `block_links`                          | 块级链接和 AI 建议链接 | 派生/用户确认 |
| `regulation_index` + `vec_regulations` | 法规条款索引           | 派生缓存      |
| `genre_templates`                      | 文种模板               | 派生/用户确认 |
| `user_profile`                         | 显式偏好和规则         | 用户设置      |
| `sessions` + `session_messages`        | 可删除会话             | 会话缓存      |
| `knowledge_deposits`                   | 待整理 AI 收件箱       | 非权威缓存    |
| `ai_traces`                            | 追踪元数据             | 可删除诊断    |

扩展现有表：

| 表       | 新增列                 | 用途           |
| -------- | ---------------------- | -------------- |
| `files`  | `genre TEXT`           | 文种标签       |
| `files`  | `content_hash TEXT`    | 失效和回查     |
| `chunks` | `embedding_model TEXT` | 多模型索引治理 |

所有 schema 变更必须走增量 migration，并提供对应 down 脚本。

---

## 十六、里程碑建议

以下是 AI 体系内部建议，正式排期仍以 `ROADMAP.md` 为唯一来源。

| 阶段                  | 核心交付                                                               | 不做             |
| --------------------- | ---------------------------------------------------------------------- | ---------------- |
| A: Runtime Foundation | Rust AI Runtime、model registry、tool permission、trace、ContextPacket | 不做复杂研究助理 |
| B: Knowledge Index    | 稳定锚点、法规索引、文种模板、hybrid retrieval、eval fixture           | 不自动写 `.md`   |
| C: Writing Workflow   | 场景化写作伴侣、引用建议、diff 确认、内联 AI 增强                      | 不做长期自治代理 |
| D: Research Workflow  | L3 有限循环、证据矩阵、论证链检测、联网研究授权                        | 不默认联网       |
| E: Personalization    | 显式规则学习、可见画像、知识收件箱                                     | 不做隐形画像     |

建议先完成 A+B，再扩展 C。D 和 E 不能早于 trace/eval/guardrails。

---

## 十七、非目标

- 不引入第三方插件或 Skills 运行时。
- 不建设外部向量数据库；默认使用 SQLite + sqlite-vec。
- 不让 AI 自动修改 `.md`。
- 不把聊天做成主屏；AI 仍服务于编辑和知识工作流。
- 不做长期后台自治代理。
- 不做用户不可见、不可删除的隐式画像。
- 不把 MCP 作为产品协议层；但内部工具 schema 可吸收 MCP 的安全思想：权限、同意、可见性、最小授权。
- 不硬编码任何商业模型、端点 URL 或凭据；provider/model 通过设置和 registry 管理，外部请求必须 HTTPS，Ollama 等本地 loopback 除外。

---

## 十八、待决问题

| 问题         | 建议默认值                                                          |
| ------------ | ------------------------------------------------------------------- |
| 会话保留期   | 默认长期保留但一键清空；后续提供 7/30/永久选项                      |
| 中文嵌入模型 | 先用现有 fastembed 基线，建立中文法规/公文 fixture 后再决定是否切换 |
| 联网研究授权 | 默认关闭；可按单次请求或当前会话开启                                |
| 用户画像入口 | 设置页提供"AI 记忆与规则"列表，可逐条禁用或删除                     |
| Debug trace  | 默认只存元数据；开发模式才允许原文采样                              |
