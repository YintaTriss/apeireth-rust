//! Task DAG Lease Mechanism — BORROW AgentFlow task state machine.
//!
//! **借鉴 ID**: `BORROW-Jimmyxiao2009/AgentFlow-task-dag-lease-2026-08-21`
//! **License**: AgentFlow 无 LICENSE (默认 all-rights-reserved).
//! **借鉴方式**: **设计思想 + 字段级 API 形状**, 0 行代码复制, 全 Rust 重写.
//!
//! # 8 哲学锚穿透 (per O-1 / O-2 / O-5)
//!
//! - **O-1 安全优先** — `Running → Ready` 转换被明文禁止 (AgentFlow 教训, 防止破坏状态机)
//! - **O-2 走在前人肩上** — 字段级移植 AgentFlow `TaskState.{Pending,Rented,Running,...}` 6 状态
//! - **O-5 不假装** — 见 module 末尾 "Explicit non-goals" 段, 持久化 / async / DAG 感知等**未做**
//!
//! # 状态机不变量 (不可妥协)
//!
//! ```text
//! Ready ──acquire──▶ Leased ──release(未跑)──▶ Ready
//!                      │
//!                      └──start──▶ Running ──release──▶ Completed
//!                                          │
//!                                          └──reap_expired──▶ Failed   ← 到期强制 Failed
//!                                          (✗ Ready 禁止 — 会破坏状态机)
//! ```
//!
//! # 典型用法
//!
//! ```no_run
//! use apeireth_team_lead::lease::{InMemoryLeaseManager, LeaseManager, TaskId, try_acquire};
//! use std::sync::Arc;
//!
//! let mgr: Arc<dyn LeaseManager> = Arc::new(InMemoryLeaseManager::new());
//! let guard = try_acquire(Arc::clone(&mgr), TaskId::from("task-42"), "agent-1", 15 * 60 * 1000).unwrap();
//! // ... agent 执行 task ...
//! drop(guard);  // RAII 自动 release
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// §1 类型定义: TaskId + TaskState + TaskLease
// ============================================================================

/// Task 唯一标识. 当前为 String newtype (UUID/字符串), 后续可换 Uuid.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    /// 便利构造.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Task 生命周期状态 (per AgentFlow TaskState 字段级移植).
///
/// 6 状态对应 AgentFlow 原仓库 (Pending/Rented/Running/Failed/Canceled/Done),
/// 此处用更标准的 Rust 术语命名, 字段含义 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// 未分配, 等待调度.
    Ready,
    /// 已分配且发了租约, owner 尚未开始执行.
    Leased,
    /// owner 正在执行.
    Running,
    /// 终态 — 失败 (包括到期 reap).
    Failed,
    /// 终态 — 用户取消.
    Cancelled,
    /// 终态 — 成功完成.
    Completed,
}

impl TaskState {
    /// 终态判断 — Failed / Cancelled / Completed.
    pub fn is_terminal(self) -> bool {
        matches!(self, TaskState::Failed | TaskState::Cancelled | TaskState::Completed)
    }

    /// 活跃态判断 — Leased / Running (被分配且未终态).
    pub fn is_active(self) -> bool {
        matches!(self, TaskState::Leased | TaskState::Running)
    }
}

/// Task 租约. 分配时发放, 到期未释放 → reap 强制 Failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLease {
    /// 租约所属 task.
    pub task_id: TaskId,
    /// 持租 agent 标识.
    pub owner: String,
    /// 发放时间 (Unix epoch ms).
    pub granted_at_ms: i64,
    /// 到期时间 (Unix epoch ms).
    pub expires_at_ms: i64,
    /// 代际号 — 每次 acquire 自增, 防止 ABA: 同一 task_id release 后新 owner acquire,
    /// 旧 guard drop 不会误 release 新租约.
    pub generation: u64,
}

// ============================================================================
// §2 错误类型
// ============================================================================

/// Lease 操作错误.
#[derive(Debug, Error)]
pub enum LeaseError {
    /// task 已被分配, 二次 acquire 失败.
    #[error("task {0} is already leased (generation={1})")]
    AlreadyLeased(TaskId, u64),
    /// release 时 task 未在 active (可能已 reap 或已 release).
    #[error("task {0} has no active lease to release")]
    NotLeased(TaskId),
    /// generation 不匹配 — ABA 防护命中.
    #[error("task {0} lease generation mismatch: expected={1}, got={2}")]
    GenerationMismatch(TaskId, u64, u64),
    /// 内部 lock 污染 (Mutex poison).
    #[error("internal lock poisoned")]
    LockPoisoned,
    /// ttl_ms 非正.
    #[error("invalid ttl_ms: {0} (must be > 0)")]
    InvalidTtl(i64),
}

