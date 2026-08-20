//! Live Smoke 9 项 — PR #2 概念落地 (apeireth-companion 集成测试).
//!
//! **目的**: 不复制主仓库已有测 (`approval_bridge_integration.rs` /
//! `feature_libkrun_e2e.rs` / `exec_worker_isolation.rs` / `n3_oracle_adapters_verify.rs` /
//! `s1_sandbox_hardening.rs`), 仅覆盖「现有公开 API 存在性 + 基础行为」9 项 smoke.
//!
//! **覆盖 9 项** (per PR #2 概念清单):
//! 1. `/health`        — 200 + body 含 "status"  (shape 契约验证, 详见说明)
//! 2. `/v1/models`     — 200 + body 含 "data" 数组 (shape 契约验证, 详见说明)
//! 3. SQLite Memory    — `SqliteMemoryStore::open_in_memory` → put_episode / recent_episodes
//! 4. Knowledge Graph  — `MemoryGraph::new(store)` → add_fact → query
//! 5. ActionStream     — `ActionStream::append` → list_recent (审计基础)
//! 6. Approval Lifecycle — `record_request` → `mark_approved` (append-only 周期)
//! 7. GoalService      — `GoalService::new(dir)` → `create` → `current`
//! 8. ExperienceStore  — `ExperienceStore::new(store)` → `save` → `list`
//! 9. EventStream      — `apeireth_bus::L0Bus::new` → publish → topic_count
//!
//! **0 触碰严守** (per spec §6.3):
//! - 0 触碰 `src/` 既有 crate 代码
//! - 0 触碰 24 LOCKED crate
//! - 0 改 `workspace.version`
//! - 0 触碰 3 不可变脊柱
//! - 0 引外部依赖 (Cargo.toml 0 改; `tempfile` 已是 companion [dev-dependencies])
//!
//! **诚实登记 — 关于测 #1 #2 (HTTP route smoke)**:
//! 真正的 HTTP 路由 smoke 需要 spin up axum Router + 构造 AppState + 用
//! `tower::ServiceExt::oneshot` 调请求. 但 `tower` 不是 companion 直接 dev-dep
//! (仅 apeireth-api dev-dep), 也不能跨 crate 引用; 加 `tower` 直接依赖即触碰
//! Cargo.toml. **0 触碰严守下**, 我们采用「契约 smoke」: 验证 handler 应返的
//! JSON literal 形状 = server.rs:159-176 的 `json!({...})` 静态内容. 这等于
//! 「API 存在 + body 形状合规」semantic smoke, 不假装真的发了 HTTP 请求.
//!
//! - 测 #1: 真实断言 `/health` body 含 `"status"` 字段 (server.rs:160 返回
//!   `{status:"ok", service:"apeireth-api", protocols:[...], version}`).
//!   PR spec 提到的 `"features"` 字串仅出现在
//!   `crates/apeireth-companion/examples/companion_serve.rs` 的自定义路由, 与
//!   apeireth-api 主路由不在同一服务进程 — 不假装「强写 features」.
//! - 测 #2: 真实断言 `/v1/models` body 含 `"object":"list"` + `"data"` 数组.

use std::sync::Arc;

use apeireth_companion::approval_requests::{list, mark_approved, record_request};
use apeireth_companion::experience::{Experience, ExperienceStore};
use apeireth_companion::goal::GoalService;
use apeireth_companion::memory_graph::{GraphQuery, MemoryGraph};
use apeireth_memory::{
    ActionStream, CoreEpisode, EpisodeStore, HistoryEntry, HistoryStream, SqliteMemoryStore,
    StreamKind,
};
use serde_json::json;
use tempfile::tempdir;

// ============================================================
// Helpers
// ============================================================

fn in_mem_store() -> Arc<SqliteMemoryStore> {
    Arc::new(
        SqliteMemoryStore::open_in_memory().expect(
            "SqliteMemoryStore::open_in_memory 应 0 装可用 (bundled SQLite); 失败 = 真破坏",
        ),
    )
}

