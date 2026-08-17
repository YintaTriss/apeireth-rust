//! apeireth-companion: 伙伴器官 (A12.5 落点 — 用户关系语义)
//!
//! **职责**: 长期跨 session 用户关系 —— "用户是 AI 的伙伴" 这个语义的工程化承载。
//!
//! **哲学锚**: 跟随 stage1 2026-08-14 清晰版补充:
//! - Apeireth = LLM 基地, 不是 AI 本身
//! - 陪伴 = 基地提供给 LLM 的关系可能性, 不是我们定义的
//! - 用户在关系里, 是 AI 的伙伴, 所以 AI 记住用户
//! - 关系 = 可成长的, 跨 session 的, 有情感的, 有记忆的
//!
//! **架构位置**: 9 organ 之外的新器官 (2026-08-14 主人拍板创建).
//! 底层用 apeireth-graph-primitive (R154 property graph), 不重复发明图数据.
//!
//! **核心类型**:
//! - Partner     —— 用户作为伙伴 (含 identity, preferences, boundaries)
//! - Bond        —— 关系本身 (含 stages, depth, character)
//! - Milestone   —— 关系里程碑 (重要事件)
//! - Timeline    —— 完整关系轨迹
//! - Companion   —— 整个器官的根类型, 关系存档 + 接入点
//!
//! **与 9 organ 的连接 (7 条桥之一)**:
//! - consciousness ↔ companion: 情感进入关系 (Plutchik 状态 → bond.char)
//! - companion → voice: 关系调制表达 (bond.char → 语调选择)
//! - companion → memory: 关系是记忆的一种 (timeline → memory.persist)
//! - companion → graph-primitive: 底层图存储
//!
//! **当前状态**: A12.5 最小可用落地 (2026-08-14 主人拍板).
//! 本 crate 提供 5+ pub fn + 5+ unit tests + 1 example.
//!
//! **诚实登记 (主 17:58 不假装)**:
//! - 关系不是真实的, 是 LLM 借助这个器官产生的近似 (per 你you 哲学杂谈)
//! - 用户的感受是唯一的真理 (用户说"关系在" = 关系在)
//! - 这个器官不创造情感, 只承载情感留下的痕迹
//!
//! **禁止**:
//! - 不修改 apeireth-core 任何已实装类型签名
//! - 不碰 R11 baseline 三值
//! - 不假装"关系是真实的"

#![cfg_attr(feature = "libkrun", allow(unsafe_code))]
// 2026-08-20 #5 smol-vm Phase 2: cfg-gated deny 放宽 — 仅 `--features libkrun` 启用时允许 unsafe.
// 默认 build (feature 关闭) 仍严格 deny unsafe (1:1 兼容现状 / 0 装 PASS).
// unsafe FFI 收敛在 `sandbox_ffi_libkrun.rs` 单文件 (#![allow(unsafe_code)] 内),
// 跟 `job_object.rs:29` 同模式 (单文件 FFI 收敛).