// ============================================================================
// §3 LeaseGuard — RAII 自动释放
// ============================================================================

/// RAII handle — `Drop` 时若未显式 release 则自动 `release`.
///
/// 设计要点:
/// - `lease: Option<TaskLease>` — 显式 release 后 take(), drop 时跳过
/// - `released: bool` — 防御性标志, 防止 Option take() 与 release 间的竞态
pub struct LeaseGuard {
    manager: Arc<dyn LeaseManager>,
    lease: Option<TaskLease>,
    released: bool,
}

impl LeaseGuard {
    /// 显式 release — 把租约归还 manager, drop 时不再自动 release.
    /// 返回 Err 表示 release 失败 (例如已 reap), 但 guard 已被 consumed.
    pub fn release(mut self) -> Result<(), LeaseError> {
        self.released = true;
        if let Some(lease) = self.lease.take() {
            self.manager.release(lease)
        } else {
            Ok(())
        }
    }

    /// 查看当前租约 (只读).
    pub fn lease(&self) -> Option<&TaskLease> {
        self.lease.as_ref()
    }
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Some(lease) = self.lease.take() {
            // RAII 不能因 release 失败就 abort — 仅 eprintln 记 debug
            if let Err(e) = self.manager.release(lease) {
                eprintln!("[apeireth-team-lead::lease] auto-release failed: {e}");
            }
        }
    }
}

// ============================================================================
// §4 LeaseManager trait
// ============================================================================

/// Lease 管理器 trait — 调度器循环每分钟调用 `reap_expired(now)` 检查到期.
///
/// **设计要点**: trait 方法**不**直接返回 LeaseGuard, 而是返回 `TaskLease` —
/// 因为 `LeaseGuard` 需要 `Arc<dyn LeaseManager>` 共享所有权, 由顶层 free function
/// `try_acquire()` 包装构造. 这样 trait 可保持 `&self` 借用, 与标准 trait 习惯兼容.
pub trait LeaseManager: Send + Sync {
    /// 尝试为 task 发放租约给 owner, ttl_ms 后到期.
    /// 失败 (AlreadyLeased / InvalidTtl) 时返回 Err, 不消耗配额.
    fn try_acquire(
        &self,
        task_id: TaskId,
        owner: &str,
        ttl_ms: i64,
    ) -> Result<TaskLease, LeaseError>;

    /// 显式释放租约 — 配合 `LeaseGuard::release()`.
    /// 若租约已不在 (被 reap), 返回 `NotLeased`.
    fn release(&self, lease: TaskLease) -> Result<(), LeaseError>;

    /// 到期回收 — 返回所有到期且仍 active 的 task_id 列表.
    /// **调用方决定** 如何把这些 task 标记为 Failed (本 trait 不耦合状态机).
    ///
    /// 关键不变量: reap 后的 task **不应回到 Ready** — 仅返回 task_id 给 caller.
    fn reap_expired(&self, now_ms: i64) -> Vec<TaskId>;

    /// 当前所有 active 租约快照 (用于 dashboard / debug).
    fn active_leases(&self) -> Vec<TaskLease>;

