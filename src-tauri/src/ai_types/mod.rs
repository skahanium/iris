//! Shared types for the Iris AI subsystem.
//!
//! This module owns all cross-cutting data types that are referenced by both
//! `ai_runtime` (business logic) and `llm` (infrastructure). Extracting them
//! here breaks the circular dependency that previously existed between those
//! two modules.
//!
//! `ai_runtime::mod` re-exports everything via `pub use crate::ai_types::*;`
//! so that existing call-sites remain unchanged.

use serde::{Deserialize, Serialize};

// 鈹€鈹€鈹€ Scene 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// AI 浣跨敤鍦烘櫙锛屽搴斿墠绔満鏅€夋嫨鍣ㄣ€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiScene {
    /// 鐭ヨ瘑鏌ラ槄 鈥?娉曡鏉℃銆佺瑪璁板叧鑱?    KnowledgeLookup,
    /// 鏂囩瀛︿範 鈥?鑼冩枃缁撴瀯銆佽〃杈剧壒寰?    ExemplarLearning,
    /// 鏂囩鍒涗綔 鈥?鍐欎綔杈呭姪
    DraftingAssist,
    /// 瀛︽湳鐮旂┒ 鈥?澶氭潗鏂欎氦鍙夎璇?    ResearchSynthesis,
}

impl AiScene {
    /// 鍦烘櫙瀵瑰簲鐨勯粯璁よ嚜娌荤瓑绾с€?    pub fn autonomy_level(&self) -> AutonomyLevel {
        match self {
            AiScene::KnowledgeLookup => AutonomyLevel::L1,
            AiScene::ExemplarLearning => AutonomyLevel::L1,
            AiScene::DraftingAssist => AutonomyLevel::L2,
            AiScene::ResearchSynthesis => AutonomyLevel::L3,
        }
    }

    /// 鍦烘櫙鐨?runtime profile 鏍囪瘑銆?    pub fn profile(&self) -> &'static str {
        match self {
            AiScene::KnowledgeLookup => "knowledge_lookup",
            AiScene::ExemplarLearning => "exemplar_learning",
            AiScene::DraftingAssist => "drafting_assist",
            AiScene::ResearchSynthesis => "research_synthesis",
        }
    }

    /// 鍦烘櫙榛樿鐨勪細璇濊寖鍥存槸鍚︿负搴撶骇锛堜笉缁戝畾绗旇锛夈€?    pub fn default_global_scope(&self) -> bool {
        matches!(self, AiScene::KnowledgeLookup | AiScene::ResearchSynthesis)
    }
}

// 鈹€鈹€鈹€ Autonomy Level 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 宸ュ叿鑷不绛夌骇銆傜瓑绾ц秺楂橈紝Agent 鑷富鍐崇瓥绌洪棿瓒婂ぇ銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// L0: 绾鍒?鏈湴妫€绱紝鏃?LLM 鍐崇瓥
    L0 = 0,
    /// L1: 鍗曡疆 LLM + 鍙楁帶涓婁笅鏂囷紝鏃犲伐鍏峰惊鐜?    L1 = 1,
    /// L2: 宸ヤ綔娴佷腑鍏佽灏戦噺宸ュ叿璋冪敤
    L2 = 2,
    /// L3: 鏈夐檺 agentic loop锛岄檺鍒舵渶澶ц疆鏁板拰宸ュ叿娆℃暟
    L3 = 3,
}

// 鈹€鈹€鈹€ Scene Profile 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// Scene profile: describes what capabilities a scene activates.
#[derive(Debug, Clone)]
pub struct SceneProfile {
    pub scene: AiScene,
    pub autonomy_level: AutonomyLevel,
    pub default_global_scope: bool,
    pub max_agentic_rounds: u32,
    pub max_tool_calls_per_round: u32,
    pub default_token_budget: usize,
    pub max_token_budget: usize,
}

/// Resolve a scene to its profile.
pub fn resolve_scene(scene: AiScene) -> SceneProfile {
    match scene {
        AiScene::KnowledgeLookup => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L2,
            default_global_scope: true,
            max_agentic_rounds: 3,
            max_tool_calls_per_round: 4,
            default_token_budget: 30_000,
            max_token_budget: 80_000,
        },
        AiScene::ExemplarLearning => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L2,
            default_global_scope: false,
            max_agentic_rounds: 2,
            max_tool_calls_per_round: 4,
            default_token_budget: 100_000,
            max_token_budget: 240_000,
        },
        AiScene::DraftingAssist => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L2,
            default_global_scope: false,
            max_agentic_rounds: 3,
            max_tool_calls_per_round: 5,
            default_token_budget: 60_000,
            default_token_budget: 120_000,
            max_token_budget: 320_000,
        AiScene::ResearchSynthesis => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L3,
            default_global_scope: true,
            max_agentic_rounds: 4,
            max_tool_calls_per_round: 6,
            default_token_budget: 100_000,
            default_token_budget: 160_000,
            max_token_budget: 320_000,
    }
}