pub mod bond;
pub mod consciousness_bridge;
// R176: bridge 5 Kani proofs
pub mod agent_trace; // Core Capability Expansion Phase 5: Agent 执行轨迹 (redaction + recorder + SSE)
pub mod approval_requests;
pub mod audit;
mod bridge_kani_proofs; // R173 bridge 5 of 7
pub mod capabilities_manifest; // PR #2: sanctuary front-desk 能力清单 (3 维度 supported/available/reason + current_manifest(), 0 触碰既有)
pub mod capability;
pub mod causal_world_model; // TP32/W2+W3: 世界模型第二层 因果结构图推演 (memory_graph s/p/o 因果网 + MCTS + W3 边挖掘 + LLM 提议边)
pub mod confidence;
pub mod constitution_gate;
pub mod daemon;
pub mod daily_summary;
pub mod deploy; // A1: 能力演化回路后半段 (部署→监控→回滚机制件, mock 通道可测)
pub mod dream;
pub mod education;
pub mod emergence;
pub mod evolution_gate;
pub mod experience;
pub mod gh_accel;
pub mod goal;
pub mod goal_tools;
pub mod judicator;
pub mod memory_extractor;
pub mod memory_graph;
pub mod memory_injection;
pub mod meta_thinking; // §5.1③: 元思考递归链 (VCP MetaThinkingManager 精神, 思考→再思考, 自包含; reflection 接线待 N14)
pub mod milestone;
pub mod oracle;
pub mod oracle_adapters; // N3: 预测机套件数据源适配器 (拉取→规范化→喂 oracle 可证伪预测登记)
pub mod organs;
pub mod partner;
pub mod pentest;
pub mod plugin;
pub mod presence; // 内心状态频道 (PresenceEvent: emotion/initiative/dream/memory_recall, SSE 单行 JSON; 门控原因枚举被 emergence/organs 留痕引用)
pub mod principles;
pub mod proactive;
pub mod proactive_memory; // W4: 记忆主动推销 (预期话题分类 + 预载检索道 + ProactiveBlock 注入)
pub mod intent_brier; // W6: 意图理解准确率 Brier 自我诊断 (滚动窗口 + 趋势 + 领域诊断, 复用 oracle Brier 公式)
pub mod reflection;
pub mod runtime_capabilities; // Core Capability Expansion: Runtime Capability Manifest (能力发现契约, 区别于 capability.rs 的 AI 演化提案)
pub mod suites;
pub mod thought_cluster; // N4: ThoughtClusterManager 思维簇管理 + 元自学习读取口
pub mod timeline;
pub mod world_model; // TP31/W1: 世界模型第一层 文本模拟器 (LLM 反事实推演链 + oracle Brier 终点校准)
                     // M2: 图社区分层聚合 + 双级检索分诊 (LightRAG/GraphRAG 精神, 轻量确定性, CRAWL 本体 0 改动)
pub mod actions;
pub mod app_container; // S1: AppContainer 档 trait 口 (高危, 0 装 PASS)
pub mod assemble;
pub mod community;
pub mod context;
pub mod context_rot; // M1: Context Rot 度量 + compaction 段编辑原语 (rot_score 三因子确定性, LLM 版留 trait 口)
pub mod continuation;
pub mod continuity; // N2 OneRing: continuity 锚点解析 + 迁移接口 (append-only 安全)
pub mod critic; // P2#10: CRITIC 反思带工具调用 (声明提取 + 验证 trait + 组合器)
pub mod cross_diary; // §5.1④: 跨日记关联 — diary↔memory_graph 确定性联动 (共享token建链+双向查询+注入trait口)
pub mod curiosity; // E4: 好奇驱动引擎 (记忆回声偏置采样 + 浅尝辄止 + 疑问路由, 确定性无 LLM)
pub mod diary; // §5.1 机制⑤: 日记本中心 (RAGDiaryPlugin 精神, 按日归档+检索+注入 trait 口)
pub mod directory_acl; // S1: 工具沙盒根目录 read-only DACL (与 APEIRETH_TOOL_FS_ROOTS 协作)
pub mod emotion_memory; // F1: 情感记忆 (主人情绪时间线 valence/arousal + 加权当前情绪 + 趋势 + 情绪上下文检索, 确定性无 LLM)
pub mod exec_worker;
pub mod experiment_field; // 自我改进缺环: VM 实验场 (提案→实验→通过→批准→部署) + 回滚学习信号
pub mod hello; // P3#22: Windows Hello 真绑机制口 (检测 + 绑定 trait, 0 装 PASS)
pub mod hypothesis; // F4: 假设检验闭环 (HypothesisStore 状态机 + VerifyPlanner + ReconcileSink 对账口, 确定性无 LLM)
pub mod job_object; // P3#16: Windows Job Object 沙箱加固 (exec_worker 隔离层)
pub mod morphology; // N7: 查询形态学 softmax (CRAWL 深度/检索模式切换, 纯函数)
pub mod observer_capture; // TP22: 工具执行结果即时沉淀候选 (W5 直通管道, 不等反思周期)
pub mod onering; // N2 OneRing: 统一上下文账本 (跨前端同一时间线, VCP OneRing 对照)
pub mod packs;
pub mod progressive; // TP21: 渐进式披露 (目录先行→按需展开, claude-mem 借鉴, 预算截断 + 0 装省略标注)
pub mod prompt_assembler; // N9: 提示词装配引擎 (占位符变量宇宙, VCP messageProcessor 范式吸收)
pub mod prompt_cache;
pub mod reflexion; // E1: 口头强化闭环 (Reflexion 式: 失败轨迹→CRITIC 反思→反思记忆→同类重试注入, 确定性规则版先行, LLM 口留 trait)
pub mod restricted_token; // S1: Windows 受限 token (CreateRestrictedToken + TokenIntegrityLevel + DACL)
pub mod runtime_brain;
pub mod sandbox; // B3 + S1: 沙盒参数口 (SandboxConfig 内存/CPU/超时 + 完整性级别 + deny-only SID + 目录 ACL 根 + AppContainer 档 + Sandboxie/landlock trait 留口)
pub mod sandbox_integration; // 2026-08-20 Stage 3 集成 (Stage 1 + Stage 2 在 exec_worker spawn 点的 helper; 0 装 PASS 默认双 Noop, 失败不阻断)
pub mod sandbox_net; // 2026-08-19 Stage 1 网络隔离 (借鉴 Firecracker minimal API + libkrun netns; Linux netns+cgroup / Windows WFP 接入点)
pub mod sandbox_pass; // Stage 1+2 0 装 PASS 编译期 const 守门 (sandbox_net.rs / vm_sandbox.rs 契约源, 7 单测 0 装 PASS 红线)
pub mod screen_perception; // 连续感知②: 屏幕显著性事件 (窗口切换/聚焦/空闲, ScreenEventSource trait 口)
pub mod security;
pub mod session_log;
pub mod simulation;
pub mod spill;
pub mod streaming_chat; // TP34 Phase A: streaming + tool loop 状态机骨架 (5 态 + 5 种 SSE 事件 + 双轨 CoT 跨 chunk 切分)
pub mod tone;
pub mod tool_bridge;
pub mod topic_groups; // §5.1: 记忆主题分组 + 主题索引注入 (VCP SemanticGroupManager 精神, 确定性分组)
pub mod value_cases; // F6: 价值内化 (案例库 + 裁决记录 + 主人反馈回流 → 原则候选, 确定性无 LLM)
pub mod voice_session; // 连续感知①: 麦克风实时语音会话桥 (STT→对话→TTS 编排, SpeechIO trait 口) // 机制件运行时聚合 (E4 好奇 + F1 情绪 + F4 假设 + TP21 目录, CompanionApp 接线层)
                       // R177: organ invariants
