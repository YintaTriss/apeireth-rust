//! apeireth-memory: 记忆子系统 (Episode/Note/Session SQLite 存储 + 6 历史流 Append-only Log + IdentityCard 跨载体唯一)
//!
//! R14 A4 成就落地:
//! 1. SQLite schema = 6 历史流表 (思想/提案/行动/关系/演化/反思期)
//!                  + `identity_cards` (continuity_id UNIQUE 跨载体)
//!                  + `episodes` (按 session_id / time range / continuity_id 索引查询)
//! 2. 6 个 Append-only Log trait: 思想/提案/行动/关系/演化/反思期
//! 3. Append-only = `BEFORE UPDATE` / `BEFORE DELETE` triggers raise ABORT
//! 4. IdentityCard.continuity_id = UNIQUE 约束, 跨载体去重
//! 5. Episode 写入 + 查询 API (按 session_id / time range / continuity_id)
//! 6. 直接 SQL (主人偏好: 不引入 ORM)
//!
//! 禁止:
//! - ❌ 不修改 apeireth-core 任何已实装类型签名
//! - ❌ 不引入 ORM (按主人偏好)
//! - ❌ 不碰 R11 baseline 三值
//! - ❌ 不碰 apeireth-legacy/

#![deny(unsafe_code)]

use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use apeireth_core::{Episode, IdentityCard, Note, Session};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod append_only;
mod episode;
mod identity;
mod migrations;
// R19 P2 战区 4: 公开 semantic + user_profile 模块 (bench 依赖)
// 54ed4c7d: semantic 模块内部分割 — 纯件 (EmbedFn/HashEmbedder/EmbedderIdentity/episode_uuid) 无条件,
// 向量路径 (SemanticIndex 等) 挂 semantic feature; semantic_persist/user_profile 整模块挂门控.
pub mod semantic;
// R19 P2 战区 4 续 (A-3): 公开 semantic_persist 模块 (跨 daemon 持久化路径)
#[cfg(feature = "semantic")]
pub mod semantic_persist;
// N8: generation 绑定观测缓存 (自包含, VCP MemoRuntime 精神, artifact_sig 联动口; 移交续接; merge 吞行后二次补回)
pub mod gen_cache;
// P2#12: 本地 ONNX embedding (feature onnx; 关闭时诚实 Err + hash 降级)
pub mod onnx;
// R179 P1-9: Episode Dedup (借鉴 mempalace dedup.py — session 内近重复检测)
pub mod dedup;
// R179 P1-10: Hallway — wing 内 entity-pair 跨位置走廊 (借鉴 mempalace hallways.py)
pub mod hallways;
mod session_note;
mod streams;
mod three_layer; // R30 U9: claude-mem 3 层 facade
                 // R19 P2 战区 4: 公开 user_profile 模块 (bench 后续可依赖)
#[cfg(feature = "semantic")]
pub mod user_profile;

pub use append_only::{AppendOnlyError, HistoryEntry, HistoryStream, Tombstone};
// R22 ST-A2.4 — 6 历史流深度公共 API (query / insert / count)
pub mod history_streams;