/// Select appropriate capability slot for scene.
pub fn slot_for_scene(scene: AiScene) -> CapabilitySlot {
    match scene {
        AiScene::KnowledgeLookup => CapabilitySlot::Fast,
        AiScene::ExemplarLearning => CapabilitySlot::Writer,
            default_token_budget: 200_000,
            max_token_budget: 480_000,
    }
}

// 鈹€鈹€鈹€ Web evidence metadata (spec 搂4.1) 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 缃戦〉妫€绱㈠悗绔爣璇嗐€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchBackend {
    Minimax,
    Duckduckgo,
}

/// 缃戦〉鏉ユ簮鍙俊绛夌骇銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSourceRank {
    Official,
    Academic,
    Media,
    Community,
    Unknown,
}

/// 缃戦〉璇佹嵁鎵╁睍鍏冩暟鎹紙浠?`source_type = web` 鏃跺～鍏咃級銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebEvidenceMeta {
    pub url: Option<String>,
    pub domain: Option<String>,
    pub published_at: Option<String>,
    pub fetched_at: String,
    pub search_backend: WebSearchBackend,
    pub source_rank: WebSourceRank,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_from: Option<WebSearchBackend>,
}

// 鈹€鈹€鈹€ ContextPacket 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 璇佹嵁鍖?鈥?缁撴瀯鍖栫殑妫€绱㈢粨鏋滐紝鏄?AI 浣撶郴鐨勬牳蹇冩暟鎹粨鏋勩€?///
/// `ContextPacket` 鐢ㄤ簬锛?/// - 涓?LLM 鎻愪緵鍙拷婧殑璇佹嵁鏉ユ簮
/// - 鏀寔寮曠敤楠岃瘉鍜屼簨瀹炴牳鏌?/// - 瀹炵幇璇佹嵁閾惧彲瑙嗗寲
///
/// 妫€绱㈢粨鏋滃繀椤诲厛鍙樻垚 `ContextPacket`锛屽啀杩涘叆 prompt銆?/// 鍚勬绱㈠眰锛團TS / Vector / Graph / Exact / Template锛夊潎杈撳嚭姝ょ被鍨嬶紝
/// 鐢?`retrieval_broker::fuse_and_rank` 缁熶竴璇勫垎铻嶅悎銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPacket {
    pub id: String,
    pub source_type: SourceType,
    pub source_path: Option<String>,
    pub title: String,
    pub heading_path: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub content_hash: String,
    pub excerpt: String,
    pub retrieval_reason: String,
    pub score: f64,
    pub trust_level: TrustLevel,
    pub citation_label: String,
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<WebEvidenceMeta>,
}

/// 璇佹嵁鍖呯殑鏁版嵁鏉ユ簮绫诲瀷銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Note,
    Anchor,
    Regulation,
    Template,
    Session,
    Web,
}

/// 婧愭枃浠朵腑鐨勫瓧绗﹀亸绉昏寖鍥淬€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// 璇佹嵁淇′换绛夌骇锛屾寜鍙俊搴︿粠楂樺埌浣庢帓鍒椼€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    UserNote,
    DerivedCache,
    ExternalWeb,
    ModelGenerated,
}

// 鈹€鈹€鈹€ Context Status 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 涓婁笅鏂囩姸鎬佹憳瑕侊紝鐢ㄤ簬鍓嶇鏄剧ず鍜岃皟璇曘€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStatus {
    pub regulations_loaded: usize,
    pub model_essays_loaded: usize,
    pub anchors_loaded: usize,
    pub links_loaded: usize,
    pub total_tokens_estimate: usize,
}

// 鈹€鈹€鈹€ Tool Access Level 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 宸ュ叿璁块棶鏉冮檺绛夌骇锛屽喅瀹氬伐鍏峰彲鎵ц鐨勬搷浣滆寖鍥淬€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccessLevel {
    ReadIndex,
    ReadNoteSpan,
    ReadProfile,
    Network,
    WriteCache,
    WriteMarkdown,
    WriteSettings,
    /// Install / uninstall / toggle agent skills.
    ManageSkills,
}

