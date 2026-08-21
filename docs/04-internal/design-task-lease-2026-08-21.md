# Task DAG Lease Mechanism — 设计文档 (B 项) — 2026-08-21

> **状态**: 设计 + 实现 + 测试一次性落地 (P0)
> **借鉴 ID**: `BORROW-Jimmyxiao2009/AgentFlow-task-dag-lease-2026-08-21`
> **License**: AgentFlow 无 LICENSE (默认 all-rights-reserved); 本文档**仅借鉴设计思想, 0 行代码复制**, 全 Rust 重写.
> **哲学锚穿透**: O-1 安全优先 / O-2 走在前人肩上 / O-5 不假装

---

## 1. 问题陈述

Team Lead / Orchestrator (`apeireth-team-lead`) 当前分配 task 给子 Agent 时, 没有任何"任务失败兜底"机制:

- 子 Agent 拿到 task 后崩溃 (进程死 / 进程 hang / 网络断)
- Orchestrator 无法感知, 任务永久卡在 `Running`
- 整个 DAG 依赖此任务的 downstream 全部永久阻塞

R215 audit 已识别此缺陷 (`team-lead/lib.rs:505-506` 的 `let _ = timeout_ms; // 占位`).

## 2. 设计目标 (per AgentFlow 教训)

借鉴 AgentFlow 的 `TaskState` + lease 概念, 把"task 生命周期"显式建模:

1. **租约 (Lease)** — task 分配时同时发放一份带 timeout 的租约给 owner agent
2. **到期回收 (Reap)** — Scheduler 每分钟主动循环 `reap_expired(now)`, 把到期未释放的 task 强制标记为 `Failed` (不是 `Ready`!)
3. **RAII 自动释放 (LeaseGuard)** — 拿到租约的人 drop guard 时自动 `release`, 不依赖人记得手动调用
4. **不变量保护** — Running → Ready 是**非法转换** (会破坏状态机)

### 2.1 关键不变量 (AgentFlow 教训, 不可妥协)

```
Running → Failed (reap_expired 到期)  ✓
Running → Ready                        ✗ FORBIDDEN (会破坏状态机)
```

到期 task 必须标 `Failed` (终态), 而**不是** `Ready` (重新排队). 原因:

- `Ready` 意味着"可以重新分配", 但 reap 时无法区分"task 真的没在跑" vs "task 在跑但 owner 死了", 标 Ready 会把运行中的 task 重复分配
- `Failed` 是终态, 不再被调度, 状态机闭合

## 3. API 形状