pub mod continuity_link;
pub use episode::{EpisodeQuery, EpisodeStore};
// TP24 (M5 + N25): 记忆来源链 + 时间元数据 (episodes 表的 4 列 V4 扩展).
// 方法以 inherent impl on SqliteMemoryStore 暴露, 不引入 trait (减少 import, 保持向后兼容).
pub mod provenance;
pub use provenance::{normalize_meta, validate_meta, EpisodeMeta, Provenance};
pub mod llm_analysis;
pub use identity::{IdentityCardRecord, IdentityCardStore, IdentityConflict};
pub use llm_analysis::{analyze_episode, AnalysisKind, AnalysisResult};
pub use migrations::{run_migrations, Migration as SchemaMigration, MIGRATIONS};
// R19 P2 战区 4: 公开 EmbedFn / SemanticIndex / UserProfile / ProfileExtractor
// (semantic + user_profile 模块是新文件, 0 触碰 LOCKED 9 文件)
// 54ed4c7d: 纯件无条件导出; SemanticIndex/PersistentSemanticIndex/user_profile 挂 semantic feature
#[cfg(feature = "semantic")]
pub use semantic::SemanticIndex;
pub use semantic::{episode_uuid, EmbedFn, HashEmbedder};
// R19 P2 战区 4 续 (A-3): 公开 PersistentSemanticIndex (跨 daemon 长程索引)
#[cfg(feature = "semantic")]
pub use semantic_persist::PersistentSemanticIndex;
pub use session_note::{NoteQuery, NoteRecord, NoteStore, SessionRecord, SessionStore};
// Core Capability Expansion Phase 2: 后端会话生命周期 (state machine + 乐观并发).
// 独立于 SessionStore trait (旧 upsert 不变), 走 inherent impl on SqliteMemoryStore.
pub mod session_lifecycle;
pub use session_lifecycle::{
    SessionLifecycleError, SessionLifecycleRecord, SessionScope, SessionState,
    SessionStore as SessionLifecycleStore,
};
// Core Capability Expansion Phase 3: 记忆治理 (forget/protect/update, 不破坏 append-only episodes).
pub mod memory_governance;
pub use memory_governance::{
    GovernedEpisode, MemoryGovernanceError, MemoryGovernanceStatus, MemoryGovernanceStore,
};
// Core Capability Expansion Phase 5: Agent 执行轨迹 (structured trace, 持久化 + 查询).
pub mod agent_trace;
pub use agent_trace::{TraceQueryError, TraceSpan, TraceSpanKind, TraceSpanStatus, TraceStore};
pub use streams::{
    ActionStream, EvolutionStream, GoalStream, LifeStream, MigrationStream, ProposalStream,
    ReflectionStream, RelationStream, StanceStream, ThoughtStream,
};
pub use three_layer::{ThreeLayerMemory, SHORT_TERM_WINDOW_SECS, WORKING_CAPACITY}; // R30 U9
#[cfg(feature = "semantic")]
pub use user_profile::{ProfileEmbedder, ProfileExtractor, UserProfile};

/// 重新导出 `apeireth_core::Episode` 方便下游不必记多个导入路径.
pub use apeireth_core::Episode as CoreEpisode;
// R177: organ invariants (10 tests + 2 Kani proofs)
mod organ_kani_proofs;
// R23 #6 派工: 从 extensions/ 子 crate re-export 3 Provider (in_memory / file / mongodb).
// 透明登记: 此处 +1 行 (pub use), 不动 LOCKED 9 文件 (append_only / identity / migrations /
// episode / session_note / streams / history_streams / continuity_link / llm_analysis).
pub use apeireth_memory_extensions::{
    provider_file::FileProvider, provider_in_memory::InMemoryProvider,
    provider_mongodb::MongoDbProvider,
};
// R37-2: 9 organ 部分合并 — life_force 透明 re-export 到 memory.
// 注释修正 (R131 P0-1 真实化): apeireth-life-force 仍是 workspace member
// (path dep 在 Cargo.toml:93), re-export 仅为 API 便利, 0 breaking.
// 显式列出导出 (避免 `pub use ...::*` 隐藏依赖关系).
pub use apeireth_life_force::{
    // 子模块 re-export
    emergence::{
        EmergenceDetector, EmergenceError, EmergenceReport, EmergenceSignal, EmergenceSignalType,
    },
    exhaustion_check,
    recovery_start,
    reflection_cycle::{
        ReflectionCycleError, ReflectionCycleEvent, ReflectionCycleScheduler, ReflectionPhase,
    },
    reflection_progress,
    // 触发函数
    reflection_trigger,
    validate_endurance,
    LifeForce,
    LifeForceError,
    ReflectionPeriod,
    ReflectionPeriodState,
    ReflectionTrigger,
    // 核心类型
    SelfGrowthIndicator,
    StandardReflectionPeriod,
    ENDURANCE_EXHAUSTION_THRESHOLD,
    ENDURANCE_MAX,
    // 常量
    ENDURANCE_MIN,
    ENDURANCE_RECOVERY_TARGET,
};

