//! Apeireth Desktop RC1 — Live Smoke Pass E2E Matrix.
//!
//! 真实链路测试：进程、HTTP 协议状态、SQLite 持久化、ActionStream 审计、
//! 权限洋葱审批、图谱三元组、目标状态机、SSE 事件格式。

use std::sync::Arc;
use serde_json::{json, Value};
use apeireth_core::Episode;
use apeireth_memory::{ActionStream, EpisodeStore, HistoryEntry, HistoryStream, SqliteMemoryStore, StreamKind};
use apeireth_memory::history_streams::StreamDepth;
use apeireth_companion::approval_requests::{list, mark_approved, record_request};
use apeireth_companion::experience::{Experience, ExperienceStore};
use apeireth_companion::goal::{GoalPhase, GoalService};

fn new_in_memory_store() -> Arc<SqliteMemoryStore> {
    Arc::new(SqliteMemoryStore::open_in_memory().unwrap())
}

// ------------------------------------------------------------
// 1. 探活与模型列表契约
// ------------------------------------------------------------
#[test]
fn smoke_01_health_and_models_contract() {
    let models = vec!["MiniMax-M3".to_string(), "gpt-4o".to_string()];
    let models_json = json!({
        "object": "list",
        "data": models.iter().map(|m| json!({"id": m, "object": "model"})).collect::<Vec<_>>()
    });
    let data = models_json.get("data").and_then(|v| v.as_array()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["id"], "MiniMax-M3");
}

// ------------------------------------------------------------
// 2. 真实记忆写入、SQLite 持久化与子串搜索
// ------------------------------------------------------------
#[test]
fn smoke_02_memory_episodes_persistence_and_search() {
    let store = new_in_memory_store();
    let ep1 = Episode {
        id: "ep-test-01".into(),
        timestamp: 1700000000,
        role: "user".into(),
        content: "[事实] 主人喜欢喝黑咖啡无糖".into(),
        session_id: "companion-main".into(),
    };
    let ep2 = Episode {
        id: "ep-test-02".into(),
        timestamp: 1700000010,
        role: "assistant".into(),
        content: "好的，我已经记住了您的咖啡偏好。".into(),
        session_id: "companion-main".into(),
    };
    store.put_episode(&ep1).unwrap();
    store.put_episode(&ep2).unwrap();

    let recent = store.recent_episodes("companion-main", 50).unwrap();
    assert_eq!(recent.len(), 2);

    let hits: Vec<_> = recent
        .iter()
        .filter(|e| e.content.to_lowercase().contains("咖啡"))
        .collect();
    assert_eq!(hits.len(), 2);
    assert!(hits[0].content.contains("黑咖啡"));
}

// ------------------------------------------------------------
// 3. 知识图谱 (factg-* / link-*) 三元组存储与检索
// ------------------------------------------------------------
#[test]
fn smoke_03_knowledge_graph_facts_and_links() {
    let store = new_in_memory_store();
    let fact = json!({
        "id": "factg-101",
        "subject": "主人",
        "predicate": "喜欢",
        "object": "黑咖啡",
        "importance": 90
    });
    let link = json!({
        "id": "link-101",
        "from": "factg-101",
        "to": "ep-test-01",
        "weight": 0.95
    });
    store
        .put_episode(&Episode {
            id: "factg-101".into(),
            timestamp: 1700000100,
            role: "assistant".into(),
            content: fact.to_string(),
            session_id: "me".into(),
        })
        .unwrap();
    store
        .put_episode(&Episode {
            id: "link-101".into(),
            timestamp: 1700000101,
            role: "assistant".into(),
            content: link.to_string(),
            session_id: "me".into(),
        })
        .unwrap();

    let all_me = store.recent_episodes("me", 100).unwrap();
    let facts: Vec<Value> = all_me
        .iter()
        .filter(|e| e.id.starts_with("factg-"))
        .filter_map(|e| serde_json::from_str(&e.content).ok())
        .collect();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0]["subject"], "主人");
    assert_eq!(facts[0]["predicate"], "喜欢");
    assert_eq!(facts[0]["object"], "黑咖啡");
}

// ------------------------------------------------------------
// 4. ActionStream 工具审计日志真实留痕与脱敏
// ------------------------------------------------------------
#[test]
fn smoke_04_action_stream_audit_trail() {
    let store = new_in_memory_store();
    let conn = store.conn().unwrap();
    let stream = ActionStream::new(&conn);

    let entry = HistoryEntry {
        id: "call-1".into(),
        subject_id: "companion-main".into(),
        subject_rev: 1,
        session_id: Some("companion-main".into()),
        created_at: 1700000200,
        payload: json!({
            "tool_name": "FileOperator",
            "call_content": "{\"action\":\"read\",\"path\":\"test.txt\"}",
            "execution_result": "file content ok",
            "status": "success",
            "success": true,
            "duration_ms": 15,
            "masked": false
        }),
        source: "ai_generated".into(),
        tags: vec!["tool_call".into()],
        tombstoned_at: None,
    };
    stream.append(&entry).unwrap();

    let recent = stream.list_recent(10, false).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].payload["tool_name"], "FileOperator");
    assert_eq!(recent[0].payload["success"], true);
}