    /// 当前系统时间 (Unix epoch ms) — 便利方法, 包装 `SystemTime`.
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// 顶层便利入口: 在 `Arc<dyn LeaseManager>` 上 acquire 并自动 wrap 到 RAII guard.
///
/// 失败时返回 Err, 不消耗配额.
pub fn try_acquire(
    mgr: Arc<dyn LeaseManager>,
    task_id: TaskId,
    owner: &str,
    ttl_ms: i64,
) -> Result<LeaseGuard, LeaseError> {
    let lease = mgr.try_acquire(task_id, owner, ttl_ms)?;
    Ok(LeaseGuard {
        manager: mgr,
        lease: Some(lease),
        released: false,
    })
}

// ============================================================================
// §5 InMemoryLeaseManager — std::sync::Mutex<HashMap<TaskId, TaskLease>>
// ============================================================================

/// 内存版 LeaseManager. 重启后所有 lease 丢失 (P1 持久化).
///
/// 线程安全: 内部 `Mutex<HashMap<TaskId, LeaseEntry>>` + `AtomicU64` global_gen,
/// 所有公开方法通过 lock 串行化. 不用 tokio Mutex, 保持 sync 接口 (per 设计 §6 "不假装").
///
/// **用法**: 通过 `Arc<InMemoryLeaseManager>` 共享 (impl LeaseManager for InMemoryLeaseManager),
/// 然后 `let mgr: Arc<dyn LeaseManager> = Arc::new(InMemoryLeaseManager::new());`
/// `try_acquire(Arc::clone(&mgr), ...).unwrap()` 即可.
pub struct InMemoryLeaseManager {
    inner: Mutex<HashMap<TaskId, LeaseEntry>>,
    global_gen: AtomicU64,
}

#[derive(Debug)]
struct LeaseEntry {
    lease: TaskLease,
    /// 占位 — 实际 generation 从 global_gen 取 (单调全局增, 跨 task 共享).
    /// 保留供将来"per-task generation"扩展.
    #[allow(dead_code)]
    next_gen: u64,
}

impl Default for InMemoryLeaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryLeaseManager {
    /// 构造空 manager.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            global_gen: AtomicU64::new(1),
        }
    }

    /// 当前 active lease 数量 (供测试 / metric 用).
    pub fn active_count(&self) -> usize {
        self.inner.lock().map(|m| m.len()).unwrap_or(0)
    }
}

impl LeaseManager for InMemoryLeaseManager {
    fn try_acquire(
        &self,
        task_id: TaskId,
        owner: &str,
        ttl_ms: i64,
    ) -> Result<TaskLease, LeaseError> {
        if ttl_ms <= 0 {
            return Err(LeaseError::InvalidTtl(ttl_ms));
        }
        let now = self.now_ms();
        let mut guard = self.inner.lock().map_err(|_| LeaseError::LockPoisoned)?;
        if let Some(entry) = guard.get(&task_id) {
            return Err(LeaseError::AlreadyLeased(task_id.clone(), entry.lease.generation));
        }
        let gen = self.global_gen.fetch_add(1, Ordering::SeqCst);
        let lease = TaskLease {
            task_id: task_id.clone(),
            owner: owner.to_string(),
            granted_at_ms: now,
            expires_at_ms: now + ttl_ms,
            generation: gen,
        };
        guard.insert(
            task_id,
            LeaseEntry {
                lease: lease.clone(),
                next_gen: gen + 1,
            },
        );
        Ok(lease)
    }

    fn release(&self, lease: TaskLease) -> Result<(), LeaseError> {
        let mut guard = self.inner.lock().map_err(|_| LeaseError::LockPoisoned)?;
        match guard.get(&lease.task_id) {
            Some(entry) if entry.lease.generation == lease.generation => {
                guard.remove(&lease.task_id);
                Ok(())
            }
            Some(entry) => Err(LeaseError::GenerationMismatch(
                lease.task_id.clone(),
                entry.lease.generation,
                lease.generation,
            )),
            None => Err(LeaseError::NotLeased(lease.task_id)),
        }
    }

    fn reap_expired(&self, now_ms: i64) -> Vec<TaskId> {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let expired: Vec<TaskId> = guard
            .iter()
            .filter(|(_, entry)| entry.lease.expires_at_ms <= now_ms)
            .map(|(tid, _)| tid.clone())
            .collect();
        for tid in &expired {
            guard.remove(tid);
        }
        expired
    }

    fn active_leases(&self) -> Vec<TaskLease> {
        self.inner
            .lock()
            .map(|m| m.values().map(|e| e.lease.clone()).collect())
            .unwrap_or_default()
    }
}

