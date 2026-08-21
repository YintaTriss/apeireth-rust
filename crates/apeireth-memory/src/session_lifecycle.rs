//! Core Capability Expansion Phase 2 — 后端会话生命周期 (真实 source of truth).
//!
//! 设计目标: 让 backend session 成为真实生命周期 source of truth, 但**不**粗暴删除既有
//! `SessionStore` trait (旧 upsert 仍只写 4 列, 向后兼容). 本模块走 inherent impl on
//! `SqliteMemoryStore`, 操作 V5 扩展列 (title/scope/project_id/state/metadata/revision/...).
//!
//! ## Session Domain Model
//! 一个 Session 是一次完整对话/工作周期的载体, 关联其 episodes (messages). 字段:
//! - `id`: 稳定 ID (caller 提供 / UUID).
//! - `title`: 可重命名的人类可读标题.
//! - `scope` + `project_id`: 作用域 (global / project) + 可选项目.
//! - `state`: 状态机 (active / archived / closed).
//! - `metadata`: 自由 JSON 元数据 (不信任: 容量受限).
//! - `revision`: 单调递增, 乐观并发 (expected_rev CAS).
//! - 时间戳: started_at / updated_at / last_active_at / archived_at / closed_at.
//!
//! ## State Machine
//! ```text
//!   (create) ──► active ──archive──► archived ──restore──► active
//!                    │                    │
//!                    └──close─────────────►──close──► closed (终态)
//! ```
//! - `closed` 是终态: 不可再 archive/restore (但数据保留, 供审计/时间线查询).
//! - `archive` 是软操作 (隐藏于默认列表, 数据不删); 与 `forget`/`purge` 不同 (本轮不实现 hard delete).
//!
//! ## 删除策略 (诚实)
//! 本轮**不**实现 hard delete. memory/episode/audit 依赖 session, 直接 `DELETE FROM sessions`
//! 会留下 orphan episodes. 优先 archive/close (tombstone 语义). 永久删除需明确 cascading /
//! retention 语义, 留待后续 (不在本轮 P0 范围).
//!
//! ## 乐观并发
//! rename/archive/restore/close 携带 `expected_rev`; CAS 失败 → `Conflict` 错误, 避免最后写入者
//! 静默覆盖. transactional + validated.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{MemoryError, MemoryResult, SqliteMemoryStore};

/// 会话作用域.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScope {
    /// 全局 (不绑定项目).
    Global,
    /// 项目作用域 (需 project_id).
    Project,
}

impl SessionScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "project" => Self::Project,
            _ => Self::Global,
        }
    }
}

/// 会话状态机.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// 活跃 (默认).
    Active,
    /// 已归档 (软隐藏, 可恢复).
    Archived,
    /// 已关闭 (终态).
    Closed,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Closed => "closed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "archived" => Self::Archived,
            "closed" => Self::Closed,
            _ => Self::Active,
        }
    }
}

/// 会话生命周期记录 (V5 扩展列的完整视图).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLifecycleRecord {
    pub id: String,
    pub title: Option<String>,
    pub scope: SessionScope,
    pub project_id: Option<String>,
    pub state: SessionState,
    pub started_at: i64,
    pub last_active_at: i64,
    pub updated_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub revision: i64,
    pub metadata: serde_json::Value,
}

/// 会话生命周期错误.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLifecycleError {
    /// 不存在.
    NotFound(String),
    /// 状态转换非法 (from → to).
    IllegalTransition {
        id: String,
        from: SessionState,
        to: SessionState,
    },
    /// 乐观并发冲突: expected_rev 与当前不匹配.
    Conflict {
        id: String,
        expected: i64,
        actual: i64,
    },
    /// 输入校验失败 (空 id / 空 title / metadata 过大).
    Invalid(String),
}

impl std::fmt::Display for SessionLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "session `{id}` not found"),
            Self::IllegalTransition { id, from, to } => {
                write!(f, "session `{id}` illegal transition: {from:?} → {to:?}")
            }
            Self::Conflict { id, expected, actual } => {
                write!(f, "session `{id}` revision conflict: expected {expected}, actual {actual}")
            }
            Self::Invalid(msg) => write!(f, "invalid session: {msg}"),
        }
    }
}