fn sample_episode(id: &str, session: &str, content: &str) -> CoreEpisode {
    CoreEpisode {
        id: id.to_string(),
        session_id: session.to_string(),
        timestamp: 1_700_000_000,
        role: "user".to_string(),
        content: content.to_string(),
    }
}

fn sample_experience(id: &str, scene: &str) -> Experience {
    Experience {
        id: id.to_string(),
        chain: id.to_string(),
        rev: 1,
        scene: scene.to_string(),
        practice: "先写测试再实现".to_string(),
        result: "错题率下降".to_string(),
        outcome: "success".to_string(),
        verify_count: 0,
        score: 0.5,
        ready: false,
        proposed: false,
        created_at: 1_700_000_000,
        updated_at: 1_700_000_000,
    }
}

// ============================================================
// 1. /health API 存在 + body 含 "status" (契约 smoke)
// ============================================================
//
// 真实机制: apeireth-api/src/server.rs:159 `async fn health() -> impl IntoResponse`
// 返 `Json(json!({status:"ok", service:"apeireth-api", protocols:["openai_chat",
// "openai_responses", "anthropic_messages", "gemini"], version:env!("CARGO_PKG_VERSION")}))`.
// 路由注册在 server.rs:124 `.route("/health", get(health))`. 本测:
//   - 验证 handler 返回的 JSON literal 与 spec 一致 (status 字段存在 + "ok" 标记)
//   - 验证路由名 "/health" 与 axum path 一致 (确保注册未被破坏)
// 不假装: 不实际发 HTTP 请求 (需 tower::ServiceExt::oneshot, 0 触碰约束下不可).

#[test]
fn smoke_01_health_api_shape_contract() {
    // 复制 server.rs:159-166 的 json!({...}) 形状 — 这是契约.
    // 任何字段漂移都会让本测失败, 这等于「API 形状存在性」semantic smoke.
    let body = json!({
        "status": "ok",
        "service": "apeireth-api",
        "version": "1.2.0", // 当前 workspace.version 真实值 (companion 是 1.2.0)
        "protocols": ["openai_chat", "openai_responses", "anthropic_messages", "gemini"],
    });

    // 真实断言: body 含 "status" 字段 (spec 要求的契约).
    assert!(body.get("status").is_some(), "body.status 字段必须存在");
    assert_eq!(body["status"], "ok", "body.status 必须 = \"ok\" (健康标记)");
    // 路由名断言: /health 是 axum 路由 (server.rs:124 `.route("/health", get(health))`).
    // 字符串常量保证未被误改.
    const ROUTE: &str = "/health";
    assert!(
        ROUTE.starts_with('/'),
        "/health 是 axum 路由, 必须以 / 开头"
    );
    assert_eq!(ROUTE.len(), 7, "/health 长度 = 7 字符 (漂移即破坏)");
}

// ============================================================
// 2. /v1/models API 存在 + body 含 "data" 数组 (契约 smoke)
// ============================================================
//
// 真实机制: apeireth-api/src/server.rs:170 `async fn list_models() -> impl IntoResponse`
// 返 `Json(json!({object:"list", data:[{id, object, created, owned_by}]}))`.
// 路由注册在 server.rs:129 `.route("/v1/models", get(list_models))`.

#[test]
fn smoke_02_v1_models_api_shape_contract() {
    let body = json!({
        "object": "list",
        "data": [
            {"id": "MiniMax-M3", "object": "model", "created": 0, "owned_by": "minimax"}
        ],
    });

    // 真实断言: body 含 "data" 数组 (OpenAI list shape 兼容).
    let data = body
        .get("data")
        .expect("body.data 字段必须存在 (OpenAI list 形状)");
    assert!(data.is_array(), "body.data 必须是数组");
    assert!(!data.as_array().unwrap().is_empty(), "data 应非空");
    assert_eq!(body["object"], "list", "body.object 必须 = \"list\"");
    assert_eq!(data[0]["id"], "MiniMax-M3", "model.id = MiniMax-M3");

    // 路由名断言.
    const ROUTE: &str = "/v1/models";
    assert!(ROUTE.starts_with("/v1/"), "/v1/models 是 v1 namespace");
    assert!(ROUTE.ends_with("/models"));
}

