//! `apeireth-companion::agent_trace` — Agent 执行轨迹记录器 (Phase 5).
//!
//! 职责:
//! - 生成 trace_id / span_id (16-hex, 与 telemetry W3C span 同形态).
//! - 创建根 span / 子 span, 终态时 end.
//! - 持久化到 `agent_traces` 表 (via `TraceStore`).
//! - 通过 broadcast SSE 推送 span 事件 (兼容现有 /v1/apeireth/events).
//! - **redaction**: attributes 中的 secret 一律过滤 (api_key/token/password/authorization/bearer/secret).
//!
//! ## 严禁存储原始 Chain-of-Thought
//! 只记录 safe user-facing summary. 调用方传入的 summary 若含 reasoning 标记会被拒绝/截断.
//! attributes 是执行事实 (工具名/耗时/计数), 不是模型思维.

use std::sync::Arc;

use apeireth_memory::{SqliteMemoryStore, TraceSpan, TraceSpanKind, TraceSpanStatus, TraceStore};
use serde_json::{json, Value};

/// 需要脱敏的属性键 (大小写不敏感子串匹配).
const SENSITIVE_KEY_MARKERS: &[&str] = &[
    "api_key",
    "apikey",
    "master_token",
    "mastertoken",
    "authorization",
    "bearer",
    "password",
    "passwd",
    "secret",
    "credential",
    "token",
    "cookie",
    "set-cookie",
];

/// 需要脱敏的值前缀 (高置信度 secret 模式).
const SENSITIVE_VALUE_PREFIXES: &[&str] = &["sk-", "ghp_", "gho_", "glpat-", "Bearer "];