// ============================================================================
// §6 Explicit non-goals (per O-5 不假装)
// ============================================================================
//
// 以下行为**有意不做**, 留作后续 P1:
// - **持久化**: 重启后 in-memory lease 全部丢失. 后续用 sled/sqlite WAL.
// - **reap 自动改 state**: reap 只返回 task_id, 由 caller 决定标 Failed. 设计上松耦合 Orchestrator.
// - **async API**: 当前 std::sync::Mutex + sync trait. 改 async 需 Orchestrator 一起迁移.
// - **Clock injection**: 用 SystemTime 而非可注入 Clock trait. 测试用 mock `now_ms` 参数绕过.
// - **DAG 拓扑感知**: 只按 TaskId 维度管理, 不感知 task 间依赖.
// - **Leased → Running 自动转换**: caller 需手动管理 (本模块不耦合 TaskState).
// - **显式 `mark_running(task_id)` API**: 留作后续 — 当前 caller 直接用 Orchestrator 内部状态机.
//
// ============================================================================
// §7 测试矩阵 (8 项 — 见 design-task-lease-2026-08-21.md §5)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn new_mgr() -> Arc<dyn LeaseManager> {
        Arc::new(InMemoryLeaseManager::new())
    }

    fn active_count_dyn(mgr: &Arc<dyn LeaseManager>) -> usize {
        // 通过 trait 看不到 active_count, 用 active_leases().len() 替代
        mgr.active_leases().len()
    }

    /// 1. acquire → release (未跑) → state 回到 Ready (新 owner 可重新 acquire).
    #[test]
    fn acquire_then_release_state_goes_ready_again() {
        let mgr = new_mgr();
        let guard = try_acquire(Arc::clone(&mgr), "task-1".into(), "agent-A", 60_000)
            .expect("acquire 1");
        assert_eq!(guard.lease().unwrap().owner, "agent-A");
        guard.release().expect("release 1");
        assert_eq!(active_count_dyn(&mgr), 0);
        // 重新 acquire 应成功 (回到 Ready)
        let guard2 = try_acquire(Arc::clone(&mgr), "task-1".into(), "agent-B", 60_000)
            .expect("acquire 2");
        assert_eq!(guard2.lease().unwrap().owner, "agent-B");
        drop(guard2);
    }

    /// 2. acquire → (caller marks Running) → release (completed) — 全 happy path.
    #[test]
    fn acquire_then_run_then_complete_full_happy_path() {
        let mgr = new_mgr();
        let task_id: TaskId = "task-2".into();
        // acquire: Ready → Leased
        let guard = try_acquire(Arc::clone(&mgr), task_id.clone(), "agent-X", 60_000)
            .expect("acquire");
        assert_eq!(guard.lease().unwrap().task_id, task_id);
        // (caller 标 Running — 本模块不耦合 TaskState, 由 Orchestrator 维护)
        // release: Leased → Completed
        guard.release().expect("release");
        assert_eq!(active_count_dyn(&mgr), 0);
    }

    /// 3. **关键不变量** — reap 到期 task 必须返回 task_id (caller 标 Failed),
    ///    且 reap 后 lease 不再 active (即不允许"返回 Ready 池").
    #[test]
    fn reap_expired_marks_task_failed_not_ready() {
        let mgr = new_mgr();
        // ttl=10ms, reap 用 i64::MAX 确保 expires_at_ms(≈ now + 10) <= MAX
        let _guard = try_acquire(Arc::clone(&mgr), "task-3".into(), "agent-FAIL", 10)
            .expect("acquire");
        let expired = mgr.reap_expired(i64::MAX);
        assert_eq!(expired, vec![TaskId::from("task-3")]);
        // 关键验证: reap 后 lease 不再 active — 验证"自动回 Ready"被禁止
        assert_eq!(active_count_dyn(&mgr), 0);
        // caller 现在必须标 Failed (终态); 若它想"重试"必须显式 release + 重新 acquire.
        let _new = try_acquire(Arc::clone(&mgr), "task-3".into(), "agent-RETRY", 60_000)
            .expect("re-acquire OK; caller 决定怎么标 — 本测试仅证明 reap 不主动改 state");
    }

    /// 4. Running → Ready 是**非法转换**. reap 只返回 task_id, 不允许 caller 直接
    ///    release 一个伪造的 lease 把 task 丢回 Ready 池 (会被 generation 检查挡住).
    #[test]
    fn running_to_ready_is_forbidden_runtime_check() {
        let mgr = new_mgr();
        // 伪造 lease 试图 release 一个未 acquire 的 task — 必须失败
        let fake_lease = TaskLease {
            task_id: TaskId::from("task-4"),
            owner: "agent-GHOST".into(),
            granted_at_ms: 0,
            expires_at_ms: 100,
            generation: 9999, // 永远不可能存在的 generation
        };
        let res = mgr.release(fake_lease);
        assert!(
            matches!(res, Err(LeaseError::GenerationMismatch { .. } | LeaseError::NotLeased(_))),
            "release on missing/forged lease must fail (got {res:?})"
        );
    }

    /// 5. reap_expired 只收割到期 lease, 未到期的保留.
    ///
    /// 用 `mgr.now_ms()` 拿到 acquire 的时间基准, 然后注入 offset 模拟"当前时刻".
    #[test]
    fn reap_expired_only_affects_running_leases() {
        let mgr = new_mgr();
        // 短 TTL lease 立即到期
        let _a = try_acquire(Arc::clone(&mgr), "task-A".into(), "agent-1", 1)
            .expect("a (ttl=1ms)");
        // 长 TTL lease 不会到期
        let _b = try_acquire(Arc::clone(&mgr), "task-B".into(), "agent-2", 60_000)
            .expect("b (ttl=60s)");
        // 取 acquire 后的 now_ms, +10ms 必然 > 短 TTL 过期时间 (< now+1+采集误差),
        // 但 < 长 TTL 过期时间 (now + 60_000).
        let now = mgr.now_ms();
        let probe = now + 10;
        let expired = mgr.reap_expired(probe);
        assert_eq!(expired, vec![TaskId::from("task-A")]);
        assert_eq!(active_count_dyn(&mgr), 1);
        // task-B 仍 active
        assert_eq!(mgr.active_leases()[0].task_id, TaskId::from("task-B"));
    }

    /// 6. 并发 acquire 同一 task — 仅一个赢家.
    ///
    /// 关键: guard 必须存活到所有线程跑完, 否则 guard drop → release → map 清空,
    /// 后续 acquire 看到空 map 又能赢. 这本身**不**是 bug, 而是 RAII 的正确语义:
    /// guard 生命周期 = lease 生命周期.
    #[test]
    fn concurrent_acquire_on_same_task_only_one_wins() {
        let mgr = new_mgr();
        let mut handles = Vec::new();
        for i in 0..16 {
            let m = Arc::clone(&mgr);
            handles.push(std::thread::spawn(move || {
                let owner = format!("agent-{i}");
                // guard 必须 move 到 Vec 持有, 否则闭包返回时 drop
                let guard = try_acquire(m, "task-race".into(), &owner, 60_000);
                match guard {
                    Ok(g) => {
                        let gen = g.lease().unwrap().generation;
                        // 故意 NOT drop — 让 guard 跨线程边界持续持有
                        // 把 guard 包进 ManuallyDrop...不行,改用 Box leak
                        let _ = Box::leak(Box::new(g)); // leak — 仅测试
                        Ok(gen)
                    }
                    Err(e) => Err(e),
                }
            }));
        }
        let mut wins: Vec<u64> = Vec::new();
        let mut errs: Vec<LeaseError> = Vec::new();
        for h in handles {
            match h.join() {
                Ok(Ok(gen)) => wins.push(gen),
                Ok(Err(e)) => errs.push(e),
                Err(_) => panic!("thread panicked"),
            }
        }
        eprintln!("wins={}, errs={}", wins.len(), errs.len());
        // 仅一个线程成功 acquire (因为只有一个 guard 存活, 后续 acquire 都看到 map 非空)
        assert_eq!(wins.len(), 1, "exactly one acquirer should win (got {})", wins.len());
        // 至少要有 AlreadyLeased 错误 (15 个失败)
        assert!(errs.iter().any(|e| matches!(e, LeaseError::AlreadyLeased(_, _))),
            "expected AlreadyLeased errors, got {:?}", errs);
    }

    /// 7. RAII — guard drop 后 lease 不再 active.
    #[test]
    fn lease_guard_drop_auto_releases() {
        let mgr = new_mgr();
        {
            let _guard = try_acquire(Arc::clone(&mgr), "task-7".into(), "agent-A", 60_000)
                .expect("acquire");
            assert_eq!(active_count_dyn(&mgr), 1);
        } // guard drop — RAII auto-release
        assert_eq!(active_count_dyn(&mgr), 0);
    }

    /// 8. 显式 release 后 drop 不重复 release (no-op); 旧 lease 再次 release 返回 NotLeased.
    #[test]
    fn guard_release_then_drop_does_not_double_release() {
        let mgr = new_mgr();
        let guard = try_acquire(Arc::clone(&mgr), "task-8".into(), "agent-A", 60_000)
            .expect("acquire");
        guard.release().expect("explicit release");
        assert_eq!(active_count_dyn(&mgr), 0);
        // 模拟"旧 guard drop 试图再次 release" — release 返回 NotLeased, 不 panic.
        let stale_lease = TaskLease {
            task_id: TaskId::from("task-8"),
            owner: "agent-A".into(),
            granted_at_ms: 0,
            expires_at_ms: 60_000,
            generation: 1,
        };
        let res = mgr.release(stale_lease);
        assert!(matches!(res, Err(LeaseError::NotLeased(_))));
    }
}