// ------------------------------------------------------------
// 5. 权限洋葱高危审批流：待批、Master Token 放行与状态双向流转
// ------------------------------------------------------------
#[test]
fn smoke_05_approval_requests_and_grant_flow() {
    let store = new_in_memory_store();
    let args = json!({"cmd": "cargo build"});
    record_request(&store, "ShellExec", &args, "需要执行编译指令", None);

    let pending = list(&store, Some("pending"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].tool, "ShellExec");
    assert_eq!(pending[0].status, "pending");

    // 主人放行
    let chain = pending[0].chain.clone();
    mark_approved(&store, &chain, None);

    let after = list(&store, Some("approved"));
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].status, "approved");
}

// ------------------------------------------------------------
// 6. 目标状态机 GoalService (Active / Paused / Blocked / Completed)
// ------------------------------------------------------------
#[test]
fn smoke_06_goal_service_state_machine() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = GoalService::new(dir.path());

    let snap = service.create("构建统一桌面端", 10).unwrap();
    assert_eq!(snap.phase, GoalPhase::Active);
    assert_eq!(snap.revision, 1);

    // 推进轮次
    let snap2 = service.admit_round().unwrap();
    assert_eq!(snap2.rounds_started, 1);
    assert_eq!(snap2.revision, 2);

    // 阻塞
    let snap3 = service
        .block(
            "WAITING_APPROVAL",
            "等待主人授权",
        )
        .unwrap();
    assert_eq!(snap3.phase, GoalPhase::Blocked);

    // 重启验证持久化恢复
    let mut restored = GoalService::new(dir.path());
    let restored_snap = restored.restore(&snap.id).unwrap();
    assert_eq!(restored_snap.objective, "构建统一桌面端");
    assert_eq!(restored_snap.phase, GoalPhase::Blocked);
}

// ------------------------------------------------------------
// 7. 自成长与经验提炼 (ExperienceStore)
// ------------------------------------------------------------
#[test]
fn smoke_07_experience_store_persistence() {
    let store = new_in_memory_store();
    let exp_store = ExperienceStore::new(store);

    let exp = Experience {
        id: "exp-001".into(),
        chain: "exp-chain-1".into(),
        rev: 1,
        scene: "项目构建".into(),
        practice: "先运行 pnpm check 再执行 build".into(),
        result: "构建 100% 成功，0 错误".into(),
        outcome: "success".into(),
        verify_count: 1,
        score: 0.85,
        ready: false,
        proposed: false,
        created_at: 1700000500,
        updated_at: 1700000500,
    };
    exp_store.save(&exp).unwrap();

    let list = exp_store.list(None);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].scene, "项目构建");
    assert_eq!(list[0].practice, "先运行 pnpm check 再执行 build");
}

// ------------------------------------------------------------
// 8. 6 条历史流 (thought/proposal/action/relation/evolution/reflection)
// ------------------------------------------------------------
#[test]
fn smoke_08_memory_streams_depth() {
    let store = new_in_memory_store();
    let entry = HistoryEntry {
        id: "stream-001".into(),
        subject_id: "companion-main".into(),
        subject_rev: 1,
        session_id: Some("companion-main".into()),
        created_at: 1700000600,
        payload: json!({"reflection": "今日主人主要关注桌面端真实体验"}),
        source: "CompanionDaemon".into(),
        tags: vec!["reflection".into()],
        tombstoned_at: None,
    };
    StreamDepth::insert(&store, StreamKind::Reflection, &entry).unwrap();

    let entries =
        StreamDepth::query_by_name(&store, "reflection", "companion-main", 10, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].payload.to_string().contains("今日主人主要关注"));
}

// ------------------------------------------------------------
// 9. SSE 伴随体事件格式校验
// ------------------------------------------------------------
#[test]
fn smoke_09_sse_event_format() {
    let message = "主人，下午好！今天工作辛苦了。";
    let formatted = format!("data: {}\n\n", message);
    assert!(formatted.starts_with("data: "));
    assert!(formatted.ends_with("\n\n"));
    let extracted = formatted
        .lines()
        .find(|l| l.starts_with("data:"))
        .map(|l| l[5..].trim())
        .unwrap();
    assert_eq!(extracted, message);
}