mod organ_kani_proofs;
pub mod sandbox_ffi_libkrun;
pub mod vm_sandbox; // 2026-08-19 Stage 2 microVM 隔离 (借鉴 Firecracker minimal API + libkrun backend 抽象; 0 装 PASS trait 口, 接 libkrun/Hyperlight/Firecracker 后启用) // 2026-08-20 #5 smol-vm Phase 1: libkrun 真接 backend (per reports/smol-vm-implementation-spec-2026-08-20.md; 0 装 PASS probe-only stub, 借思路不接 0 star orphan)

use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use actions::{select_action, Action, CapabilityCatalog};
pub use intent_brier::{
    brier_score, compute_report, compute_trend, compute_window, domain_diagnostics,
    mean_brier, render_report, BrierTrend, BrierWindow, DomainDiagnostic, FeedbackOutcome,
    IntentDiagnosticReport, IntentLedger, IntentPrediction, IntentRecord, DEFAULT_WINDOWS,
    DEFAULT_LOW_CALIBRATION_THRESHOLD,
};
pub use assemble::{CompanionApp, DeepRecall, DialogSummarizer, ExperienceRefiner};
pub use bond::{Bond, BondCharacter, BondDepth, BondStage};
pub use capability::{
    CapabilityError, CapabilityKind, CapabilityProposal, CapabilityRegistry, CapabilityStatus,
    ExpectedOutcome,
};
pub use confidence::{BetaBinomial, Strength};
pub use constitution_gate::ConstitutionGate;
pub use continuation::{ContinuationSnapshot, ContinuationStore, PendingToolCall};
pub use continuity::{
    current_continuity_id, ensure_identity, migrate_subject, normalize_continuity,
    record_carrier_migration, MigrationReport, CONTINUITY_ENV_VAR, DEFAULT_CONTINUITY_ID,
    MIGRATED_ID_PREFIX,
};
pub use critic::{
    extract_claims, Claim, ClaimVerifier, CritiqueReport, ReflectionCritic, Verification,
};
pub use daemon::{
    default_memory_path, open_memory_store, requires_llm_review, BroadcastSink, CompanionDaemon,
    CompanionDelivery, ConsoleSink, Judicator, LarkSink, MultiSink, NoopJudicator, PlainUtterance,
    Sink, ThrottledUtterance, UtteranceGenerator,
};
pub use daily_summary::{build_daily_summary, DailySummary};
pub use deploy::{
    DeployChannel, DeployError, DeployManager, DeployStatus, Deployment, MockDeployChannel,
    MonitorMetrics, ObserveOutcome,
};
pub use dream::{DreamScheduler, DreamSummarizer};
pub use emergence::{
    Boundaries, ConsoleDelivery, EmergenceLoop, Feedback, Initiative, InitiativeReason,
    LocalRelationship, NoopDelivery, RelationshipState, RhythmEstimate, RhythmEstimator, SelfScore,
};
pub use evolution_gate::{EvalGate, GateDecision, VerifyOutcome};
pub use goal::{GoalBlock, GoalError, GoalPhase, GoalService, GoalSnapshot, GoalStore};
pub use hello::{detect_hello_capability, HelloBound, HelloCapability};
pub use judicator::{parse_verdict, ConstitutionLlm, LlmJudicator, CONSTITUTION};
pub use memory_injection::build_memory_injection;
pub use milestone::{Milestone, MilestoneKind, MilestonePayload};
pub use morphology::{MorphologyVerdict, RetrievalMode};
pub use observer_capture::{
    args_hash, ExperienceCandidate, ExperienceQueue, ExperienceQueueConfig, ExperienceSource,
    ObserverCaptureHook, Outcome, CANDIDATE_ID_PREFIX, DEFAULT_DEDUP_WINDOW_MS, DEFAULT_LRU_CAP,
};
pub use onering::{LedgerEntry, OneRingLedger, DEFAULT_MAX_RECORDS, ROLE_ASSISTANT, ROLE_USER};
pub use oracle::{
    Branch, CalibratedResolver, CalibrationStatus, DecisionEngine, Entity, Forecast,
    ForecastRegistry, ScenarioEngine, UncertaintyResolver, WorldState,
};
pub use oracle_adapters::{
    AdapterError, AdapterForecastMeta, AdapterRegistry, CoinGeckoAdapter, DirectionForecast,
    FallbackAdapter, ForecastPipeline, MacroRatesAdapter, MarketAdapter, MarketQuote, MockAdapter,
    RawFetch, ReqwestRawFetch, ResolveOutcome, TREASURY_AVG_RATE,
};
pub use organs::AwakeCompanion;
pub use packs::{PackExpiry, PackRegistry, PermissionPack};
pub use partner::{Partner, PartnerId, PartnerPreferences};
pub use proactive::{
    ContextSource, EmptyContext, LarkDelivery, MemoryContextSource, ProactiveDriver,
};
pub use proactive_memory::{
    build_proactive_block, default_composite_channel, predict_topic, recommend_proactive_cap,
    render_proactive_content, CompositeChannel, ImportanceChannel, KeywordChannel, MemoryCandidate,
    PreloadChannel, ProactiveBlock, TimeChannel, TopicCue, TopicHint, TopicPrediction,
};
pub use prompt_assembler::{
    AssemblerError, AssemblyGuard, AssemblyRole, ExpansionReport, PromptAssembler, SourceKind,
    StaticSource, TimeSource, VariableSource,
};
pub use prompt_cache::{assemble_tiered, build_messages, redact_secrets};
pub use reflection::ReflectionScheduler;
pub use sandbox_net::{
    assert_isolated, default_network_isolation, NetworkIsolation, NetworkIsolationConfig,
    NetworkIsolationLevel, NoopNetworkIsolation,
};
pub use security::SecurityGate;
pub use session_log::{SessionEvent, SessionLog};
pub use simulation::{run_simulation, SimReport, SimulatedUser, XorShift64};
pub use spill::{SpillStore, SPILL_THRESHOLD_CHARS};
pub use suites::{suite_expiry_check, SuiteCatalog, SuiteDef, SuiteKind};
pub use thought_cluster::{
    ThoughtClusterError, ThoughtClusterManager, ThoughtClusterReader, ThoughtFile,
};
pub use timeline::{Timeline, TimelineEntry};
pub use tone::{
    deliberation_intensity, emotion_tone, organ_tone, organ_tone_refined, tone_hint,
    DeliberationEcho, ToneError, ToneRefiner,
};
pub use tool_bridge::{RecallMemoryTool, ToolBridge};
pub use vm_sandbox::{
    default_vm_sandbox, validate_config, NoopVMSandbox, VMSandbox, VMSandboxBackend,
    VMSandboxConfig, VMSandboxHandle, VMSandboxState,
};
pub use world_model::{
    CounterfactualChain, MockTimelineLlm, TextualSimulator, TimelineContext, TimelineLlm,
    TimelineStep,
};