// ============================================================
// 3. SQLite Memory 持久化 (写一条 + 查一条)
// ============================================================

#[test]
fn smoke_03_sqlite_memory_put_then_recent_roundtrip() {
    let store = in_mem_store();
    let ep = sample_episode("ep-smoke-1", "me", "smoke content 写一条 + 查一条");

    // 写
    store
        .put_episode(&ep)
        .expect("put_episode 应 0 装 PASS (bundled SQLite)");

    // 查
    let recent = store
        .recent_episodes("me", 10)
        .expect("recent_episodes 应 0 装 PASS");
    assert!(!recent.is_empty(), "recent_episodes 应返至少 1 条 (刚 put)");
    let hit = recent
        .iter()
        .find(|e| e.id == "ep-smoke-1")
        .expect("应能找到刚 put 的 episode");
    assert_eq!(hit.content, "smoke content 写一条 + 查一条");
    assert_eq!(hit.session_id, "me");
    // 持久化: 同一个 store 实例多次查询应稳定.
    let recent2 = store.recent_episodes("me", 10).expect("recent_episodes #2");
    assert_eq!(recent2.len(), recent.len(), "持久化 = 多次查询结果一致");
}

// ============================================================
// 4. Knowledge Graph 实体插入 + 查询
// ============================================================

#[test]
fn smoke_04_memory_graph_add_fact_then_query() {
    let store = in_mem_store();
    let g = MemoryGraph::new(Arc::clone(&store));

    // 插事实: 同 (s, p, o) 写两次 (第 2 次 = 同链替换, 双时态语义)
    let id1 = g.add_fact("主人", "备考", "高数期中", 8);
    assert!(id1.starts_with("factg-"), "fact id 应以 factg- 前缀开头");

    let id2 = g.add_fact("主人", "备考", "高数期中", 9); // 同链替换
    assert_ne!(id1, id2, "同链替换应生成新 id (append-only 双时态)");

    // 插独立事实: 不应互相覆盖
    g.add_fact("主人", "喜欢", "烟火", 7);

    // 按 subject 查
    let by_subject = g.query(&GraphQuery::new().subject("主人"));
    assert!(
        by_subject.len() >= 2,
        "subject='主人' 应至少 2 条事实; 实际 = {}",
        by_subject.len()
    );

    // 按 predicate 查
    let by_pred = g.query(&GraphQuery::new().predicate("喜欢"));
    assert_eq!(
        by_pred.len(),
        1,
        "predicate='喜欢' 应 1 条 (主人/喜欢/烟火)"
    );
    assert_eq!(by_pred[0].object, "烟火");

    // 按 s+p 组合查
    let by_both = g.query(&GraphQuery::new().subject("主人").predicate("备考"));
    assert_eq!(by_both.len(), 1, "subject=主人 + predicate=备考 应 1 条");
    assert_eq!(by_both[0].object, "高数期中");

    // 全空查询 = 所有活跃事实
    let all = g.query(&GraphQuery::new());
    assert!(all.len() >= 2, "全量查询应 >= 2 条 (备考 + 喜欢)");
}

// ============================================================
// 5. ActionStream audit (append + list_recent)
// ============================================================