// 鈹€鈹€鈹€ Tool Spec 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 宸ュ叿瑙勬牸瀹氫箟锛屾弿杩颁竴涓彲渚?LLM 璋冪敤鐨勫伐鍏枫€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub access_level: ToolAccessLevel,
    pub scene_allowlist: Vec<AiScene>,
    pub requires_confirmation: bool,
    pub max_results: Option<u32>,
    /// Scenes where this tool is naturally relevant.
    /// Empty means universally available. New field parallel to scene_allowlist;
    /// Phase 4 will remove scene_allowlist once policy engine is complete.
    #[serde(default)]
    pub scene_affinity: Vec<AiScene>,
}

// 鈹€鈹€鈹€ Request / Response types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// AI 璇锋眰锛屼粠鍓嶇鍙戣捣銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub scene: AiScene,
    pub note_path: Option<String>,
    pub note_content_hash: Option<String>,
    pub query: String,
    pub session_id: Option<i64>,
    pub selected_packet_ids: Option<Vec<String>>,
}

// 鈹€鈹€鈹€ Tool Confirmation 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 宸ュ叿璋冪敤纭璇锋眰銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfirmRequest {
    pub request_id: String,
    pub tool_call_id: String,
    pub decision: ToolConfirmDecision,
    pub modified_args: Option<serde_json::Value>,
}

/// 鐢ㄦ埛瀵瑰伐鍏疯皟鐢ㄧ殑纭鍐崇瓥銆?#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolConfirmDecision {
    Approve,
    Reject,
    Modify,
}

// 鈹€鈹€鈹€ Tool Call Result 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 宸ュ叿璋冪敤缁撴灉锛堝惈鍙娴嬫€у厓鏁版嵁锛夈€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub tool_name: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub duration_ms: u64,
    pub tokens_used: Option<u32>,
    pub error: Option<String>,
}

// 鈹€鈹€鈹€ PatchProposal 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 鍙楁帶缂栬緫琛ヤ竵 鈥?AI 瀵?Markdown 鐨勬墍鏈夋鏂囧啓鍏ラ兘蹇呴』璧版缁撴瀯銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposal {
    pub id: String,
    pub target_path: String,
    pub base_content_hash: String,
    pub range: SourceSpan,
    pub original_text: String,
    pub replacement_text: String,
    pub evidence_packet_ids: Vec<String>,
    pub risk_level: RiskLevel,
    pub warnings: Vec<String>,
    pub created_at: String,
}

/// 琛ヤ竵椋庨櫓绛夌骇銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

// 鈹€鈹€鈹€ Chunked Patch Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 鍒嗗潡琛ヤ竵 鈥?澶氫釜鐩稿叧琛ヤ竵鐨勯泦鍚堛€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedPatchProposal {
    pub id: String,
    pub target_path: String,
    pub base_content_hash: String,
    pub chunks: Vec<PatchChunk>,
    pub description: String,
    pub risk_level: RiskLevel,
    pub warnings: Vec<String>,
    pub created_at: String,
}

/// 琛ヤ竵鍧椼€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchChunk {
    pub id: String,
    pub range: SourceSpan,
    pub original_text: String,
    pub replacement_text: String,
    pub chunk_type: ChunkType,
    pub order: usize,
}

/// 鍧楃被鍨嬨€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    Rewrite,
    Insert,
    Delete,
    Move,
}

/// 琛ヤ竵搴旂敤缁撴灉銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    pub success: bool,
    pub new_content_hash: Option<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

/// 琛ヤ竵楠岃瘉閿欒銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatchValidationError {
    HashMismatch {
        expected: String,
        actual: String,
    },
    RangeOutOfBounds {
        range_start: usize,
        range_end: usize,
        content_length: usize,
    },
    TextMismatch {
        expected: String,
        actual: String,
    },
    FileNotFound {
        path: String,
    },
}

impl std::fmt::Display for PatchValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchValidationError::HashMismatch { expected, actual } => {
                write!(f, "鍐呭鍝堝笇涓嶅尮閰嶏細鏈熸湜 {expected}锛屽疄闄?{actual}")
            }
            PatchValidationError::RangeOutOfBounds {
                range_start,
                range_end,
                content_length,
            } => {
                write!(
                    f,
                    "鑼冨洿瓒婄晫锛歔{range_start}, {range_end}) 瓒呭嚭鍐呭闀垮害 {content_length}"
                )
            }
            PatchValidationError::TextMismatch { expected, actual } => {
                write!(
                    f,
                    "鍘熸枃涓嶄竴鑷达細鏈熸湜 {:?}锛屽疄闄?{:?}",
                    &expected[..expected.len().min(50)],
                    &actual[..actual.len().min(50)]
                )
            }
            PatchValidationError::FileNotFound { path } => {
                write!(f, "鏂囦欢涓嶅瓨鍦細{path}")
            }
        }
    }
}