/// 顶层错误: 所有 memory 子系统的 fallback error.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// SQLite 底层错误.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Append-only 约束被违反.
    #[error("append-only violation: {0}")]
    AppendOnly(#[from] AppendOnlyError),
    /// IdentityCard continuity_id 冲突.
    #[error("identity conflict: {0}")]
    Identity(#[from] IdentityConflict),
    /// JSON 序列化/反序列化失败.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// 调用方提供的参数非法 (空字符串 / 时间范围倒置等).
    #[error("invalid argument: {0}")]
    Invalid(String),
    /// 互斥锁中毒 (panic 持有锁后).
    #[error("memory store mutex poisoned: {0}")]
    Poisoned(String),
    /// R19 P2 战区 4: vector/semantic 子系统错误 (vec0 操作 / embedder / 等).
    /// 加在 lib.rs 顶层 (不触碰 9 LOCKED 文件, 与 R23 P3 / R37-2 透明 re-export 同模式).
    #[error("memory subsystem error: {0}")]
    Other(String),
}

/// 统一结果类型.
pub type MemoryResult<T> = Result<T, MemoryError>;

/// 流枚举 (按主人 A4 描述与 D2 §5 三域映射):
/// - Thought      → 思想流 (思想域, 对应 §5 目标史 + 自我叙事)
/// - Proposal     → 提案流 (提案域, 对应 §5 立场史)
/// - Action       → 行动流 (行动域, 对应 §5 生命史)
/// - Relation     → 关系流 (行动域, 对应 §5 关系史)
/// - Evolution    → 演化流 (思想 + 提案, 对应 §5 自我叙事)
/// - Reflection   → 反思期流 (反思期审计, Self-Disable §3 使用)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamKind {
    /// 思想流.
    Thought,
    /// 提案流.
    Proposal,
    /// 行动流.
    Action,
    /// 关系流.
    Relation,
    /// 演化流.
    Evolution,
    /// 反思期流.
    Reflection,
}

impl StreamKind {
    /// 返回对应的物理表名 (snake_case).
    pub const fn table_name(self) -> &'static str {
        match self {
            StreamKind::Thought => "thought_stream",
            StreamKind::Proposal => "proposal_stream",
            StreamKind::Action => "action_stream",
            StreamKind::Relation => "relation_stream",
            StreamKind::Evolution => "evolution_stream",
            StreamKind::Reflection => "reflection_stream",
        }
    }

    /// D2 §5 对应的语义命名 (供 UI / 报告使用).
    pub const fn semantic_name(self) -> &'static str {
        match self {
            StreamKind::Thought => "思想 (Thought)",
            StreamKind::Proposal => "提案 (Proposal)",
            StreamKind::Action => "行动 (Action)",
            StreamKind::Relation => "关系 (Relation)",
            StreamKind::Evolution => "演化 (Evolution)",
            StreamKind::Reflection => "反思期 (Reflection Period)",
        }
    }

    /// 全部 6 种流 (按主人 2026-07-31 指示顺序).
    pub const ALL: [StreamKind; 6] = [
        StreamKind::Thought,
        StreamKind::Proposal,
        StreamKind::Action,
        StreamKind::Relation,
        StreamKind::Evolution,
        StreamKind::Reflection,
    ];
}

impl FromStr for StreamKind {
    type Err = MemoryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "thought" => Ok(StreamKind::Thought),
            "proposal" => Ok(StreamKind::Proposal),
            "action" => Ok(StreamKind::Action),
            "relation" => Ok(StreamKind::Relation),
            "evolution" => Ok(StreamKind::Evolution),
            "reflection" => Ok(StreamKind::Reflection),
            other => Err(MemoryError::Invalid(format!(
                "unknown stream kind: {other}"
            ))),
        }
    }
}

/// 统一的内存存储入口 (SQLite 实现).
///
/// 内部持有一个 `Mutex<rusqlite::Connection>`, 默认开启 `WAL` + `foreign_keys`,
/// 并在构造时跑完所有 schema migration (见 [`MIGRATIONS`]).
#[derive(Debug)]
pub struct SqliteMemoryStore {
    conn: Mutex<Connection>,
}

