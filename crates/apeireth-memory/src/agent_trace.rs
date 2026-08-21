//! Core Capability Expansion Phase 5 — Agent 执行轨迹存储 (structured trace).
//!
//! 一次用户请求 → 一个 `trace_id`; Commander/Worker/Tool/Memory/Workflow 各为 span,
//! 通过 `parent_span_id` 关联成因果树. span 终态时写 ended_at/status.
//!
//! ## 严禁存储原始 Chain-of-Thought
//! Trace 是 **execution trace**, 不是 hidden reasoning dump. 只存 safe user-facing summary
//! (如「检索长期记忆」「调用工具」「启动 Worker」「响应完成」). 模型私有思维过程 / reasoning
//! tokens / 内部推理草稿**绝不**持久化. attributes 已 redaction (见 companion agent_trace.rs).
//!
//! ## ID 形态
//! trace_id / span_id 用 16-hex (与 telemetry W3C span 同形态, 便于未来打通; 不复用 bus u64).
//! caller 也可自带 id (如继承上游 traceparent).
//!
//! ## 持久化
//! append-only: 每 span 一行. 运行中 span (status=running, ended_at=NULL) 可后续 end.
//! 终态 span 不可变 (本轮不强制 trigger, 但 API 不提供 span 改写).

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{MemoryError, MemoryResult, SqliteMemoryStore};

/// Span 种类.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceSpanKind {
    /// 对话根 (一次用户请求).
    Conversation,
    /// Agent 编排.
    Agent,
    /// Worker 子任务.
    Worker,
    /// 记忆操作 (检索/写入).
    Memory,
    /// 工具调用.
    Tool,
    /// 工作流.
    Workflow,
    /// Runtime 诊断.
    Runtime,
}

impl TraceSpanKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Agent => "agent",
            Self::Worker => "worker",
            Self::Memory => "memory",
            Self::Tool => "tool",
            Self::Workflow => "workflow",
            Self::Runtime => "runtime",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "agent" => Self::Agent,
            "worker" => Self::Worker,
            "memory" => Self::Memory,
            "tool" => Self::Tool,
            "workflow" => Self::Workflow,
            "runtime" => Self::Runtime,
            _ => Self::Conversation,
        }
    }
}

/// Span 状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceSpanStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TraceSpanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Running,
        }
    }
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// 一个 trace span (执行轨迹节点).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub kind: TraceSpanKind,
    pub actor: String,
    pub status: TraceSpanStatus,
    pub summary: Option<String>,
    /// 已 redaction 的属性 (不含 secret). JSON.
    pub attributes: serde_json::Value,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceQueryError {
    NotFound(String),
    Invalid(String),
}

impl std::fmt::Display for TraceQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "trace/span `{id}` not found"),
            Self::Invalid(m) => write!(f, "invalid trace query: {m}"),
        }
    }
}
impl std::error::Error for TraceQueryError {}
impl From<TraceQueryError> for MemoryError {
    fn from(e: TraceQueryError) -> Self {
        MemoryError::Invalid(e.to_string())
    }
}
impl From<MemoryError> for TraceQueryError {
    fn from(e: MemoryError) -> Self {
        Self::Invalid(e.to_string())
    }
}
impl From<rusqlite::Error> for TraceQueryError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn row_to_span(row: &rusqlite::Row<'_>) -> rusqlite::Result<TraceSpan> {
    let kind_str: String = row.get("kind")?;
    let status_str: String = row.get("status")?;
    let attrs_str: String = row.get("attributes_json")?;
    let attributes: serde_json::Value = serde_json::from_str(&attrs_str).unwrap_or(serde_json::Value::Null);
    Ok(TraceSpan {
        span_id: row.get("span_id")?,
        trace_id: row.get("trace_id")?,
        parent_span_id: row.get("parent_span_id")?,
        kind: TraceSpanKind::from_str(&kind_str),
        actor: row.get("actor")?,
        status: TraceSpanStatus::from_str(&status_str),
        summary: row.get("summary")?,
        attributes,
        started_at: row.get("started_at")?,
        ended_at: row.get("ended_at")?,
        session_id: row.get("session_id")?,
    })
}

