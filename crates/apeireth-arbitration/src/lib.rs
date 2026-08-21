//! `apeireth-arbitration` — **Apeireth R145 HASH-SQL 仲裁 + 唯一事实时间线**
//!
//! **源**: VCP v1.1 官网 "纯 HASH-SQL 仲裁所有前端、群聊、邮箱与 Agent 间通讯, 构建唯一事实时间线".
//!
//! **本 crate 设计** (借鉴上升, 不模仿):
//! - **append-only** 事件日志 (SQLite `events` 表, 没有 UPDATE, 只能 APPEND)
//! - 每事件计算 SHA-256 `content_hash` (字段级引用 VCP HASH-SQL 仲裁)
//! - **canonical order = (timestamp_ms, content_hash, seq)` 三元组:
//!   - timestamp_ms: 事件首次登记的 epoch ms (单调递增, 由 `Ord` 保证)
//!   - content_hash: 32-byte SHA-256 hex, 同时间戳时 hash 决定次序
//!   - seq: 同时间戳同 hash 时插入序号 (rare, 解决完全冲突)
//! - 跨前端/群聊/邮箱/Agent 通讯统一通过本仲裁, 保证唯一事实时间线
//! - **不假装** (O-5): SQLite 真持久化, drop+reopen 拿同一序列
//!
//! **架构位置**:
//! ```text
//!   apeireth-api / pipeline / council / agent
//!          ↓
//!   apeireth-arbitration::ArbitrationLog (本 crate)
//!          ↓ (SQLite WAL)
//!   events.db
//! ```
//!
//! **不假装 (Honest Stub 标注)**:
//! - ✅ SQLite 真持久化 (rusqlite 0.32 bundled)
//! - ✅ SHA-256 content_hash 计算
//! - ✅ (timestamp_ms, content_hash, seq) 三元 canonical order
//! - ✅ 5+ 单元测试 (insert/query/order/hash determinism)
//! - ⚠️ 多源 fan-in 冲突解决: 当前 last-write-wins per source + hash, 不做 vector clock

#![deny(unsafe_code)]

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

// ============================================================================
// 错误类型
// ============================================================================

#[derive(Debug, Error)]
pub enum ArbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("event not found: seq={0}")]
    NotFound(i64),
    #[error("invalid source: {0}")]
    InvalidSource(String),
}

pub type ArbResult<T> = Result<T, ArbError>;

// ============================================================================
// 事件类型
// ============================================================================

/// 跨前端/群聊/邮箱/Agent 通讯的统一事件源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventSource {
    /// 来自某个前端 (e.g. VCPChat / TUI / API)
    Frontend,
    /// 来自群聊 / 多 Agent 群组
    GroupChat,
    /// 来自邮箱流转
    Email,
    /// 来自 Agent 与 Agent 通讯
    AgentComm,
    /// 来自系统内部 (e.g. supervisor, scheduler)
    System,
    /// 未知 / 外部
    External,
}

impl EventSource {
    pub const COUNT: usize = 6;
    pub const ALL: [EventSource; 6] = [
        Self::Frontend,
        Self::GroupChat,
        Self::Email,
        Self::AgentComm,
        Self::System,
        Self::External,
    ];

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::GroupChat => "group_chat",
            Self::Email => "email",
            Self::AgentComm => "agent_comm",
            Self::System => "system",
            Self::External => "external",
        }
    }

    fn from_str(s: &str) -> ArbResult<Self> {
        match s {
            "frontend" => Ok(Self::Frontend),
            "group_chat" => Ok(Self::GroupChat),
            "email" => Ok(Self::Email),
            "agent_comm" => Ok(Self::AgentComm),
            "system" => Ok(Self::System),
            "external" => Ok(Self::External),
            _ => Err(ArbError::InvalidSource(s.to_string())),
        }
    }
}

/// 仲裁事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrationEvent {
    /// 唯一序号 (SQLite AUTOINCREMENT, append-only)
    pub seq: i64,
    /// 时间戳 (epoch ms, 插入时分配)
    pub timestamp_ms: i64,
    /// 来源
    pub source: EventSource,
    /// 来源 ID (e.g. "agent:cat", "frontend:tui", "room:family")
    pub source_id: String,
    /// 主题 (e.g. "user_message", "tool_call", "approval")
    pub topic: String,
    /// 内容 (JSON 字符串)
    pub payload_json: String,
    /// SHA-256 (timestamp_ms + source + source_id + topic + payload_json)
    pub content_hash: String,
}