impl SqliteMemoryStore {
    /// 在给定 path 打开一个 SQLite 数据库, 应用 migrations.
    pub fn open(path: impl AsRef<Path>) -> MemoryResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.configure()?;
        {
            let mut guard = store.conn.lock().expect("memory store mutex");
            run_migrations(&mut guard)?;
        }
        Ok(store)
    }

    /// 打开一个内存数据库 (主要用于测试, 每次新建独立 store).
    pub fn open_in_memory() -> MemoryResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.configure()?;
        {
            let mut guard = store.conn.lock().expect("memory store mutex");
            run_migrations(&mut guard)?;
        }
        Ok(store)
    }

    fn configure(&self) -> MemoryResult<()> {
        let conn = self.conn.lock().map_err(map_poisoned)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }

    /// 拿到内部 connection 的锁. 调用方应尽快完成操作并释放.
    pub fn conn(&self) -> MemoryResult<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(map_poisoned)
    }

    /// 列出已应用的 migration 版本号.
    pub fn applied_migrations(&self) -> MemoryResult<Vec<i64>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT version FROM schema_migrations ORDER BY version ASC")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 一键导出所有 6 历史流条目 (按时间排序), JSON Lines 友好的结构.
    ///
    /// D2 §5.3 硬规则 #4: "可导出" — 6 历史流必须可一键导出.
    pub fn export_streams_jsonl(&self) -> MemoryResult<Vec<HistoryEntry>> {
        let conn = self.conn()?;
        append_only::export_all_streams(&conn)
    }

    /// R19 P2 战区 4: 一次性语义搜索 (用 in-memory vector store).
    ///
    /// 适合 "偶发" 查询; 高频场景请自己持 `SemanticIndex` 复用.
    ///
    /// 流程:
    /// 1. 拉 memory 里所有 episodes (limit 100_000)
    /// 2. 在 in-memory vec0 backend 重建索引
    /// 3. embed query + KNN 检索
    /// 4. 反查 episode 返回
    ///
    /// 返回 episodes 按相似度降序, 长度 <= k.
    #[cfg(feature = "semantic")]
    pub fn semantic_search(
        &self,
        query: &str,
        k: usize,
        embedder: Arc<dyn EmbedFn>,
    ) -> MemoryResult<Vec<Episode>> {
        use apeireth_vector::{SqliteVecBackend, VectorStore};

        if k == 0 {
            return Ok(Vec::new());
        }

        // 1. 拉所有 episodes.
        let eps = <Self as EpisodeStore>::query(self, &EpisodeQuery::new().limit(100_000))?;
        if eps.is_empty() {
            return Ok(Vec::new());
        }

        // 2. in-memory vec0 backend.
        let mut backend = SqliteVecBackend::open_in_memory()
            .map_err(|e| MemoryError::Other(format!("vector open: {e}")))?;
        backend
            .set_dimension(embedder.dim())
            .map_err(|e| MemoryError::Other(format!("vector set_dim: {e}")))?;
        let index = SemanticIndex::new(self, Box::new(backend), embedder);

        // 3. 索引 + 检索.
        index.index_episodes(&eps)?;
        index.search(query, k)
    }

    /// R19 P2 战区 4: 一次性提取用户画像.
    ///
    /// 跟 `semantic_search` 一样, 一次性 in-memory index.
    /// 高频场景请自己持 `SemanticIndex` 复用.
    #[cfg(feature = "semantic")]
    pub fn extract_user_profile(&self, embedder: Arc<dyn EmbedFn>) -> MemoryResult<UserProfile> {
        use apeireth_vector::SqliteVecBackend;

        let backend = SqliteVecBackend::open_in_memory()
            .map_err(|e| MemoryError::Other(format!("vector open: {e}")))?;
        let index = SemanticIndex::new(self, Box::new(backend), embedder);
        index.extract_profile()
    }

    // ============================================================================
    // R19 P2 战区 4 续 (A-3): 跨 daemon 持久化长程 API
    // ----------------------------------------------------------------------------
    // 跟 A 一次性的 `semantic_search` / `extract_user_profile` 0 冲突.
    // 1:1 对齐: 一次性用 in-memory vec0 重建, 长程用 path-based vec0 真接 disk.
    // ============================================================================

    /// 打开一个跨 daemon 持久化的 `PersistentSemanticIndex`.
    ///
    /// 跟 A 一次性 `SemanticIndex::new` 区别:
    /// - 内部 `Arc<SqliteMemoryStore>` (不借用, 跨 daemon 共享)
    /// - 内部 `SqliteVecBackend::open(path)` 真接 disk (write-through WAL)
    /// - `save()` 公开 (实际 no-op, WAL 已 write-through)
    /// - `as_semantic_index(&mem)` 桥接 A 一次性 API 借用视图
    ///
    /// 用法: daemon 启动时 `open`, 运行时复用, daemon 关闭时 `save` (no-op).
    #[cfg(feature = "semantic")]
    pub fn open_persistent_semantic_index(
        self: &Arc<Self>,
        vector_path: impl AsRef<std::path::Path>,
        embedder: Arc<dyn EmbedFn>,
    ) -> MemoryResult<PersistentSemanticIndex> {
        PersistentSemanticIndex::open(Arc::clone(self), vector_path, embedder)
    }

    /// 一次性持久化语义搜索 (便捷方法).
    ///
    /// 等价于:
    /// ```ignore
    /// let arc_self: Arc<SqliteMemoryStore> = Arc::new(...);
    /// let idx = arc_self.open_persistent_semantic_index(vector_path, embedder)?;
    /// let hits = idx.search(query, k)?;
    /// idx.save()?;
    /// hits
    /// ```
    /// 但不开 `PersistentSemanticIndex` 长期持有, 一次调用完.
    ///
    /// 跟 A 一次性 `semantic_search` 区别: 复用 path-based vec0 db, 跨 daemon
    /// 重启数据不丢. 首次调用会建立空 index; 后续调用累积.
    #[cfg(feature = "semantic")]
    pub fn semantic_search_persistent(
        self: &Arc<Self>,
        query: &str,
        k: usize,
        vector_path: impl AsRef<std::path::Path>,
        embedder: Arc<dyn EmbedFn>,
    ) -> MemoryResult<Vec<Episode>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        // 1. 拉所有 episodes.
        let eps = <Self as EpisodeStore>::query(self, &EpisodeQuery::new().limit(100_000))?;
        if eps.is_empty() {
            return Ok(Vec::new());
        }
        // 2. 打开 / 复用 path-based vec0.
        let idx = PersistentSemanticIndex::open(Arc::clone(self), vector_path, embedder)?;
        // 3. 索引 + 检索.
        idx.index_episodes(&eps)?;
        idx.search(query, k)
    }
}