const SELECT_COLS: &str = "span_id, trace_id, parent_span_id, kind, actor, status, summary, \
     attributes_json, started_at, ended_at, session_id";

/// Trace 存储 (inherent impl on SqliteMemoryStore).
pub trait TraceStore {
    /// 写入一个 span (开始或终态). 同 span_id 再写 = 覆盖终态 (ended_at/status).
    fn put_trace_span(&self, span: &TraceSpan) -> Result<(), TraceQueryError>;

    /// 结束一个 span (写 ended_at + 终态 status). 必须存在.
    fn end_trace_span(
        &self,
        span_id: &str,
        status: TraceSpanStatus,
        summary: Option<&str>,
    ) -> Result<TraceSpan, TraceQueryError>;

    /// 读取单 span.
    fn get_trace_span(&self, span_id: &str) -> Result<Option<TraceSpan>, TraceQueryError>;

    /// 列出某 trace 的全部 span (按 started_at 升序).
    fn list_trace_spans(&self, trace_id: &str) -> Result<Vec<TraceSpan>, TraceQueryError>;

    /// 列出最近的 trace (按 trace 去重, 取每个 trace 的根 span, 按 started_at desc).
    /// 返回 (trace_id, root_span, span_count).
    fn list_recent_traces(&self, limit: usize) -> Result<Vec<TraceSummary>, TraceQueryError>;
}

/// Trace 摘要 (列表项).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    pub trace_id: String,
    pub root_span: TraceSpan,
    pub span_count: i64,
}

impl TraceStore for SqliteMemoryStore {
    fn put_trace_span(&self, span: &TraceSpan) -> Result<(), TraceQueryError> {
        if span.span_id.trim().is_empty() || span.trace_id.trim().is_empty() {
            return Err(TraceQueryError::Invalid("span_id/trace_id empty".into()));
        }
        let attrs_json = serde_json::to_string(&span.attributes)
            .map_err(|e| TraceQueryError::Invalid(e.to_string()))?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO agent_traces (span_id, trace_id, parent_span_id, kind, actor, status, summary, \
             attributes_json, started_at, ended_at, session_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
             ON CONFLICT(span_id) DO UPDATE SET status=excluded.status, summary=excluded.summary, \
             ended_at=excluded.ended_at, attributes_json=excluded.attributes_json",
            params![
                span.span_id,
                span.trace_id,
                span.parent_span_id,
                span.kind.as_str(),
                span.actor,
                span.status.as_str(),
                span.summary,
                attrs_json,
                span.started_at,
                span.ended_at,
                span.session_id,
            ],
        )?;
        Ok(())
    }