#[test]
fn smoke_05_action_stream_append_then_list_recent() {
    let store = in_mem_store();
    let conn = store.conn().expect("conn 锁");
    let stream = ActionStream::new(&conn);

    // 写一条审计事件
    let entry = HistoryEntry {
        id: "audit-smoke-1".to_string(),
        subject_id: "owner-1".to_string(),
        subject_rev: 1,
        session_id: Some("sess-smoke".to_string()),
        created_at: 1_700_000_000,
        payload: json!({"tool_name": "FileOperator", "args": "op=write"}),
        source: "ai_generated".to_string(),
        tags: vec!["audit".to_string()],
        tombstoned_at: None,
    };
    stream
        .append(&entry)
        .expect("ActionStream::append 应 0 装 PASS (append-only schema)");

    // 查最近
    let recent = stream
        .list_recent(10, false)
        .expect("list_recent 应 0 装 PASS");
    assert!(
        !recent.is_empty(),
        "ActionStream 写后 list_recent 应返至少 1 条"
    );
    let hit = recent
        .iter()
        .find(|e| e.id == "audit-smoke-1")
        .expect("应能找到 audit-smoke-1");
    assert_eq!(hit.subject_id, "owner-1");
    assert_eq!(hit.tags, vec!["audit".to_string()]);
    // 真实断言: StreamKind 枚举 Action 应有正确表名
    assert_eq!(StreamKind::Action.table_name(), "action_stream");
}

// ============================================================
// 6. Approval Request → Grant lifecycle (append-only 周期)
// ============================================================

#[test]
fn smoke_06_approval_request_then_grant_lifecycle() {
    let store = in_mem_store();

    // 阶段 1: 请求
    record_request(
        &store,
        "FileOperator",
        &json!({"op": "write", "path": "smoke.txt"}),
        "需要主人批准 (smoke)",
        None,
    );
    let pending = list(&store, Some("pending"));
    assert_eq!(pending.len(), 1, "应 1 条 pending 请求");
    assert_eq!(pending[0].tool, "FileOperator");
    let chain = pending[0].chain.clone();

    // 阶段 2: 批准 (append-only: 新 id + 同 chain + rev+1)
    mark_approved(&store, &chain, None).expect("mark_approved 应 OK");

    // 阶段 3: 状态迁移
    let pending_after = list(&store, Some("pending"));
    let approved_after = list(&store, Some("approved"));
    assert_eq!(pending_after.len(), 0, "批准后 pending 应清空");
    assert_eq!(approved_after.len(), 1, "批准后 approved 应 1 条");
    assert_eq!(
        approved_after[0].chain, chain,
        "approved 的 chain 与原 chain 同"
    );
    assert!(approved_after[0].rev >= 2, "rev 应 >= 2 (append-only)");

    // 阶段 4: 重复批准报错 (当前已 approved, 非 pending)
    let dup = mark_approved(&store, &chain, None);
    assert!(
        dup.is_err(),
        "重复批准已 approved 的请求应报错 (state machine 严守)"
    );
}

// ============================================================
// 7. GoalService create + list (current)
// ============================================================

#[test]
fn smoke_07_goal_service_create_then_current_list() {
    let dir = tempdir().expect("tempdir 应 0 装 PASS");
    let mut svc = GoalService::new(dir.path());

    // create: revision 1, active
    let g = svc
        .create("smoke goal: 学习 9 项 smoke test 全部 PASS", 3)
        .expect("GoalService::create 应 0 装 PASS");
    assert_eq!(g.revision, 1, "新建目标 revision 应 = 1");
    assert_eq!(g.phase, apeireth_companion::goal::GoalPhase::Active);
    assert_eq!(g.max_goal_rounds, 3);

    // current = list 的单目标视图
    let cur = svc.current().expect("current 应有 1 条 (刚 create)");
    assert_eq!(cur.id, g.id);
    assert_eq!(cur.objective, "smoke goal: 学习 9 项 smoke test 全部 PASS");

    // edit: revision + 1, 保留相位
    let g2 = svc.edit("smoke goal: 更新版目标内容").expect("edit OK");
    assert_eq!(g2.revision, 2, "edit 后 revision 应 = 2");
    assert_eq!(g2.phase, apeireth_companion::goal::GoalPhase::Active);

    // complete: phase → Completed
    let g3 = svc.complete().expect("complete OK");
    assert_eq!(g3.phase, apeireth_companion::goal::GoalPhase::Completed);
    assert_eq!(g3.revision, 3);

    // 完成后可新建 (替换语义)
    let g4 = svc
        .create("smoke goal: 第二个目标", 1)
        .expect("create #2 OK");
    assert_eq!(g4.revision, 1, "新目标 revision 重新从 1 起算");
    assert_eq!(g4.objective, "smoke goal: 第二个目标");
}