fn map_poisoned(e: std::sync::PoisonError<std::sync::MutexGuard<'_, Connection>>) -> MemoryError {
    MemoryError::Poisoned(e.to_string())
}

// ============================================
// 兼容旧 trait: ContinuitySnapshotStore (A1 阶段 CLI 已引用)
// ============================================

/// ContinuitySnapshotStore trait (Phase 1 实现, 对齐 mvp/memory/store.py).
///
/// 该 trait 在 A1 阶段由 `apeireth-cli` 调用, 不可破坏签名.
/// A4 升级: SqliteMemoryStore 实现了完整版, 含 Append-only Log + IdentityCard.
pub trait ContinuitySnapshotStore: Send {
    /// 写入一个 Episode.
    fn put_episode(&self, ep: &Episode) -> anyhow::Result<()>;
    /// 写入一个 Note.
    fn put_note(&self, note: &Note) -> anyhow::Result<()>;
    /// 检索最近 N 条 Episodes.
    fn recent_episodes(&self, session_id: &str, n: usize) -> anyhow::Result<Vec<Episode>>;
}

impl ContinuitySnapshotStore for SqliteMemoryStore {
    fn put_episode(&self, ep: &Episode) -> anyhow::Result<()> {
        <Self as EpisodeStore>::put_episode(self, ep).map_err(Into::into)
    }

    fn put_note(&self, note: &Note) -> anyhow::Result<()> {
        <Self as NoteStore>::put_note(self, note).map_err(Into::into)
    }

    fn recent_episodes(&self, session_id: &str, n: usize) -> anyhow::Result<Vec<Episode>> {
        <Self as EpisodeStore>::recent_episodes(self, session_id, n).map_err(Into::into)
    }
}

/// 重新导出 `apeireth_core` 给下游 (避免下游写 `apeireth_core::*` 又引一次).
pub use apeireth_core;