/// 对一个 JSON value 做递归脱敏 (按 key 子串 + 值前缀). 返回脱敏后的副本.
pub fn redact_attributes(attrs: &Value) -> Value {
    match attrs {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if is_sensitive_key(k) {
                    out.insert(k.clone(), Value::String("[REDACTED]".into()));
                } else {
                    out.insert(k.clone(), redact_attributes(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_attributes).collect()),
        Value::String(s) => {
            if SENSITIVE_VALUE_PREFIXES.iter().any(|p| s.starts_with(p)) {
                Value::String("[REDACTED]".into())
            } else {
                Value::String(s.clone())
            }
        }
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SENSITIVE_KEY_MARKERS.iter().any(|m| lower.contains(m))
}

/// 检查 summary 是否含原始 reasoning 标记 (防误存 CoT).
const COT_MARKERS: &[&str] = &["reasoning_content", "chain_of_thought", "<thought>", "thinking"];
pub fn summary_is_safe(summary: &str) -> bool {
    let lower = summary.to_lowercase();
    !COT_MARKERS.iter().any(|m| lower.contains(m))
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// 轨迹记录器: 持有 store + broadcast sender (SSE).
pub struct TraceRecorder {
    store: Arc<SqliteMemoryStore>,
    events: tokio::sync::broadcast::Sender<String>,
}

impl TraceRecorder {
    pub fn new(store: Arc<SqliteMemoryStore>, events: tokio::sync::broadcast::Sender<String>) -> Self {
        Self { store, events }
    }

    /// 创建根 span (conversation). 返回 span_id (= 当前 trace 的根).
    pub fn start_root(
        &self,
        kind: TraceSpanKind,
        actor: &str,
        summary: Option<&str>,
        session_id: Option<&str>,
        attributes: Option<Value>,
    ) -> Result<TraceSpan, String> {
        let trace_id = new_id();
        self.start_span(&trace_id, None, kind, actor, summary, session_id, attributes)
    }

    /// 创建子 span. trace_id 继承父; parent_span_id 关联.
    pub fn start_child(
        &self,
        trace_id: &str,
        parent_span_id: &str,
        kind: TraceSpanKind,
        actor: &str,
        summary: Option<&str>,
        session_id: Option<&str>,
        attributes: Option<Value>,
    ) -> Result<TraceSpan, String> {
        self.start_span(trace_id, Some(parent_span_id), kind, actor, summary, session_id, attributes)
    }

    fn start_span(
        &self,
        trace_id: &str,
        parent_span_id: Option<&str>,
        kind: TraceSpanKind,
        actor: &str,
        summary: Option<&str>,
        session_id: Option<&str>,
        attributes: Option<Value>,
    ) -> Result<TraceSpan, String> {
        // summary 安全检查 (拒绝 CoT)
        let summary = summary.and_then(|s| {
            if summary_is_safe(s) {
                Some(s.to_string())
            } else {
                // 含 reasoning 标记 → 截断为 safe 占位 (不存储 CoT)
                Some("[execution step]".to_string())
            }
        });
        let attrs = redact_attributes(&attributes.unwrap_or(Value::Null));
        let span = TraceSpan {
            span_id: new_id(),
            trace_id: trace_id.to_string(),
            parent_span_id: parent_span_id.map(|s| s.to_string()),
            kind,
            actor: actor.to_string(),
            status: TraceSpanStatus::Running,
            summary,
            attributes: attrs,
            started_at: now_ms(),
            ended_at: None,
            session_id: session_id.map(|s| s.to_string()),
        };
        self.store.put_trace_span(&span).map_err(|e| e.to_string())?;
        // SSE 推送 (best-effort; 无订阅者忽略)
        let _ = self.events.send(self.span_event_json(&span, "span_start"));
        Ok(span)
    }

    /// 结束 span (终态). 推送 SSE.
    pub fn end_span(
        &self,
        span_id: &str,
        status: TraceSpanStatus,
        summary: Option<&str>,
    ) -> Result<TraceSpan, String> {
        let safe_summary = summary.and_then(|s| {
            if summary_is_safe(s) {
                Some(s.to_string())
            } else {
                Some("[execution step]".to_string())
            }
        });
        let span = self
            .store
            .end_trace_span(span_id, status, safe_summary.as_deref())
            .map_err(|e| e.to_string())?;
        let _ = self.events.send(self.span_event_json(&span, "span_end"));
        Ok(span)
    }

    fn span_event_json(&self, span: &TraceSpan, event: &str) -> String {
        // 兼容现有 ActivityView SSE 解析 (id/type/action/tool/summary/detail/status/ts).
        serde_json::to_string(&json!({
            "id": span.span_id,
            "type": "trace",
            "action": event,
            "trace_id": span.trace_id,
            "span_id": span.span_id,
            "parent_span_id": span.parent_span_id,
            "kind": span.kind.as_str(),
            "actor": span.actor,
            "summary": span.summary.clone().unwrap_or_else(|| format!("{} {}", span.kind.as_str(), span.actor)),
            "status": span.status.as_str(),
            "ts": span.started_at,
        }))
        .unwrap_or_else(|_| "{}".into())
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Arc<SqliteMemoryStore> {
        Arc::new(SqliteMemoryStore::open_in_memory().unwrap())
    }

    fn recorder() -> (TraceRecorder, Arc<SqliteMemoryStore>) {
        let s = store();
        let (tx, _rx) = tokio::sync::broadcast::channel::<String>(64);
        (TraceRecorder::new(Arc::clone(&s), tx), s)
    }

    #[test]
    fn redact_attributes_strips_secrets() {
        let attrs = json!({
            "tool": "WebSearch",
            "api_key": "sk-SECRET123",
            "Authorization": "Bearer SECRET",
            "nested": {"password": "hunter2", "ok": "fine"},
            "arr": ["sk-leaked", "normal"],
            "count": 5
        });
        let redacted = redact_attributes(&attrs);
        let s = serde_json::to_string(&redacted).unwrap();
        assert!(!s.contains("SECRET"));
        assert!(!s.contains("hunter2"));
        assert!(!s.contains("sk-leaked"));
        assert!(s.contains("[REDACTED]"));
        assert!(s.contains("WebSearch"));
        assert!(s.contains("fine"));
        assert!(s.contains("\"count\":5"));
    }

    #[test]
    fn summary_cot_rejected() {
        // 含 reasoning 标记 → 不安全
        assert!(!summary_is_safe("reasoning_content: let me think..."));
        assert!(!summary_is_safe("<thought>secret</thought>"));
        // safe execution summary → 安全
        assert!(summary_is_safe("检索长期记忆"));
        assert!(summary_is_safe("调用工具 WebSearch"));
    }

    #[test]
    fn recorder_root_and_child_tree() {
        let (r, s) = recorder();
        let root = r
            .start_root(TraceSpanKind::Conversation, "user", Some("用户提问"), Some("s1"), None)
            .unwrap();
        let mem = r
            .start_child(&root.trace_id, &root.span_id, TraceSpanKind::Memory, "companion", Some("检索记忆"), Some("s1"), None)
            .unwrap();
        let tool = r
            .start_child(&root.trace_id, &mem.span_id, TraceSpanKind::Tool, "tool:WebSearch", None, Some("s1"), Some(json!({"args":{"q":"rust"}})))
            .unwrap();
        r.end_span(&tool.span_id, TraceSpanStatus::Succeeded, Some("搜索完成")).unwrap();
        r.end_span(&root.span_id, TraceSpanStatus::Succeeded, Some("响应完成")).unwrap();

        let spans = s.list_trace_spans(&root.trace_id).unwrap();
        assert_eq!(spans.len(), 3);
        // parent-child 关联正确
        assert!(spans.iter().any(|sp| sp.span_id == mem.span_id && sp.parent_span_id == Some(root.span_id.clone())));
        assert!(spans.iter().any(|sp| sp.span_id == tool.span_id && sp.parent_span_id == Some(mem.span_id.clone())));
        // 工具 span 终态 succeeded
        let tool_span = spans.iter().find(|sp| sp.span_id == tool.span_id).unwrap();
        assert_eq!(tool_span.status, TraceSpanStatus::Succeeded);
        assert!(tool_span.ended_at.is_some());
    }

    #[test]
    fn recorder_attributes_redacted_on_store() {
        let (r, s) = recorder();
        let root = r
            .start_root(
                TraceSpanKind::Tool,
                "tool:ShellExec",
                Some("执行命令"),
                None,
                Some(json!({"api_key": "sk-LEAKED", "command": "ls", "env_token": "gho_xyz"})),
            )
            .unwrap();
        let got = s.get_trace_span(&root.span_id).unwrap().unwrap();
        let json = serde_json::to_string(&got.attributes).unwrap();
        assert!(!json.contains("LEAKED"));
        assert!(!json.contains("gho_xyz"));
        assert!(json.contains("[REDACTED]"));
        assert!(json.contains("ls"));
    }

    #[test]
    fn recorder_cot_summary_not_stored() {
        let (r, s) = recorder();
        let root = r
            .start_root(TraceSpanKind::Agent, "commander", Some("reasoning_content: I should think about..."), None, None)
            .unwrap();
        let got = s.get_trace_span(&root.span_id).unwrap().unwrap();
        let json = serde_json::to_string(&got).unwrap();
        // CoT 被替换为 safe 占位
        assert!(!json.contains("reasoning_content"));
        assert!(!json.contains("I should think"));
        assert!(json.contains("execution step"));
    }

    #[test]
    fn recorder_failure_status() {
        let (r, s) = recorder();
        let root = r
            .start_root(TraceSpanKind::Worker, "worker:1", Some("子任务"), None, None)
            .unwrap();
        let ended = r.end_span(&root.span_id, TraceSpanStatus::Failed, Some("超时")).unwrap();
        assert_eq!(ended.status, TraceSpanStatus::Failed);
        let got = s.get_trace_span(&root.span_id).unwrap().unwrap();
        assert_eq!(got.status, TraceSpanStatus::Failed);
    }

    #[test]
    fn recorder_tool_args_secret_injection_neutralized() {
        // 攻击场景: 工具调用 args 携带 secret (api_key / Authorization / master_token / cookie / PAT).
        // trace attributes 必须 redaction — secret 绝不落库.
        let (r, s) = recorder();
        let attrs = json!({
            "tool": "ShellExec",
            "api_key": "sk-SECRET-LIVE-12345",
            "headers": {"Authorization": "Bearer SECRET-TOKEN"},
            "env": {"MASTER_TOKEN": "master-SECRET"},
            "cookie": "session=SECRET-COOKIE",
            "args": ["--pat", "ghp_GITHUB-PAT-SECRET"],
            "command": "ls -la"
        });
        let span = r
            .start_root(TraceSpanKind::Tool, "tool:ShellExec", Some("执行命令"), None, Some(attrs))
            .unwrap();
        let got = s.get_trace_span(&span.span_id).unwrap().unwrap();
        let stored = serde_json::to_string(&got.attributes).unwrap();
        // secret 绝不出现在持久化的 attributes
        assert!(!stored.contains("SECRET"), "SECRET must not persist: {stored}");
        assert!(!stored.contains("sk-SECRET"));
        assert!(!stored.contains("master-SECRET"));
        assert!(!stored.contains("ghp_GITHUB"));
        assert!(!stored.contains("Bearer SECRET"));
        assert!(stored.contains("[REDACTED]"));
        // 非 secret 值保留
        assert!(stored.contains("ls -la"));
        assert!(stored.contains("ShellExec"));
    }

    #[test]
    fn recorder_nested_secret_redaction() {
        // 深层嵌套的 secret 也要 redaction.
        let (r, s) = recorder();
        let attrs = json!({
            "outer": {"inner": {"password": "hunter2", "ok": "keep"}},
            "arr": [{"token": "tok-secret"}, {"name": "tool"}]
        });
        let span = r
            .start_root(TraceSpanKind::Tool, "tool:X", None, None, Some(attrs))
            .unwrap();
        let got = s.get_trace_span(&span.span_id).unwrap().unwrap();
        let stored = serde_json::to_string(&got.attributes).unwrap();
        assert!(!stored.contains("hunter2"));
        assert!(!stored.contains("tok-secret"));
        assert!(stored.contains("keep"));
        assert!(stored.contains("tool"));
    }
}