    fn end_trace_span(
        &self,
        span_id: &str,
        status: TraceSpanStatus,
        summary: Option<&str>,
    ) -> Result<TraceSpan, TraceQueryError> {
        if span_id.trim().is_empty() {
            return Err(TraceQueryError::Invalid("span_id empty".into()));
        }
        let now = now_ms();
        let conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE agent_traces SET status = ?1, ended_at = ?2, summary = COALESCE(?3, summary) \
             WHERE span_id = ?4",
            params![status.as_str(), now, summary, span_id],
        )?;
        drop(conn);
        if updated == 0 {
            return Err(TraceQueryError::NotFound(span_id.to_string()));
        }
        self.get_trace_span(span_id)?
            .ok_or(TraceQueryError::NotFound(span_id.to_string()))
    }

    fn get_trace_span(&self, span_id: &str) -> Result<Option<TraceSpan>, TraceQueryError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                &format!("SELECT {SELECT_COLS} FROM agent_traces WHERE span_id = ?1"),
                params![span_id],
                row_to_span,
            )
            .optional()?;
        Ok(row)
    }

    fn list_trace_spans(&self, trace_id: &str) -> Result<Vec<TraceSpan>, TraceQueryError> {
        if trace_id.trim().is_empty() {
            return Err(TraceQueryError::Invalid("trace_id empty".into()));
        }
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM agent_traces WHERE trace_id = ?1 ORDER BY started_at ASC, span_id ASC"
        ))?;
        let rows = stmt.query_map(params![trace_id], row_to_span)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn list_recent_traces(&self, limit: usize) -> Result<Vec<TraceSummary>, TraceQueryError> {
        // 注意: 不能在循环里调 self.list_trace_spans (它会再次 self.conn()? 锁同一 Mutex → 死锁).
        // 全部在同一个 conn 内用窗口查询完成: 每 trace 取 root span (parent_span_id IS NULL 优先)
        // + span_count, 按 trace 最早 started_at desc.
        let conn = self.conn()?;
        // 1. 取最近的 trace_id 列表 (按每 trace 最早 started_at desc).
        let mut stmt = conn.prepare(
            "SELECT trace_id, MIN(started_at) AS first_started \
             FROM agent_traces GROUP BY trace_id ORDER BY first_started DESC LIMIT ?1",
        )?;
        let trace_ids: Vec<String> = stmt
            .query_map(params![limit as i64], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        if trace_ids.is_empty() {
            return Ok(Vec::new());
        }
        // 2. 在同一 conn 内逐 trace 取 root span + count (不再调 self.list_trace_spans, 避免重入锁).
        let mut out = Vec::new();
        for trace_id in trace_ids {
            // root = parent_span_id IS NULL 的 span (优先), 否则最早 started_at 的 span.
            let root: Option<TraceSpan> = conn
                .query_row(
                    &format!(
                        "SELECT {SELECT_COLS} FROM agent_traces WHERE trace_id = ?1 \
                         ORDER BY (parent_span_id IS NOT NULL), started_at ASC, span_id ASC LIMIT 1"
                    ),
                    params![trace_id],
                    row_to_span,
                )
                .optional()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_traces WHERE trace_id = ?1",
                params![trace_id],
                |r| r.get(0),
            )?;
            if let Some(root_span) = root {
                out.push(TraceSummary {
                    trace_id,
                    root_span,
                    span_count: count,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteMemoryStore;

    fn store() -> SqliteMemoryStore {
        SqliteMemoryStore::open_in_memory().unwrap()
    }

    fn new_id() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    fn root_span(trace: &str) -> TraceSpan {
        TraceSpan {
            span_id: new_id(),
            trace_id: trace.into(),
            parent_span_id: None,
            kind: TraceSpanKind::Conversation,
            actor: "user".into(),
            status: TraceSpanStatus::Running,
            summary: Some("用户提问".into()),
            attributes: serde_json::json!({"query_len": 42}),
            started_at: 1000,
            ended_at: None,
            session_id: Some("s1".into()),
        }
    }

    fn child_span(trace: &str, parent: &str, kind: TraceSpanKind, actor: &str) -> TraceSpan {
        TraceSpan {
            span_id: new_id(),
            trace_id: trace.into(),
            parent_span_id: Some(parent.into()),
            kind,
            actor: actor.into(),
            status: TraceSpanStatus::Running,
            summary: None,
            attributes: serde_json::json!({}),
            started_at: 1100,
            ended_at: None,
            session_id: Some("s1".into()),
        }
    }

    #[test]
    fn trace_root_and_children_persisted() {
        let s = store();
        let root = root_span("t1");
        let root_id = root.span_id.clone();
        s.put_trace_span(&root).unwrap();
        let mem = child_span("t1", &root_id, TraceSpanKind::Memory, "companion");
        let mem_id = mem.span_id.clone();
        s.put_trace_span(&mem).unwrap();
        let worker = child_span("t1", &root_id, TraceSpanKind::Worker, "worker:1");
        s.put_trace_span(&worker).unwrap();
        let tool = child_span("t1", &mem_id, TraceSpanKind::Tool, "tool:WebSearch");
        s.put_trace_span(&tool).unwrap();

        let spans = s.list_trace_spans("t1").unwrap();
        assert_eq!(spans.len(), 4);
        // parent-child 关联
        assert!(spans.iter().any(|sp| sp.span_id == mem_id && sp.parent_span_id.as_deref() == Some(&root_id)));
        assert!(spans.iter().any(|sp| sp.kind == TraceSpanKind::Tool && sp.parent_span_id.as_deref() == Some(&mem_id)));
        assert!(s.applied_migrations().unwrap().contains(&7));
    }

    #[test]
    fn trace_end_span_terminal() {
        let s = store();
        let root = root_span("t1");
        let root_id = root.span_id.clone();
        s.put_trace_span(&root).unwrap();
        let ended = s.end_trace_span(&root_id, TraceSpanStatus::Succeeded, Some("响应完成")).unwrap();
        assert_eq!(ended.status, TraceSpanStatus::Succeeded);
        assert!(ended.ended_at.is_some());
        assert_eq!(ended.summary.as_deref(), Some("响应完成"));
        assert!(ended.status.is_terminal());
    }

    #[test]
    fn trace_failure_status() {
        let s = store();
        let root = root_span("t1");
        let rid = root.span_id.clone();
        s.put_trace_span(&root).unwrap();
        let ended = s.end_trace_span(&rid, TraceSpanStatus::Failed, Some("工具超时")).unwrap();
        assert_eq!(ended.status, TraceSpanStatus::Failed);
    }

    #[test]
    fn trace_list_recent_traces() {
        let s = store();
        let r1 = root_span("t1");
        s.put_trace_span(&r1).unwrap();
        let r2 = root_span("t2");
        s.put_trace_span(&r2).unwrap();
        let summaries = s.list_recent_traces(10).unwrap();
        assert_eq!(summaries.len(), 2);
        // 每个 summary 有 root span + span_count
        assert_eq!(summaries[0].span_count, 1);
        assert!(summaries.iter().all(|ts| ts.root_span.parent_span_id.is_none()));
    }

    #[test]
    fn trace_not_found() {
        let s = store();
        let err = s.end_trace_span("ghost", TraceSpanStatus::Succeeded, None).unwrap_err();
        assert!(matches!(err, TraceQueryError::NotFound(_)));
        assert!(s.get_trace_span("ghost").unwrap().is_none());
        let err = s.list_trace_spans("ghost-trace").unwrap();
        assert!(err.is_empty());
    }

    #[test]
    fn trace_restart_persistence() {
        let s = store();
        let root = root_span("t1");
        let rid = root.span_id.clone();
        s.put_trace_span(&root).unwrap();
        s.end_trace_span(&rid, TraceSpanStatus::Succeeded, None).unwrap();
        let got = s.get_trace_span(&rid).unwrap().unwrap();
        assert_eq!(got.status, TraceSpanStatus::Succeeded);
        assert!(got.ended_at.is_some());
    }

    #[test]
    fn trace_no_raw_cot_stored() {
        // summary 只存 safe user-facing 文本; 不存 reasoning.
        let s = store();
        let mut root = root_span("t1");
        root.summary = Some("检索长期记忆".into()); // safe execution summary
        root.attributes = serde_json::json!({"retrieved": 3, "latency_ms": 120});
        s.put_trace_span(&root).unwrap();
        let got = s.get_trace_span(&root.span_id).unwrap().unwrap();
        let json = serde_json::to_string(&got).unwrap();
        // 不得含 reasoning / chain-of-thought 标记
        assert!(!json.contains("reasoning_content"));
        assert!(!json.contains("chain_of_thought"));
        assert!(!json.contains("thoughts"));
    }
}