// 鈹€鈹€鈹€ Writing Workflow Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 鍐欎綔鎰忓浘銆?#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingIntent {
    Continue,
    Rewrite,
    AddEvidence,
    Outline,
    UnifyTone,
    ChapterRewrite,
    ChapterContinue,
    ChapterRestructure,
    OutlineCheck,
    CitationGapCheck,
    StyleConsistency,
    CrossDocReference,
}

/// 鍐欎綔鎰忓浘绾у埆銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingIntentLevel {
    Selection,
    Chapter,
    Document,
}

impl WritingIntent {
    /// 鑾峰彇鎰忓浘绾у埆銆?    pub fn level(&self) -> WritingIntentLevel {
        match self {
            WritingIntent::Continue
            | WritingIntent::Rewrite
            | WritingIntent::AddEvidence
            | WritingIntent::Outline
            | WritingIntent::UnifyTone => WritingIntentLevel::Selection,
            WritingIntent::ChapterRewrite
            | WritingIntent::ChapterContinue
            | WritingIntent::ChapterRestructure => WritingIntentLevel::Chapter,
            WritingIntent::OutlineCheck
            | WritingIntent::CitationGapCheck
            | WritingIntent::StyleConsistency
            | WritingIntent::CrossDocReference => WritingIntentLevel::Document,
        }
    }
}

/// 鍐欎綔寤鸿銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingSuggestion {
    pub id: String,
    pub intent: WritingIntent,
    pub explanation: String,
    pub confidence: f64,
}

/// 鍐欎綔浠诲姟杈撳叆銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingTaskInput {
    pub target_path: String,
    pub base_content_hash: String,
    pub selection: Option<String>,
    pub cursor_context: String,
    pub writing_goal: String,
    pub web_authorized: bool,
}

/// 鍐欎綔浠诲姟缁撴灉銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingTaskResult {
    pub request_id: String,
    pub suggestions: Vec<WritingSuggestion>,
    pub patches: Vec<PatchProposal>,
    pub evidence_used: Vec<ContextPacket>,
    pub total_tokens: TokenUsage,
}

// 鈹€鈹€鈹€ Citation Check Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 寮曠敤妫€鏌ヨ緭鍏ャ€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckInput {
    pub paragraph_text: String,
    pub document_path: String,
    pub scope: Option<CitationCheckScope>,
    pub web_authorized: bool,
}

/// 寮曠敤妫€鏌ヨ寖鍥淬€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckScope {
    pub paths: Vec<String>,
    pub path_prefixes: Vec<String>,
    pub corpus_ids: Option<Vec<String>>,
}

/// 寮曠敤妫€鏌ョ粨鏋溿€?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckResult {
    pub request_id: String,
    pub claims: Vec<FactClaim>,
    pub coverage: CitationCoverage,
    pub suggestions: Vec<CitationSuggestion>,
    pub evidence_used: Vec<ContextPacket>,
    pub total_tokens: TokenUsage,
}

/// 浜嬪疄澹版槑銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactClaim {
    pub id: String,
    pub statement: String,
    pub has_support: bool,
    pub supporting_evidence: Vec<String>,
    pub conflicting_evidence: Vec<String>,
}

/// 寮曠敤瑕嗙洊搴︺€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationCoverage {
    WellSupported,
    PartiallySupported,
    WeaklySupported,
    Unsupported,
    Contradicted,
}

/// 寮曠敤寤鸿鍔ㄤ綔銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationAction {
    AddCitation,
    Rewrite,
    RemoveClaim,
    AddQualifier,
}

/// 寮曠敤寤鸿銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationSuggestion {
    pub claim_id: String,
    pub action: CitationAction,
    pub suggested_citation: Option<String>,
    pub explanation: String,
}

// 鈹€鈹€鈹€ Organize Workflow Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 鏁寸悊寤鸿绫诲瀷銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeSuggestionType {
    RenameTitle,
    AddTag,
    MoveToFolder,
    AssignCorpus,
    AddBlockLink,
    ExtractTemplate,
}

/// 鏁寸悊寤鸿銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeSuggestion {
    pub id: String,
    pub suggestion_type: OrganizeSuggestionType,
    pub target_path: String,
    pub current_value: Option<String>,
    pub suggested_value: String,
    pub reason: String,
    pub source: String,
    pub confidence: f64,
    pub evidence_packet_ids: Vec<String>,
}