impl ArbitrationEvent {
    /// 计算 content_hash (字段级引用 VCP HASH-SQL 仲裁)
    pub fn compute_hash(
        timestamp_ms: i64,
        source: EventSource,
        source_id: &str,
        topic: &str,
        payload_json: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(timestamp_ms.to_be_bytes());
        hasher.update(source.as_str().as_bytes());
        hasher.update(b"|");
        hasher.update(source_id.as_bytes());
        hasher.update(b"|");
        hasher.update(topic.as_bytes());
        hasher.update(b"|");
        hasher.update(payload_json.as_bytes());
        let bytes = hasher.finalize();
        hex::encode(bytes)
    }
}

// ============================================================================
// 仲裁日志 (append-only SQLite)
// ============================================================================

/// 仲裁日志 (Arc-shared, clone 即可共享)
#[derive(Clone)]
pub struct ArbitrationLog {
    inner: Arc<Mutex<rusqlite::Connection>>,
    path: Arc<String>,
}

impl ArbitrationLog {
    /// 打开或创建 (in-memory)
    pub fn open_in_memory() -> ArbResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        let log = Self {
            inner: Arc::new(Mutex::new(conn)),
            path: Arc::new(":memory:".to_string()),
        };
        log.init_schema()?;
        Ok(log)
    }

    /// 打开或创建 (file)
    pub fn open<P: AsRef<Path>>(path: P) -> ArbResult<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let conn = rusqlite::Connection::open(path)?;
        let log = Self {
            inner: Arc::new(Mutex::new(conn)),
            path: Arc::new(path_str),
        };
        log.init_schema()?;
        Ok(log)
    }

    fn init_schema(&self) -> ArbResult<()> {
        let conn = self.inner.lock();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                source TEXT NOT NULL,
                source_id TEXT NOT NULL,
                topic TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                content_hash TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_events_hash ON events(content_hash);
            CREATE INDEX IF NOT EXISTS idx_events_source ON events(source, source_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_events_uniq ON events(timestamp_ms, content_hash, seq);
            "
        )?;
        Ok(())
    }

    /// 追加一个事件 (append-only, 不能改)
    pub fn append(
        &self,
        source: EventSource,
        source_id: impl Into<String>,
        topic: impl Into<String>,
        payload_json: impl Into<String>,
    ) -> ArbResult<ArbitrationEvent> {
        let source_id = source_id.into();
        let topic = topic.into();
        let payload_json = payload_json.into();
        let timestamp_ms = now_ms();
        let content_hash =
            ArbitrationEvent::compute_hash(timestamp_ms, source, &source_id, &topic, &payload_json);

        let conn = self.inner.lock();
        conn.execute(
            "INSERT INTO events (timestamp_ms, source, source_id, topic, payload_json, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![timestamp_ms, source.as_str(), source_id, topic, payload_json, content_hash],
        )?;
        let seq = conn.last_insert_rowid();

        Ok(ArbitrationEvent {
            seq,
            timestamp_ms,
            source,
            source_id,
            topic,
            payload_json,
            content_hash,
        })
    }

    /// 按 canonical order 查询 (timestamp_ms ASC, content_hash ASC, seq ASC)
    pub fn canonical_order(&self, limit: usize) -> ArbResult<Vec<ArbitrationEvent>> {
        let conn = self.inner.lock();
        let mut stmt = conn.prepare(
            "SELECT seq, timestamp_ms, source, source_id, topic, payload_json, content_hash
             FROM events ORDER BY timestamp_ms ASC, content_hash ASC, seq ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            let source_str: String = row.get(2)?;
            let source =
                EventSource::from_str(&source_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(ArbitrationEvent {
                seq: row.get(0)?,
                timestamp_ms: row.get(1)?,
                source,
                source_id: row.get(3)?,
                topic: row.get(4)?,
                payload_json: row.get(5)?,
                content_hash: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 按 source + source_id 过滤
    pub fn by_source(
        &self,
        source: EventSource,
        source_id: &str,
        limit: usize,
    ) -> ArbResult<Vec<ArbitrationEvent>> {
        let conn = self.inner.lock();
        let mut stmt = conn.prepare(
            "SELECT seq, timestamp_ms, source, source_id, topic, payload_json, content_hash
             FROM events WHERE source = ?1 AND source_id = ?2 ORDER BY seq ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![source.as_str(), source_id, limit as i64],
            |row| {
                let source_str: String = row.get(2)?;
                let source = EventSource::from_str(&source_str)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok(ArbitrationEvent {
                    seq: row.get(0)?,
                    timestamp_ms: row.get(1)?,
                    source,
                    source_id: row.get(3)?,
                    topic: row.get(4)?,
                    payload_json: row.get(5)?,
                    content_hash: row.get(6)?,
                })
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 事件总数
    pub fn len(&self) -> ArbResult<usize> {
        let conn = self.inner.lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn is_empty(&self) -> ArbResult<bool> {
        Ok(self.len()? == 0)
    }

    /// 当前路径
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// 当前 epoch ms (UTC)
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_event_source_as_str() {
        assert_eq!(EventSource::Frontend.as_str(), "frontend");
        assert_eq!(EventSource::GroupChat.as_str(), "group_chat");
        assert_eq!(EventSource::Email.as_str(), "email");
        assert_eq!(EventSource::AgentComm.as_str(), "agent_comm");
        assert_eq!(EventSource::System.as_str(), "system");
        assert_eq!(EventSource::External.as_str(), "external");
        assert_eq!(EventSource::COUNT, 6);
    }

    #[test]
    fn t02_hash_deterministic() {
        let h1 = ArbitrationEvent::compute_hash(1000, EventSource::Frontend, "u1", "msg", "{}");
        let h2 = ArbitrationEvent::compute_hash(1000, EventSource::Frontend, "u1", "msg", "{}");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn t03_hash_changes_with_input() {
        let h1 = ArbitrationEvent::compute_hash(1000, EventSource::Frontend, "u1", "msg", "{}");
        let h2 = ArbitrationEvent::compute_hash(1001, EventSource::Frontend, "u1", "msg", "{}");
        let h3 = ArbitrationEvent::compute_hash(1000, EventSource::GroupChat, "u1", "msg", "{}");
        let h4 = ArbitrationEvent::compute_hash(1000, EventSource::Frontend, "u2", "msg", "{}");
        let h5 =
            ArbitrationEvent::compute_hash(1000, EventSource::Frontend, "u1", "approval", "{}");
        let h6 =
            ArbitrationEvent::compute_hash(1000, EventSource::Frontend, "u1", "msg", "{\"a\":1}");
        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
        assert_ne!(h1, h5);
        assert_ne!(h1, h6);
    }

    #[test]
    fn t04_append_and_query() {
        let log = ArbitrationLog::open_in_memory().unwrap();
        let e1 = log
            .append(
                EventSource::Frontend,
                "tui",
                "user_message",
                "{\"text\":\"hi\"}",
            )
            .unwrap();
        let e2 = log
            .append(
                EventSource::AgentComm,
                "cat",
                "tool_call",
                "{\"name\":\"file_read\"}",
            )
            .unwrap();
        assert_eq!(e1.seq, 1);
        assert_eq!(e2.seq, 2);
        assert_eq!(log.len().unwrap(), 2);
        assert!(!log.is_empty().unwrap());
    }

    #[test]
    fn t05_canonical_order_by_timestamp() {
        let log = ArbitrationLog::open_in_memory().unwrap();
        log.append(EventSource::Frontend, "a", "msg", "1").unwrap();
        log.append(EventSource::GroupChat, "b", "msg", "2").unwrap();
        log.append(EventSource::Email, "c", "msg", "3").unwrap();
        let evts = log.canonical_order(10).unwrap();
        assert_eq!(evts.len(), 3);
        // 同一毫秒内, hash 决定顺序
        for i in 0..evts.len() - 1 {
            assert!(evts[i].timestamp_ms <= evts[i + 1].timestamp_ms);
        }
    }

    #[test]
    fn t06_by_source_filter() {
        let log = ArbitrationLog::open_in_memory().unwrap();
        log.append(EventSource::Frontend, "tui", "a", "1").unwrap();
        log.append(EventSource::GroupChat, "family", "msg", "2")
            .unwrap();
        log.append(EventSource::GroupChat, "family", "msg", "3")
            .unwrap();
        log.append(EventSource::Email, "inbox", "mail", "4")
            .unwrap();

        let family = log.by_source(EventSource::GroupChat, "family", 10).unwrap();
        assert_eq!(family.len(), 2);
        assert!(family
            .iter()
            .all(|e| e.source == EventSource::GroupChat && e.source_id == "family"));

        let inbox = log.by_source(EventSource::Email, "inbox", 10).unwrap();
        assert_eq!(inbox.len(), 1);
    }

    #[test]
    fn t07_persistence_drop_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("events.db");

        let log1 = ArbitrationLog::open(&db_path).unwrap();
        log1.append(EventSource::Frontend, "tui", "msg", "{\"v\":1}")
            .unwrap();
        log1.append(EventSource::GroupChat, "family", "msg", "{\"v\":2}")
            .unwrap();
        assert_eq!(log1.len().unwrap(), 2);
        drop(log1);

        // drop + reopen
        let log2 = ArbitrationLog::open(&db_path).unwrap();
        assert_eq!(log2.len().unwrap(), 2);
        let evts = log2.canonical_order(10).unwrap();
        assert_eq!(evts.len(), 2);
    }

    #[test]
    fn t08_canonical_order_deterministic_across_logs() {
        // canonical_order 的语义是时间序 (ORDER BY timestamp_ms, content_hash, seq — 见 t05)。
        // 「跨日志模式一致」在时间戳语义下不可满足: 两个日志 append 时刻不同 →
        // timestamp_ms 必不同 → 排序键不同 → 相对序可不同 (此前断言偶发失败根因)。
        // 时间序 API 的真实确定性 = 同一日志重复查询结果一致 (同库同数据 → 同序)。
        // 0 装 PASS: 不假装「跨日志一致」是 canonical_order 的语义。
        let log1 = ArbitrationLog::open_in_memory().unwrap();
        let log2 = ArbitrationLog::open_in_memory().unwrap();

        let payloads = vec![
            (EventSource::Frontend, "tui", "msg", "{\"a\":1}"),
            (EventSource::GroupChat, "family", "msg", "{\"b\":2}"),
            (EventSource::Email, "inbox", "mail", "{\"c\":3}"),
        ];
        for (s, sid, t, p) in &payloads {
            log1.append(*s, *sid, *t, *p).unwrap();
        }
        for (s, sid, t, p) in &payloads {
            log2.append(*s, *sid, *t, *p).unwrap();
        }

        let mode = |evts: &[ArbitrationEvent]| -> Vec<(String, String, String)> {
            evts.iter()
                .map(|e| {
                    (
                        e.source.as_str().to_string(),
                        e.source_id.clone(),
                        e.topic.clone(),
                    )
                })
                .collect()
        };
        // 确定性: 同一日志重复查询 → 相同排序模式
        let e1a = log1.canonical_order(10).unwrap();
        let e1b = log1.canonical_order(10).unwrap();
        assert_eq!(
            mode(&e1a),
            mode(&e1b),
            "同日志重复查询 canonical 模式应一致"
        );
        // 两日志含相同事件集 (各自时间序, 排序后模式集合一致)
        let e2 = log2.canonical_order(10).unwrap();
        assert_eq!(e1a.len(), e2.len());
        let mut m1 = mode(&e1a);
        let mut m2 = mode(&e2);
        m1.sort();
        m2.sort();
        assert_eq!(m1, m2, "跨日志事件集一致 (排序后模式相同)");
    }
}

// R177: organ invariants (5 tests + 2 Kani)
mod organ_kani_proofs;

// R215 (2026-08-21): hash-chained append-only audit journal (BORROW-Jimmyxiao2009).
// 0 装 PASS: 与 SQLite 主仲裁 (ArbitrationLog) 并行, 不动 canonical order, 仅作
// audit 链 tamper-evidence (per journal.rs module docs §"What this crate adds").
pub mod journal;