#[cfg(test)]
pub(crate) fn fresh_store() -> SqliteMemoryStore {
    SqliteMemoryStore::open_in_memory().expect("open in-memory store")
}

#[cfg(test)]
mod tests {
    use super::*;
    use apeireth_core::Migration;

    #[test]
    fn open_in_memory_creates_schema() {
        let store = SqliteMemoryStore::open_in_memory().unwrap();
        let migrations = store.applied_migrations().unwrap();
        assert!(
            !migrations.is_empty(),
            "expected at least one migration to be applied"
        );
    }

    #[test]
    fn stream_kind_roundtrip() {
        for kind in StreamKind::ALL {
            let s: &'static str = match kind {
                StreamKind::Thought => "thought",
                StreamKind::Proposal => "proposal",
                StreamKind::Action => "action",
                StreamKind::Relation => "relation",
                StreamKind::Evolution => "evolution",
                StreamKind::Reflection => "reflection",
            };
            assert_eq!(StreamKind::from_str(s).unwrap(), kind);
            assert!(!kind.table_name().is_empty());
            assert!(!kind.semantic_name().is_empty());
        }
    }

    #[test]
    fn stream_kind_all_covers_six() {
        assert_eq!(StreamKind::ALL.len(), 6);
        let mut names: Vec<_> = StreamKind::ALL.iter().map(|k| k.table_name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 6, "6 历史流必须 6 张独立表");
    }

    #[test]
    fn continuity_trait_smoke() {
        let store = SqliteMemoryStore::open_in_memory().unwrap();
        let ep = Episode {
            id: "ep-smoke".into(),
            timestamp: 1_700_000_000,
            role: "user".into(),
            content: "hi".into(),
            session_id: "sess-smoke".into(),
        };
        ContinuitySnapshotStore::put_episode(&store, &ep).unwrap();
        let recent = ContinuitySnapshotStore::recent_episodes(&store, "sess-smoke", 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, "ep-smoke");
    }

    #[test]
    fn identity_card_record_roundtrip() {
        let card = IdentityCard {
            continuity_id: "cid-roundtrip".into(),
            birth_time: 1_700_000_000,
            carriers: vec!["carrier-a".into(), "carrier-b".into()],
            migration_history: vec![Migration {
                from_carrier: "carrier-a".into(),
                to_carrier: "carrier-b".into(),
                timestamp: 1_700_000_500,
            }],
        };
        let record = identity::IdentityCardRecord::from_core(&card);
        let back = record.into_core();
        assert_eq!(back.continuity_id, card.continuity_id);
        assert_eq!(back.birth_time, card.birth_time);
        assert_eq!(back.carriers, card.carriers);
        assert_eq!(back.migration_history.len(), 1);
    }

    #[test]
    fn session_and_note_record_roundtrip() {
        let session = Session {
            id: "sess-rt".into(),
            started_at: 1_700_000_000,
            last_active_at: 1_700_000_500,
        };
        let record = session_note::SessionRecord::from_core(&session);
        let back = record.into_core();
        assert_eq!(back.id, session.id);
        assert_eq!(back.started_at, session.started_at);
        assert_eq!(back.last_active_at, session.last_active_at);

        let note = Note {
            id: "n-rt".into(),
            timestamp: 1_700_000_600,
            content: "hello".into(),
            source_episode_ids: vec!["ep-1".into()],
            confidence: 0.7,
            tags: vec!["a".into()],
        };
        let nrecord = session_note::NoteRecord::from_core(&note);
        let nback = nrecord.into_core();
        assert_eq!(nback.content, note.content);
        assert!((nback.confidence - note.confidence).abs() < f64::EPSILON);
    }
}

/// R146: 3 memory crate -> 1 apeireth-memory (子模块)
///
/// dailynote: 按日期分区存储 (R141)
/// lightmemo: VCP production V3 拓扑简化 (R142-R143)
pub mod dailynote;
pub mod g5_memory_bridge;
pub mod lightmemo; // R161: memory insert/retrieve 5 步 -> g5 substrate (5th caller)

/// 编译期守门 (per O-5 不假装)
pub const MEMORY_SUBMODULE_COUNT: usize = 2;