### 3.1 TaskState 枚举 (per AgentFlow 字段级)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState { Ready, Leased, Running, Failed, Cancelled, Completed }
```

| State | 含义 |
|-------|------|
| `Ready` | 未分配, 等待调度 |
| `Leased` | 已分配且发了租约, owner 尚未开始执行 |
| `Running` | owner 已开始执行 (`start()` 调用) |
| `Failed` | 终态 — 失败 (包括到期 reap) |
| `Cancelled` | 终态 — 用户取消 |
| `Completed` | 终态 — 成功完成 |

### 3.2 TaskLease 结构

```rust
#[derive(Debug, Clone)]
pub struct TaskLease {
    pub task_id: TaskId,
    pub owner: String,             // 哪个 agent 持租
    pub granted_at_ms: i64,
    pub expires_at_ms: i64,
    pub generation: u64,           // 防止 ABA: 每次 acquire 自增
}
```

`generation` 字段 — 防止 ABA 问题: 同一 `TaskId` 先 release 又被新 agent acquire 时, 旧 owner 的过期 guard drop 不会误 release 新租约.

### 3.3 状态机转换表

| From | To | Trigger | 合法? |
|------|----|---------|------|
| Ready | Leased | `try_acquire` | ✓ |
| Leased | Running | (caller 显式调用, 未来扩展) | ✓ |
| Leased | Ready | `release` (未跑) | ✓ |
| Running | Completed | `release` (成功) | ✓ |
| Running | Failed | `reap_expired` (到期) | ✓ |
| Running | Ready | — | ✗ **FORBIDDEN** |
| Ready | Failed | (不允许: 没跑过的任务不会 reap) | ✗ |

> 注: 当前的 `InMemoryLeaseManager` 实现把"task state 是否在 Running"留给调用方判断; reap 只返回到期 task_id, 由 orchestrator 决定怎么 mark Failed. 这是**有意的松耦合**, 见 §5 "不假装" 段.

### 3.4 LeaseManager trait

```rust
pub trait LeaseManager: Send + Sync {
    fn try_acquire(&self, task_id: TaskId, owner: &str, ttl_ms: i64) -> Result<LeaseGuard, LeaseError>;
    fn release(&self, lease: TaskLease) -> Result<(), LeaseError>;
    fn reap_expired(&self, now_ms: i64) -> Vec<TaskId>;
    fn active_leases(&self) -> Vec<TaskLease>;
}
```

`LeaseGuard` 是 RAII handle — `Drop` impl 自动调用 `release` (除非已显式 consumed), 见 §3.5.

### 3.5 LeaseGuard — RAII 自动释放

```rust
pub struct LeaseGuard {
    manager: Arc<dyn LeaseManager>,
    lease: Option<TaskLease>,     // None = 已显式 release, drop 时不再 release
    released: bool,
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.take() {
            if !self.released {
                let _ = self.manager.release(lease);
            }
        }
    }
}
```

## 4. 选址决策

**选址**: `apeireth-team-lead::lease` 子模块 (而非独立 `apeireth-lease` crate).

**理由**:
- Lease 是 Orchestrator 调度机制的内部组件, 跟 `Orchestrator` trait 紧耦合
- 其他 crate (governance / arbitration / runtime) 目前**没有** lease 需求
- 后续如有跨 crate 需求, `pub use lease::LeaseManager` 即可暴露 — 零迁移成本
- 新独立 crate 会强制 apeireth-team-lead → apeireth-lease 的反向依赖, 加 depth 无价值

**借鉴 ID**: `BORROW-Jimmyxiao2009/AgentFlow-task-dag-lease-2026-08-21`

## 5. 测试矩阵 (8 项)

| # | 测试名 | 验证目标 |
|---|--------|---------|
| 1 | `acquire_then_release_state_goes_ready_again` | Ready → Leased → Ready (未跑释放) |
| 2 | `acquire_then_run_then_complete_full_happy_path` | Ready → Leased → Running → Completed |
| 3 | `reap_expired_marks_task_failed_not_ready` | 到期 → reap 返回 task_id, 验证 Running → Failed (非 Ready) — **关键不变量** |
| 4 | `running_to_ready_is_forbidden_runtime_check` | reap 不允许返回 Ready 状态, 错误信息指明 Failed |
| 5 | `reap_expired_only_affects_running_leases` | 未到期 lease 不被 reap |
| 6 | `concurrent_acquire_on_same_task_only_one_wins` | 双重分配防 ABA |
| 7 | `lease_guard_drop_auto_releases` | RAII — guard drop 后 lease 不再 active |
| 8 | `guard_release_then_drop_does_not_double_release` | 显式 release 后 drop 是 no-op |

## 6. 不假装 (O-5) — 明确"什么没做"

以下行为**有意不做**, 留作后续 P1:

| 没做 | 为什么 | 后续 P1 |
|------|-------|--------|
| 持久化 lease | 重启后所有 in-memory lease 丢失 | 用 sled/sqlite WAL 持久化 (TaskLease → lease_table) |
| reap_expired 自动改 state | reap 只返回 task_id 列表, 由 caller 决定标 Failed/Retry | 集成到 Orchestrator reap loop, 自动 transition TaskState |
| async API | 当前用 `std::sync::Mutex` + sync trait, 阻塞 | 改成 `tokio::sync::Mutex` + async trait (需 Orchestrator 一起改) |
| Generation stamp 自动检查 | generation 字段已存, 但 release 时未强制 match | 加 check: `release` 时校验传入 generation == active generation |
| Clock injection | 使用 `std::time::SystemTime` 而非可注入 clock | 加 `Clock` trait, 测试用 mock clock |
| Task DAG 拓扑感知 | 当前只按 TaskId 维度管理, 不感知 task 间依赖 | 集成 apeireth-graph DAG, reap Failed 时 cascade mark downstream |
| Leased → Running 自动转换 | 当前 caller 需手动标, 没有 `try_start(task_id)` | 后续加 `try_acquire_with_start()` |

## 7. 集成面 (本次未做, 但已 ready)

- `Orchestrator::spawn_agent` 路径可立即接入 lease: spawn 时 `try_acquire`, agent 完成时 `release`
- `wait_agent_idle` 的 timeout (现占位 `let _ = timeout_ms;` per audit) 可作为 reap 触发器 — 每 timeout_ms 检查一次 `reap_expired(now)`
- Scheduler 后台循环 (per audit 缺) 可用 `parking_lot::Mutex<InMemoryLeaseManager>` 实例, 每分钟调一次 `reap_expired`

## 8. 借鉴合规

| 项 | 值 |
|----|----|
| 借鉴仓库 | Jimmyxiao2009/AgentFlow |
| License | ⚠️ 无 LICENSE (默认 all-rights-reserved) |
| 借鉴方式 | **设计思想 + 字段级 API 形状** (TaskState 6 状态 / 状态机不变量 / lease+timeout+generation 三件套) |
| 代码复制 | **0 行** — 全部 Rust 重写, 用 std::sync + parking_lot (而非 AgentFlow 用的第三方 lock lib) |
| 借鉴 ID | `BORROW-Jimmyxiao2009/AgentFlow-task-dag-lease-2026-08-21` |
| 字段级移植对应 | AgentFlow `TaskState.{Pending,Rented,Running,Failed,Canceled,Done}` → Rust `TaskState.{Ready,Leased,Running,Failed,Cancelled,Completed}` (改名仅取更标准术语, 字段含义 1:1) |

## 9. 参考文献

- `crates/apeireth-team-lead/src/lib.rs` — Team Lead 主模块 (anchor for lease integration)
- `docs/04-internal/borrow-from-jimmyxiao2009.md` — 借鉴合规总册
- R215 audit — `team-lead/lib.rs:505-506 let _ = timeout_ms; // 占位` (本次落地点之一)
- `_research_mem/AgentFlow-analysis.md` — AgentFlow 分析报告 (65 KB, 借鉴源头)