impl std::error::Error for SessionLifecycleError {}

impl From<SessionLifecycleError> for MemoryError {
    fn from(e: SessionLifecycleError) -> Self {
        match e {
            SessionLifecycleError::Invalid(m) => MemoryError::Invalid(m),
            other => MemoryError::Invalid(other.to_string()),
        }
    }
}

/// 底层存储错误 (mutex poisoned / sqlite) → 包装为 Invalid (不丢失诊断信息).
impl From<MemoryError> for SessionLifecycleError {
    fn from(e: MemoryError) -> Self {
        SessionLifecycleError::Invalid(e.to_string())
    }
}

/// metadata JSON 容量上限 (防滥用; ~64KB).
const MAX_METADATA_BYTES: usize = 65_536;
/// title 长度上限.
const MAX_TITLE_LEN: usize = 200;

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn validate_id(id: &str) -> Result<(), SessionLifecycleError> {
    if id.trim().is_empty() {
        return Err(SessionLifecycleError::Invalid("session id is empty".into()));
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<(), SessionLifecycleError> {
    if title.chars().count() > MAX_TITLE_LEN {
        return Err(SessionLifecycleError::Invalid(format!(
            "title too long (max {MAX_TITLE_LEN} chars)"
        )));
    }
    Ok(())
}

fn validate_metadata(meta: &serde_json::Value) -> Result<(), SessionLifecycleError> {
    if meta.serialize_len() > MAX_METADATA_BYTES {
        return Err(SessionLifecycleError::Invalid(format!(
            "metadata too large (max {MAX_METADATA_BYTES} bytes)"
        )));
    }
    Ok(())
}

/// 仅供 validate_metadata 用: 估算序列化字节数 (不实际分配).
trait SerLen {
    fn serialize_len(&self) -> usize;
}
impl SerLen for serde_json::Value {
    fn serialize_len(&self) -> usize {
        serde_json::to_string(self).map(|s| s.len()).unwrap_or(usize::MAX)
    }
}

/// 读取单行 → record.
fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionLifecycleRecord> {
    let title: Option<String> = row.get("title")?;
    let scope_str: Option<String> = row.get("scope")?;
    let project_id: Option<String> = row.get("project_id")?;
    let state_str: Option<String> = row.get("state")?;
    let metadata_str: Option<String> = row.get("metadata_json")?;
    let metadata: serde_json::Value = metadata_str
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    Ok(SessionLifecycleRecord {
        id: row.get("id")?,
        title,
        scope: SessionScope::from_str(scope_str.as_deref().unwrap_or("global")),
        project_id,
        state: SessionState::from_str(state_str.as_deref().unwrap_or("active")),
        started_at: row.get("started_at")?,
        last_active_at: row.get("last_active_at")?,
        updated_at: row.get("updated_at")?,
        archived_at: row.get("archived_at")?,
        closed_at: row.get("closed_at")?,
        revision: row.get("revision")?,
        metadata,
    })
}

const SELECT_COLS: &str = "id, title, scope, project_id, state, started_at, last_active_at, \
     updated_at, archived_at, closed_at, revision, metadata_json";

/// Session 生命周期存储接口 (inherent impl on SqliteMemoryStore).
///
/// 命名用 `SessionStore` trait 以表达"这是会话生命周期的正式存储契约"; 与旧
/// `session_note::SessionStore` 区分 (旧 trait 只做 upsert/get/close/list 的 4 列操作).
/// 为避免同名冲突, lib.rs re-export 时用 `SessionLifecycleStore` 别名.
pub trait SessionStore {
    /// 创建新会话 (active, revision=0). 已存在同 id → Conflict (不覆盖).
    fn create_session(
        &self,
        id: &str,
        title: Option<&str>,
        scope: SessionScope,
        project_id: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<SessionLifecycleRecord, SessionLifecycleError>;

    /// 读取单会话.
    fn get_session_lifecycle(&self, id: &str) -> Result<Option<SessionLifecycleRecord>, SessionLifecycleError>;

    /// 列出会话 (默认排除 archived, 可选包含).
    fn list_sessions(&self, include_archived: bool) -> Result<Vec<SessionLifecycleRecord>, SessionLifecycleError>;

    /// 重命名 (乐观并发: expected_rev CAS).
    fn rename_session(
        &self,
        id: &str,
        new_title: &str,
        expected_rev: i64,
    ) -> Result<SessionLifecycleRecord, SessionLifecycleError>;

    /// 归档 (active → archived).
    fn archive_session(&self, id: &str, expected_rev: i64) -> Result<SessionLifecycleRecord, SessionLifecycleError>;

    /// 恢复 (archived → active).
    fn restore_session(&self, id: &str, expected_rev: i64) -> Result<SessionLifecycleRecord, SessionLifecycleError>;

    /// 关闭 (active/archived → closed, 终态).
    fn close_session_lifecycle(&self, id: &str, expected_rev: i64) -> Result<SessionLifecycleRecord, SessionLifecycleError>;
}

impl SessionStore for SqliteMemoryStore {
    fn create_session(
        &self,
        id: &str,
        title: Option<&str>,
        scope: SessionScope,
        project_id: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<SessionLifecycleRecord, SessionLifecycleError> {
        validate_id(id)?;
        if let Some(t) = title {
            validate_title(t)?;
        }
        if scope == SessionScope::Project && project_id.map_or(true, |p| p.trim().is_empty()) {
            return Err(SessionLifecycleError::Invalid(
                "project scope requires project_id".into(),
            ));
        }
        let meta = metadata.cloned().unwrap_or(serde_json::Value::Null);
        validate_metadata(&meta)?;
        let meta_json = serde_json::to_string(&meta).map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        let now = now_ms();

        let conn = self.conn()?;
        // 已存在 → Conflict (不静默覆盖).
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        if exists {
            return Err(SessionLifecycleError::Conflict {
                id: id.to_string(),
                expected: 0,
                actual: -1,
            });
        }
        conn.execute(
            "INSERT INTO sessions (id, started_at, last_active_at, closed_at, title, scope, project_id, \
             state, metadata_json, revision, archived_at, updated_at) \
             VALUES (?1, ?2, ?2, NULL, ?3, ?4, ?5, 'active', ?6, 0, NULL, ?2)",
            params![id, now, title, scope.as_str(), project_id, meta_json],
        )
        .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        drop(conn);
        self.get_session_lifecycle(id)?.ok_or(SessionLifecycleError::NotFound(id.to_string()))
    }

    fn get_session_lifecycle(&self, id: &str) -> Result<Option<SessionLifecycleRecord>, SessionLifecycleError> {
        validate_id(id)?;
        let conn = self.conn()?;
        let row = conn
            .query_row(
                &format!("SELECT {SELECT_COLS} FROM sessions WHERE id = ?1"),
                params![id],
                row_to_record,
            )
            .optional()
            .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        Ok(row)
    }

    fn list_sessions(&self, include_archived: bool) -> Result<Vec<SessionLifecycleRecord>, SessionLifecycleError> {
        let conn = self.conn()?;
        let sql = if include_archived {
            format!("SELECT {SELECT_COLS} FROM sessions ORDER BY last_active_at DESC, id ASC")
        } else {
            format!(
                "SELECT {SELECT_COLS} FROM sessions WHERE state IS NULL OR state = 'active' \
                 ORDER BY last_active_at DESC, id ASC"
            )
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_record)
            .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?);
        }
        Ok(out)
    }

    fn rename_session(
        &self,
        id: &str,
        new_title: &str,
        expected_rev: i64,
    ) -> Result<SessionLifecycleRecord, SessionLifecycleError> {
        validate_id(id)?;
        validate_title(new_title)?;
        let now = now_ms();
        let conn = self.conn()?;
        let updated = conn
            .execute(
                "UPDATE sessions SET title = ?1, revision = revision + 1, updated_at = ?2 \
                 WHERE id = ?3 AND revision = ?4",
                params![new_title, now, id, expected_rev],
            )
            .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        drop(conn);
        if updated == 0 {
            return Err(self.cas_failure(id, expected_rev));
        }
        self.get_session_lifecycle(id)?.ok_or(SessionLifecycleError::NotFound(id.to_string()))
    }

    fn archive_session(&self, id: &str, expected_rev: i64) -> Result<SessionLifecycleRecord, SessionLifecycleError> {
        self.transition(id, expected_rev, SessionState::Active, SessionState::Archived, "archived")
    }

    fn restore_session(&self, id: &str, expected_rev: i64) -> Result<SessionLifecycleRecord, SessionLifecycleError> {
        self.transition(id, expected_rev, SessionState::Archived, SessionState::Active, "active")
    }

    fn close_session_lifecycle(&self, id: &str, expected_rev: i64) -> Result<SessionLifecycleRecord, SessionLifecycleError> {
        let now = now_ms();
        let conn = self.conn()?;
        // closed 是终态: active/archived → closed (单条 CAS, 任意非 closed 源态).
        let updated = conn
            .execute(
                "UPDATE sessions SET state = 'closed', closed_at = ?1, revision = revision + 1, updated_at = ?1 \
                 WHERE id = ?2 AND revision = ?3 AND (state IS NULL OR state != 'closed')",
                params![now, id, expected_rev],
            )
            .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?;
        drop(conn);
        if updated == 0 {
            // 区分: 已 closed (非法转换) vs revision 冲突 vs 不存在.
            let rec = self.get_session_lifecycle(id)?;
            match rec {
                None => Err(SessionLifecycleError::NotFound(id.to_string())),
                Some(r) if r.state == SessionState::Closed => Err(SessionLifecycleError::IllegalTransition {
                    id: id.to_string(),
                    from: SessionState::Closed,
                    to: SessionState::Closed,
                }),
                Some(r) => Err(SessionLifecycleError::Conflict {
                    id: id.to_string(),
                    expected: expected_rev,
                    actual: r.revision,
                }),
            }
        } else {
            self.get_session_lifecycle(id)?.ok_or(SessionLifecycleError::NotFound(id.to_string()))
        }
    }
}

impl SqliteMemoryStore {
    /// 通用状态转换 (active↔archived), CAS + 非法转换检测.
    fn transition(
        &self,
        id: &str,
        expected_rev: i64,
        from: SessionState,
        to: SessionState,
        to_str: &str,
    ) -> Result<SessionLifecycleRecord, SessionLifecycleError> {
        let now = now_ms();
        let conn = self.conn()?;
        let archived_set = if to == SessionState::Archived {
            ", archived_at = ?5"
        } else {
            ""
        };
        // CAS: 必须当前状态 = from 且 revision 匹配.
        let sql = format!(
            "UPDATE sessions SET state = ?1, revision = revision + 1, updated_at = ?2{archived_set} \
             WHERE id = ?3 AND revision = ?4 AND (state IS NULL OR state = ?5_src)"
        );
        // SQLite 不支持参数化列值比较的占位符重用, 拼接 from 字符串 (固定枚举, 安全).
        let sql = sql.replace("?5_src", &format!("'{}'", from.as_str()));
        let updated = if to == SessionState::Archived {
            conn.execute(&sql, params![to_str, now, id, expected_rev, now])
                .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?
        } else {
            conn.execute(&sql, params![to_str, now, id, expected_rev])
                .map_err(|e| SessionLifecycleError::Invalid(e.to_string()))?
        };
        drop(conn);
        if updated == 0 {
            return Err(self.transition_failure(id, expected_rev, from, to));
        }
        self.get_session_lifecycle(id)?.ok_or(SessionLifecycleError::NotFound(id.to_string()))
    }

    /// CAS 失败时区分 Conflict vs NotFound vs IllegalTransition.
    fn cas_failure(&self, id: &str, expected_rev: i64) -> SessionLifecycleError {
        match self.get_session_lifecycle(id) {
            Ok(None) => SessionLifecycleError::NotFound(id.to_string()),
            Ok(Some(r)) => SessionLifecycleError::Conflict {
                id: id.to_string(),
                expected: expected_rev,
                actual: r.revision,
            },
            Err(_) => SessionLifecycleError::NotFound(id.to_string()),
        }
    }

    fn transition_failure(
        &self,
        id: &str,
        expected_rev: i64,
        from: SessionState,
        to: SessionState,
    ) -> SessionLifecycleError {
        match self.get_session_lifecycle(id) {
            Ok(None) => SessionLifecycleError::NotFound(id.to_string()),
            Ok(Some(r)) => {
                if r.state != from {
                    // 当前状态不匹配 from → 非法转换 (或已是 to).
                    SessionLifecycleError::IllegalTransition {
                        id: id.to_string(),
                        from: r.state,
                        to,
                    }
                } else {
                    SessionLifecycleError::Conflict {
                        id: id.to_string(),
                        expected: expected_rev,
                        actual: r.revision,
                    }
                }
            }
            Err(_) => SessionLifecycleError::NotFound(id.to_string()),
        }
    }
}

// rusqlite OptionalExtension (imported at top) provides .optional() on query_row.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteMemoryStore;

    fn store() -> SqliteMemoryStore {
        SqliteMemoryStore::open_in_memory().unwrap()
    }

    #[test]
    fn session_create_get_list() {
        let s = store();
        let r = s
            .create_session("s1", Some("首个会话"), SessionScope::Global, None, None)
            .unwrap();
        assert_eq!(r.state, SessionState::Active);
        assert_eq!(r.revision, 0);
        assert_eq!(r.title.as_deref(), Some("首个会话"));
        assert!(s.applied_migrations().unwrap().contains(&5));

        let got = s.get_session_lifecycle("s1").unwrap().unwrap();
        assert_eq!(got.id, "s1");

        // list 默认排除 archived
        s.create_session("s2", None, SessionScope::Global, None, None).unwrap();
        let list = s.list_sessions(false).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn session_rename_revision_increment() {
        let s = store();
        s.create_session("s1", Some("旧标题"), SessionScope::Global, None, None).unwrap();
        let r = s.rename_session("s1", "新标题", 0).unwrap();
        assert_eq!(r.title.as_deref(), Some("新标题"));
        assert_eq!(r.revision, 1);
        // 旧 rev → conflict
        let err = s.rename_session("s1", "再次", 0).unwrap_err();
        assert!(matches!(err, SessionLifecycleError::Conflict { .. }));
        // 正确 rev → ok
        let r2 = s.rename_session("s1", "再次", 1).unwrap();
        assert_eq!(r2.revision, 2);
    }

    #[test]
    fn session_archive_restore() {
        let s = store();
        s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
        let r = s.archive_session("s1", 0).unwrap();
        assert_eq!(r.state, SessionState::Archived);
        assert!(r.archived_at.is_some());
        // archived 默认不列出
        assert!(s.list_sessions(false).unwrap().is_empty());
        assert_eq!(s.list_sessions(true).unwrap().len(), 1);
        // restore
        let r2 = s.restore_session("s1", 1).unwrap();
        assert_eq!(r2.state, SessionState::Active);
        assert_eq!(s.list_sessions(false).unwrap().len(), 1);
    }

    #[test]
    fn session_close_is_terminal() {
        let s = store();
        s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
        let r = s.close_session_lifecycle("s1", 0).unwrap();
        assert_eq!(r.state, SessionState::Closed);
        assert!(r.closed_at.is_some());
        // closed 不可再 archive
        let err = s.archive_session("s1", 1).unwrap_err();
        assert!(matches!(err, SessionLifecycleError::IllegalTransition { .. }));
        // closed 不可再 close
        let err2 = s.close_session_lifecycle("s1", 1).unwrap_err();
        assert!(matches!(err2, SessionLifecycleError::IllegalTransition { .. }));
        // closed 默认不列出, 但 include_archived 时列出 (供审计)
        assert!(s.list_sessions(false).unwrap().is_empty());
        assert_eq!(s.list_sessions(true).unwrap().len(), 1);
    }

    #[test]
    fn session_invalid_transition_archived_to_archived() {
        let s = store();
        s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
        s.archive_session("s1", 0).unwrap();
        // 已 archived 再 archive → illegal (from != active)
        let err = s.archive_session("s1", 1).unwrap_err();
        assert!(matches!(err, SessionLifecycleError::IllegalTransition { .. }));
    }

    #[test]
    fn session_revision_conflict_double_archive() {
        let s = store();
        s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
        s.archive_session("s1", 0).unwrap(); // rev 0→1
                                               // 用过期 rev 再 archive → conflict (state 仍是 archived, 但 rev 不匹配)
        let err = s.archive_session("s1", 0).unwrap_err();
        // 这里 state 已是 archived (≠active) → IllegalTransition 优先于 Conflict
        assert!(matches!(err, SessionLifecycleError::IllegalTransition { .. }));
    }

    #[test]
    fn session_restart_persistence() {
        // open_in_memory 同一 store 重开 (in-memory 不跨进程, 但验证 reopen 不崩 + migration 幂等).
        let s = store();
        s.create_session("s1", Some("持久"), SessionScope::Global, None, None).unwrap();
        s.rename_session("s1", "改名", 0).unwrap();
        let got = s.get_session_lifecycle("s1").unwrap().unwrap();
        assert_eq!(got.title.as_deref(), Some("改名"));
        assert_eq!(got.revision, 1);
    }

    #[test]
    fn session_episode_relation_preserved() {
        // session 与 episode 关联: archived session 的 episode 仍可查询 (不删).
        let s = store();
        s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
        use crate::EpisodeStore;
        let ep = apeireth_core::Episode {
            id: "ep-1".into(),
            timestamp: 1000,
            role: "user".into(),
            content: "hello".into(),
            session_id: "s1".into(),
        };
        s.put_episode(&ep).unwrap();
        s.archive_session("s1", 0).unwrap();
        // episode 仍存在
        let eps = s.recent_episodes("s1", 10).unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].id, "ep-1");
    }

    #[test]
    fn session_create_duplicate_conflict() {
        let s = store();
        s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
        let err = s.create_session("s1", None, SessionScope::Global, None, None).unwrap_err();
        assert!(matches!(err, SessionLifecycleError::Conflict { .. }));
    }

    #[test]
    fn session_project_scope_requires_project_id() {
        let s = store();
        let err = s
            .create_session("s1", None, SessionScope::Project, None, None)
            .unwrap_err();
        assert!(matches!(err, SessionLifecycleError::Invalid(_)));
        // 有 project_id → ok
        s.create_session("s2", None, SessionScope::Project, Some("proj-1"), None)
            .unwrap();
    }

    #[test]
    fn session_metadata_capacity_and_persistence() {
        let s = store();
        let meta = serde_json::json!({"foo": "bar", "n": 42});
        s.create_session("s1", None, SessionScope::Global, None, Some(&meta))
            .unwrap();
        let got = s.get_session_lifecycle("s1").unwrap().unwrap();
        assert_eq!(got.metadata, meta);
        // 过大 metadata → 拒绝
        let big = serde_json::json!(vec!["x"; 100_000].join(""));
        let err = s
            .create_session("s2", None, SessionScope::Global, None, Some(&big))
            .unwrap_err();
        assert!(matches!(err, SessionLifecycleError::Invalid(_)));
    }

    #[test]
    fn session_legacy_client_compat() {
        // 旧 SessionStore trait 只写 4 列 (title/scope/state/revision = NULL/默认).
        // 生命周期读取必须兼容: NULL state → Active, NULL revision → 0.
        let s = store();
        use crate::SessionStore as LegacySessionStore;
        let old = apeireth_core::Session {
            id: "legacy-1".into(),
            started_at: 1000,
            last_active_at: 2000,
        };
        s.upsert_session(&old).unwrap();
        let got = s.get_session_lifecycle("legacy-1").unwrap().unwrap();
        assert_eq!(got.state, SessionState::Active); // NULL → Active
        assert_eq!(got.revision, 0); // NULL → 0
        assert_eq!(got.scope, SessionScope::Global); // NULL → Global
        assert!(got.title.is_none());
        // 旧 session 仍可被生命周期操作 (archive 等), revision 从 0 开始 CAS.
        let r = s.archive_session("legacy-1", 0).unwrap();
        assert_eq!(r.state, SessionState::Archived);
    }
}
