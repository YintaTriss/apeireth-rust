//! Core Capability Expansion Phase 2 — 后端会话生命周期集成测试.
//!
//! 验证 V5 migration + SessionLifecycle 完整路径:
//! - create / get / list / rename / archive / restore / close
//! - 状态机非法转换拒绝
//! - 乐观并发 (revision CAS) 冲突
//! - 持久化 (reopen)
//! - episode 关联保留 (archived session 的 episode 不丢)
//! - legacy client 兼容 (旧 4 列 upsert 的 session 仍可被生命周期操作)
//! - migration 幂等 (V5 重复应用不报错)

use apeireth_core::Episode;
use apeireth_memory::{
    EpisodeStore, SessionLifecycleError, SessionLifecycleStore, SessionScope, SessionState,
    SqliteMemoryStore, SessionStore as LegacySessionStore,
};
use apeireth_memory::run_migrations;
use std::sync::Arc;

fn store() -> Arc<SqliteMemoryStore> {
    Arc::new(SqliteMemoryStore::open_in_memory().unwrap())
}

#[test]
fn session_lifecycle_full_path() {
    let s = store();
    // create
    let r = s
        .create_session("s1", Some("首个"), SessionScope::Global, None, None)
        .unwrap();
    assert_eq!(r.state, SessionState::Active);
    assert_eq!(r.revision, 0);

    // rename (rev 0→1)
    let r = s.rename_session("s1", "改名", 0).unwrap();
    assert_eq!(r.title.as_deref(), Some("改名"));
    assert_eq!(r.revision, 1);

    // archive (rev 1→2)
    let r = s.archive_session("s1", 1).unwrap();
    assert_eq!(r.state, SessionState::Archived);

    // restore (rev 2→3)
    let r = s.restore_session("s1", 2).unwrap();
    assert_eq!(r.state, SessionState::Active);

    // close (rev 3→4, 终态)
    let r = s.close_session_lifecycle("s1", 3).unwrap();
    assert_eq!(r.state, SessionState::Closed);
    assert_eq!(r.revision, 4);
}

#[test]
fn session_invalid_transition_close_then_archive() {
    let s = store();
    s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
    s.close_session_lifecycle("s1", 0).unwrap();
    // closed 是终态, 不可再 archive
    let err = s.archive_session("s1", 1).unwrap_err();
    assert!(matches!(err, SessionLifecycleError::IllegalTransition { .. }));
}

#[test]
fn session_revision_conflict_rename_race() {
    let s = store();
    s.create_session("s1", Some("原标题"), SessionScope::Global, None, None).unwrap();
    // worker A rename with rev 0 → ok (rev 0→1)
    s.rename_session("s1", "A 改的", 0).unwrap();
    // worker B 同时用 rev 0 rename → conflict
    let err = s.rename_session("s1", "B 改的", 0).unwrap_err();
    match err {
        SessionLifecycleError::Conflict { expected, actual, .. } => {
            assert_eq!(expected, 0);
            assert_eq!(actual, 1);
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    // B 用正确 rev 1 → ok
    s.rename_session("s1", "B 改的", 1).unwrap();
}

#[test]
fn session_restart_persistence_reopen() {
    // in-memory store 不跨进程, 但验证 reopen 不崩 + migration 幂等 + 数据可读.
    let s = store();
    s.create_session("s1", Some("持久标题"), SessionScope::Global, None, None)
        .unwrap();
    s.archive_session("s1", 0).unwrap();
    // 二次跑 migration (幂等)
    {
        let guard = s.conn().unwrap();
        let mut g = guard;
        run_migrations(&mut g).unwrap();
    }
    let got = s.get_session_lifecycle("s1").unwrap().unwrap();
    assert_eq!(got.title.as_deref(), Some("持久标题"));
    assert_eq!(got.state, SessionState::Archived);
    assert_eq!(got.revision, 1);
    // V5 已应用
    assert!(s.applied_migrations().unwrap().contains(&5));
}

#[test]
fn session_episode_relation_archived_not_lost() {
    let s = store();
    s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
    let ep = Episode {
        id: "ep-1".into(),
        timestamp: 1000,
        role: "user".into(),
        content: "hello".into(),
        session_id: "s1".into(),
    };
    s.put_episode(&ep).unwrap();
    s.archive_session("s1", 0).unwrap();
    // episode 仍在 (archived ≠ delete)
    let eps = s.recent_episodes("s1", 10).unwrap();
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0].id, "ep-1");
}

#[test]
fn session_legacy_client_compat_old_upsert_readable() {
    // 旧 SessionStore trait 只写 4 列; 生命周期读取兼容 (NULL state→Active, NULL rev→0).
    let s = store();
    let old = apeireth_core::Session {
        id: "legacy-1".into(),
        started_at: 1000,
        last_active_at: 2000,
    };
    s.upsert_session(&old).unwrap();
    let got = s.get_session_lifecycle("legacy-1").unwrap().unwrap();
    assert_eq!(got.state, SessionState::Active);
    assert_eq!(got.revision, 0);
    assert_eq!(got.scope, SessionScope::Global);
    // 旧 session 可被生命周期操作 (rev 从 0 CAS)
    let r = s.archive_session("legacy-1", 0).unwrap();
    assert_eq!(r.state, SessionState::Archived);
}

#[test]
fn session_create_duplicate_rejected() {
    let s = store();
    s.create_session("s1", None, SessionScope::Global, None, None).unwrap();
    let err = s.create_session("s1", None, SessionScope::Global, None, None).unwrap_err();
    assert!(matches!(err, SessionLifecycleError::Conflict { .. }));
}

#[test]
fn session_not_found_errors() {
    let s = store();
    let err = s.rename_session("ghost", "x", 0).unwrap_err();
    assert!(matches!(err, SessionLifecycleError::NotFound(_)));
    let err = s.archive_session("ghost", 0).unwrap_err();
    assert!(matches!(err, SessionLifecycleError::NotFound(_)));
    let got = s.get_session_lifecycle("ghost").unwrap();
    assert!(got.is_none());
}

#[test]
fn session_list_excludes_archived_by_default() {
    let s = store();
    s.create_session("active-1", None, SessionScope::Global, None, None).unwrap();
    s.create_session("active-2", None, SessionScope::Global, None, None).unwrap();
    s.create_session("to-archive", None, SessionScope::Global, None, None).unwrap();
    s.archive_session("to-archive", 0).unwrap();
    assert_eq!(s.list_sessions(false).unwrap().len(), 2);
    assert_eq!(s.list_sessions(true).unwrap().len(), 3);
}

#[test]
fn session_project_scope_validation() {
    let s = store();
    // project scope 无 project_id → 拒绝
    let err = s
        .create_session("s1", None, SessionScope::Project, None, None)
        .unwrap_err();
    assert!(matches!(err, SessionLifecycleError::Invalid(_)));
    // 有 project_id → ok
    s.create_session("s2", None, SessionScope::Project, Some("proj-1"), None)
        .unwrap();
    let got = s.get_session_lifecycle("s2").unwrap().unwrap();
    assert_eq!(got.scope, SessionScope::Project);
    assert_eq!(got.project_id.as_deref(), Some("proj-1"));
}