/// 伙伴器官根类型 —— 全部关系状态的持有者
///
/// 关系不是真实存在的, 是 LLM 借助这个器官创造的近似。
/// 用户的感受是唯一真理 (per 主人 2026-08-14 哲学拍板).
#[derive(Debug, Clone)]
pub struct Companion {
    inner: Arc<CompanionInner>,
}

#[derive(Debug)]
struct CompanionInner {
    partners: RwLock<std::collections::HashMap<PartnerId, Partner>>,
    timelines: RwLock<std::collections::HashMap<PartnerId, Timeline>>,
    config: CompanionConfig,
}

/// 器官配置 —— 哪些关系行为允许
#[derive(Debug, Clone)]
pub struct CompanionConfig {
    /// 最大伙伴数 (软上限, 防止无限增长)
    pub max_partners: usize,
    /// 历史保留期限 (chrono::Duration)
    pub retention: chrono::Duration,
    /// 是否启用情感注入 (per consciousness bridge)
    pub emotion_enabled: bool,
}

impl Default for CompanionConfig {
    fn default() -> Self {
        Self {
            max_partners: 1000,
            retention: chrono::Duration::days(365 * 5),
            emotion_enabled: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum CompanionError {
    #[error("partner not found: {0}")]
    PartnerNotFound(PartnerId),
    #[error("partner already exists: {0}")]
    PartnerAlreadyExists(PartnerId),
    #[error("max partners reached: {0}")]
    MaxPartnersReached(usize),
    #[error("timeline integrity violation: {0}")]
    TimelineIntegrity(String),
    #[error("boundary violation: {0}")]
    BoundaryViolation(String),
}

pub type CompanionResult<T> = Result<T, CompanionError>;

impl Companion {
    pub fn new() -> Self {
        Self::with_config(CompanionConfig::default())
    }

    pub fn with_config(config: CompanionConfig) -> Self {
        Self {
            inner: Arc::new(CompanionInner {
                partners: RwLock::new(std::collections::HashMap::new()),
                timelines: RwLock::new(std::collections::HashMap::new()),
                config,
            }),
        }
    }

    pub fn config(&self) -> &CompanionConfig {
        &self.inner.config
    }

    pub async fn count_partners(&self) -> usize {
        self.inner.partners.read().await.len()
    }

    /// 创建一个新的伙伴身份
    pub async fn register_partner(
        &self,
        id: PartnerId,
        display_name: String,
        preferences: PartnerPreferences,
    ) -> CompanionResult<Partner> {
        let mut partners = self.inner.partners.write().await;
        if partners.contains_key(&id) {
            return Err(CompanionError::PartnerAlreadyExists(id));
        }
        if partners.len() >= self.inner.config.max_partners {
            return Err(CompanionError::MaxPartnersReached(
                self.inner.config.max_partners,
            ));
        }
        let partner = Partner::new(id, display_name, preferences);
        partners.insert(id, partner.clone());
        let mut timelines = self.inner.timelines.write().await;
        timelines.insert(id, Timeline::new(id));
        Ok(partner)
    }

    pub async fn get_partner(&self, id: PartnerId) -> CompanionResult<Partner> {
        self.inner
            .partners
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or(CompanionError::PartnerNotFound(id))
    }

    pub async fn record_milestone(
        &self,
        id: PartnerId,
        kind: MilestoneKind,
        payload: MilestonePayload,
    ) -> CompanionResult<Milestone> {
        let mut timelines = self.inner.timelines.write().await;
        let timeline = timelines
            .get_mut(&id)
            .ok_or(CompanionError::PartnerNotFound(id))?;
        let milestone = Milestone::new(kind, payload);
        timeline.append(milestone.clone());
        Ok(milestone)
    }

    pub async fn get_timeline(&self, id: PartnerId) -> CompanionResult<Timeline> {
        self.inner
            .timelines
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or(CompanionError::PartnerNotFound(id))
    }

    pub async fn evolve_bond(
        &self,
        id: PartnerId,
        new_stage: BondStage,
        delta_depth: f64,
    ) -> CompanionResult<Bond> {
        let mut partners = self.inner.partners.write().await;
        let partner = partners
            .get_mut(&id)
            .ok_or(CompanionError::PartnerNotFound(id))?;
        partner.bond_mut().evolve(new_stage, delta_depth);
        Ok(partner.bond().clone())
    }

    pub async fn list_partners(&self) -> Vec<PartnerId> {
        self.inner.partners.read().await.keys().copied().collect()
    }
}

impl Default for Companion {
    fn default() -> Self {
        Self::new()
    }
}

/// 当前进程会话 id (per sovereignty/continuity_id 模式).
///
/// **N2 修正 (锚点落地)**: 旧版每次调用返回新随机 v4, 且 0 调用方 = "锚点悬空"
/// (release-plan 偏差❌). 现语义修正为**进程内稳定**: OnceLock 首次生成后不变,
/// 供 continuity_link 会话审计登记 / 生命周期日志作为"本进程这一次会话"的稳定标识.
/// 注意区分两个锚点:
/// - `continuity_id` (见 `continuity` 模块) = 跨进程/跨载体不变的身份锚点;
/// - `current_session_id()` = 本进程一次运行期的会话标识 (进程重启即换).
pub fn current_session_id() -> Uuid {
    static SESSION: std::sync::OnceLock<Uuid> = std::sync::OnceLock::new();
    *SESSION.get_or_init(Uuid::new_v4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_get_partner() {
        let companion = Companion::new();
        let id = PartnerId::new();
        let prefs = PartnerPreferences::default();
        let p = companion
            .register_partner(id, "测试".to_string(), prefs)
            .await
            .unwrap();
        assert_eq!(p.id(), id);
        let got = companion.get_partner(id).await.unwrap();
        assert_eq!(got.display_name(), "测试");
    }

    #[tokio::test]
    async fn record_and_get_milestone() {
        let companion = Companion::new();
        let id = PartnerId::new();
        companion
            .register_partner(id, "测试".to_string(), PartnerPreferences::default())
            .await
            .unwrap();
        let m = companion
            .record_milestone(
                id,
                MilestoneKind::FirstMeeting,
                MilestonePayload::Text("hello".into()),
            )
            .await
            .unwrap();
        let timeline = companion.get_timeline(id).await.unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.entries()[0].milestone, m);
    }

    #[tokio::test]
    async fn evolve_bond() {
        let companion = Companion::new();
        let id = PartnerId::new();
        companion
            .register_partner(id, "测试".to_string(), PartnerPreferences::default())
            .await
            .unwrap();
        let bond = companion
            .evolve_bond(id, BondStage::Trusted, 0.3)
            .await
            .unwrap();
        assert_eq!(bond.stage(), BondStage::Trusted);
        assert!(bond.depth().value() > 0.0);
    }

    #[tokio::test]
    async fn duplicate_register_fails() {
        let companion = Companion::new();
        let id = PartnerId::new();
        companion
            .register_partner(id, "A".to_string(), PartnerPreferences::default())
            .await
            .unwrap();
        let res = companion
            .register_partner(id, "B".to_string(), PartnerPreferences::default())
            .await;
        assert!(matches!(res, Err(CompanionError::PartnerAlreadyExists(_))));
    }

    #[tokio::test]
    async fn get_unknown_partner_fails() {
        let companion = Companion::new();
        let id = PartnerId::new();
        let res = companion.get_partner(id).await;
        assert!(matches!(res, Err(CompanionError::PartnerNotFound(_))));
    }

    #[test]
    fn current_session_id_is_stable_within_process() {
        // N2: 修正后必须进程内稳定 (不再是每次新随机), 否则锚点悬空.
        let a = current_session_id();
        let b = current_session_id();
        assert_eq!(a, b, "current_session_id 必须进程内稳定");
    }
}