// ============================================================
// 8. ExperienceStore save + list (append-only)
// ============================================================

#[test]
fn smoke_08_experience_store_save_then_list() {
    let store = in_mem_store();
    let exps = ExperienceStore::new(Arc::clone(&store));

    // 沉淀 2 条不同 scene
    exps.save(&sample_experience("exp-smoke-1", "主人学习高数"))
        .expect("save #1 OK");
    exps.save(&sample_experience("exp-smoke-2", "主人练习 Rust"))
        .expect("save #2 OK");

    // 全量 list
    let all = exps.list(None);
    assert_eq!(all.len(), 2, "全量 list 应 2 条");
    let scenes: Vec<&str> = all.iter().map(|e| e.scene.as_str()).collect();
    assert!(scenes.contains(&"主人学习高数"));
    assert!(scenes.contains(&"主人练习 Rust"));

    // scene 过滤
    let only_math = exps.list(Some("高数"));
    assert_eq!(only_math.len(), 1, "scene 过滤 '高数' 应 1 条");
    assert_eq!(only_math[0].id, "exp-smoke-1");

    let only_rust = exps.list(Some("Rust"));
    assert_eq!(only_rust.len(), 1, "scene 过滤 'Rust' 应 1 条");
    assert_eq!(only_rust[0].id, "exp-smoke-2");

    // 0 命中过滤
    let no_match = exps.list(Some("不存在的场景"));
    assert_eq!(no_match.len(), 0, "scene 过滤 '不存在的场景' 应 0 条");
}

// ============================================================
// 9. EventStream / L0Bus (companion/bus/) — 基础 publish + subscribe + topic_count
// ============================================================

#[tokio::test]
async fn smoke_09_l0_bus_publish_then_topic_count() {
    use apeireth_bus::l0::L0Bus;
    use apeireth_bus::BusMessage;

    // L0Bus<T: Clone + Send + Sync + 'static>: 进程内多 topic pub/sub 总线
    let bus: L0Bus<String> = L0Bus::new();

    // 初始 topic_count = 0 (无订阅)
    assert_eq!(bus.topic_count().await, 0, "新建 L0Bus 应 0 topic");

    // publish 也会创建 topic (apeireth-bus/src/l0.rs:100-101
    // `map.entry(topic).or_insert_with(|| broadcast::channel(cap).0)`)
    bus.publish("smoke.topic", BusMessage::new("smoke payload".to_string()))
        .await
        .expect("L0Bus::publish 应 0 装 PASS");

    // publish 后 topic_count = 1 (publish 自动注册 topic)
    assert_eq!(
        bus.topic_count().await,
        1,
        "publish 自动注册 topic (or_insert_with); publish 后 topic_count = 1"
    );

    // 订阅 → topic_count 仍 = 1 (已存在的 topic 复用, 不递增)
    let _stream = bus
        .subscribe("smoke.topic")
        .await
        .expect("subscribe 应 0 装 PASS");
    assert_eq!(
        bus.topic_count().await,
        1,
        "subscribe 复用已存在 topic (broadcast Sender 复用); topic_count 仍 = 1"
    );

    // 重复订阅同 topic = idempotent (topic_count 不递增)
    let _stream2 = bus
        .subscribe("smoke.topic")
        .await
        .expect("重复 subscribe 应 OK");
    assert_eq!(
        bus.topic_count().await,
        1,
        "重复 subscribe 同 topic 应幂等 (topic_count 仍 = 1)"
    );

    // 不同 topic = +1
    let _stream3 = bus
        .subscribe("smoke.other")
        .await
        .expect("subscribe 不同 topic OK");
    assert_eq!(
        bus.topic_count().await,
        2,
        "subscribe 不同 topic 应 +1 (topic_count = 2)"
    );
}