/// 鎵归噺鍙樻洿璁″垝銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeBatch {
    pub id: String,
    pub title: String,
    pub description: String,
    pub suggestions: Vec<OrganizeSuggestion>,
    pub created_at: String,
}

/// 鏁寸悊浠诲姟杈撳叆銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskInput {
    pub scope: Option<OrganizeTaskScope>,
    pub task_type: OrganizeTaskType,
}

/// 鏁寸悊浠诲姟鑼冨洿銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskScope {
    pub paths: Vec<String>,
    pub path_prefixes: Vec<String>,
    pub corpus_ids: Option<Vec<String>>,
}

/// 鏁寸悊浠诲姟绫诲瀷銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeTaskType {
    FullAudit,
    TitleSuggestions,
    TagSuggestions,
    FolderSuggestions,
    LinkSuggestions,
}

/// 鏁寸悊浠诲姟缁撴灉銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskResult {
    pub request_id: String,
    pub batch: OrganizeBatch,
    pub total_tokens: TokenUsage,
}

// 鈹€鈹€鈹€ Research Workflow State 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 鐮旂┒浠诲姟鐘舵€併€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchTaskState {
    Idle,
    Planning,
    Retrieving,
    Analyzing,
    Completed,
    Paused,
    Failed,
    Aborted,
}

/// 鐮旂┒浠诲姟閫愯疆杩涘害銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProgress {
    pub request_id: String,
    pub topic: String,
    pub state: ResearchTaskState,
    pub current_round: u32,
    pub max_rounds: u32,
    pub queries_executed: Vec<String>,
    pub new_evidence_count: usize,
    pub total_evidence_count: usize,
    pub tokens_used: u32,
    pub token_budget: usize,
    pub progress_pct: f64,
    pub round_terminated_early: bool,
}

/// 鐮旂┒绗旇鐢熸垚璇锋眰銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNoteRequest {
    pub topic: String,
    pub summary: String,
    pub evidence_count: usize,
    pub coverage_score: f64,
    pub target_path: Option<String>,
}

/// 鐮旂┒绗旇鐢熸垚缁撴灉銆?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNoteResult {
    pub content: String,
    pub suggested_path: String,
    pub section_count: usize,
}

// 鈹€鈹€鈹€ Gateway Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub prompt_cache_hit_tokens: u32,
    #[serde(default)]
    pub prompt_cache_miss_tokens: u32,
}

/// 鑳藉姏妲戒綅锛岀敤浜?provider/model 閫夋嫨銆?#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySlot {
    Fast,
    Writer,
    Reasoner,
    LongContext,
    Embedding,
    Reranker,
    LocalPrivate,
}

/// LLM provider configuration (from settings or registry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub slot: CapabilitySlot,
}

/// LLM 瀵硅瘽娑堟伅瑙掕壊銆?#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// DeepSeek / thinking-mode chain-of-thought; must be echoed on tool-call turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

impl Default for LlmMessage {
    fn default() -> Self {
        Self {
            role: MessageRole::User,
            content: String::new(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }
}

/// Tool call from LLM (OpenAI / DeepSeek chat completions format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_call_type")]
    pub call_type: String,
    pub function: FunctionCall,
}

fn default_tool_call_type() -> String {
    "function".into()
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            call_type: default_tool_call_type(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }
}

/// Function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// 鈹€鈹€鈹€ LLM Config Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// 涓婁笅鏂囩粍瑁呯瓥鐣ャ€?#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    Hybrid,
    LongContext,
}

// 鈹€鈹€鈹€ Testability Seams (trait abstractions) 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// Abstraction over an LLM provider for testability.
///
/// Production code uses the concrete implementation backed by `reqwest`;
/// tests can inject a mock that records calls and returns canned responses.
#[allow(async_fn_in_trait)]
pub trait LlmBackend: Send + Sync {
    /// Send a non-streaming chat completion request.
    async fn chat(
        &self,
        provider: &ProviderConfig,
        messages: &[LlmMessage],
        tools: &[serde_json::Value],
        max_tokens: Option<u32>,
        temperature: Option<f64>,
    ) -> Result<LlmBackendResponse, String>;
}

/// Simplified response from [`LlmBackend::chat`].
#[derive(Debug, Clone)]
pub struct LlmBackendResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: String,
}

/// Abstraction over a text embedding engine for testability.
///
/// Production code loads the fastembed model; tests can inject a
/// deterministic stub that returns fixed vectors.
pub trait EmbedBackend: Send + Sync {
    /// Embed a single text into a vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;

    /// Batch-embed multiple texts (default: sequential calls to [`embed`]).
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}
