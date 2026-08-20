//! # `apeireth-rate-limiter`
//!
//! Apeireth 专用 rate limiter (R20 阶段 6 估补, **专用 rate limiter, 比 apeireth-constraint 简单**,
//! 1:1 翻译 v0.9.21 `@anthropic-ai/rate-limiter` 商业版)。
//!
//! ## 8 哲学锚穿透 (per APEIRETH-CONVENTIONS §9, R125 B5 升 6→8, R126 P1-2 实施)
//!
//! 1. **S-1 主 22:33 — 北极星导向**: 服务 ASI 北极星, rate limiter 是「主对话 / Tool call / LLM 调用」
//!    的限流基础设施, 直接影响 1.0 release 12 项 checklist #7 稳定性
//! 2. **S-2 主 17:43 — 实事求是**: 1:1 翻译商业版 API surface, 不重写已有算法理论
//! 3. **O-5 主 17:58 — 不假装**: 4 算法 + 5 storage, in-memory 完整, 4 storage 全部 stub
//!    (返 `NotImplemented`, 留 R21+ 续真接), 0 假装 Redis 已接
//! 4. **O-2 主 19:33 — 走在前人经验上**: token / leaky / fixed / sliding 4 大经典算法
//!    直接对照 stripe/ratelimit/governor 等开源实现, 选 1:1 翻译商业版
//! 5. **O-3 主 23:44 — 干到底**: API / 4 算法 / 5 storage / 30+ 测试 / example 全在一 PR 一次落地
//! 6. **O-4 主 00:56 — 任何人都能接手**: 模块边界 (token_bucket / leaky_bucket / fixed_window /
//!    sliding_window / storage / error / config) 清晰, 每模块可独立 review
//! 7. **S-3 主 — 质量工程化**: retry/backoff 借鉴 3 限流重试 (LiteLLM full-jitter + opencode agent
//!    retry + Guardrails action policy) → `retry` 模块, 0 装严守从公开文档借鉴, 0 假装 git clone 上游
//! 8. **O-1 主 — 安全优先**: `RetryAfter` 解析尊重 server 给的 429/503 (借鉴 Guardrails), 0 假装
//!    client 端算 backoff 就能覆盖 server 限流 — server 说等多久就等多久
//!
//! ## 8 项不修改承诺 (per task spec §10)
//!
//! 1. **0 真接商业版 `@anthropic-ai/rate-limiter` SDK** — 4 算法全自实现, 不引外部
//! 2. **0 触碰 24 LOCKED crate** — 仅新建本 crate, 不改任何已有 src/
//! 3. **0 改 workspace version** — `version.workspace = true`, 不写裸版本号
//! 4. **0 改 workspace Cargo.toml** — 本 crate 不加进 members 列表
//!    (硬约束; 验收改用 `cargo check` / `cargo test` 从 crate 目录执行)
//! 5. **0 改任何已有 crate** — 仅本 crate 新增文件
//! 6. **8 工具白名单** — rate limiter 不暴露工具调用接口, 概念省略
//!    (RateLimiter trait 是 4 个核心方法, 不是 8 工具)
//! 7. **5 K-1 强校验** — rate / burst / window_size / slide_interval / max_wait 均 > 0
//! 8. **0 主动 commit** — 文件落到主仓路径, **不** `git commit`
//!
//! ## 模块布局
//!
//! - [`token_bucket`]: 令牌桶 (5 参数, lazy refill)
//! - [`leaky_bucket`]: 漏桶 (4 参数, Drop/Block 溢出)
//! - [`fixed_window`]: 固定窗口 (3 参数, 已知边界突刺)
//! - [`sliding_window`]: 滑窗 (4 参数, Log/Counter 双精度)
//! - [`storage`]: 5 storage (in-memory 完整 + 4 stub)
//! - [`error`]: 9 种错误
//! - [`config`]: 4 段配置 (bucket / strategy / storage / observability)
//! - [`retry`]: retry / backoff 策略 (借鉴 3 限流重试: LiteLLM full-jitter + opencode + Guardrails action policy)
//!
//! ## 快速开始
//!
//! ```no_run
//! use apeireth_rate_limiter::{RateLimiter, RateLimiterImpl, RateLimiterConfig, BucketConfig,
//!     StrategyConfig, StrategyKind, StorageConfig, StorageKind, ObservabilityConfig};
//! use std::time::Duration;
//!
//! # async fn demo() {
//! let cfg = RateLimiterConfig {
//!     bucket: BucketConfig {
//!         rate_per_second: 10.0,
//!         burst: 20,
//!         initial_tokens: None,
//!         max_wait: Some(Duration::from_secs(5)),
//!         refill_interval: Duration::from_millis(100),
//!     },
//!     strategy: StrategyConfig {
//!         kind: StrategyKind::TokenBucket,
//!         window_size: None,
//!         slide_interval: None,
//!         max_requests: None,
//!         precision: None,
//!         overflow_policy: None,
//!         reset_strategy: None,
//!     },
//!     storage: StorageConfig::default(),
//!     observability: ObservabilityConfig::default(),
//! };
//!
//! let limiter = RateLimiterImpl::new(cfg).unwrap();
//! assert!(limiter.try_acquire("user:42", 1).await.unwrap());
//! let _permit = limiter.acquire("user:42", 5).await.unwrap();
//! // permit drop 时自动 release 5 tokens
//! let _stats = limiter.stats().await;
//! # }
//! ```

#![doc(html_root_url = "https://docs.rs/apeireth-rate-limiter/1.0.0")]
#![deny(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub mod config;
// R177: organ invariants (5 tests + 2 Kani)
pub mod error;
pub mod fixed_window;
pub mod leaky_bucket;
mod organ_kani_proofs;
pub mod retry; // 2026-08-19 借鉴 3 限流重试: LiteLLM full-jitter + opencode + Guardrails action policy
pub mod sliding_window;
pub mod storage;
pub mod token_bucket;

// =====================================================================
// Re-exports — 一站式 import
// =====================================================================

pub use crate::config::{
    BucketConfig, FixedWindowReset, LeakyBucketOverflow, ObservabilityConfig, RateLimiterConfig,
    SlidingWindowPrecision, StorageConfig, StorageKind, StrategyConfig, StrategyKind,
};
pub use crate::error::{RateLimiterError, Result};
pub use crate::fixed_window::FixedWindow;
pub use crate::leaky_bucket::LeakyBucket;
pub use crate::retry::{
    Backoff, ConstantBackoff, ExponentialBackoff, RetryAfter, RetryOutcome, StopReason,
};
pub use crate::sliding_window::SlidingWindow;
pub use crate::storage::{
    build_storage, DistributedStorage, FileStorage, InMemoryStorage, MemcachedStorage,
    RedisStorage, Storage,
};
pub use crate::token_bucket::TokenBucket;

// =====================================================================
// RateLimiter trait — 4 个核心方法
// =====================================================================

/// 通用 rate limiter 接口。
///
/// 4 个方法, 跟 v0.9.21 `@anthropic-ai/rate-limiter` 商业版 `RateLimiter` 接口 1:1 翻译:
///
/// - `try_acquire` — 非阻塞尝试, 立即返 bool
/// - `acquire` — 阻塞直到拿到 permit, 返 RAII guard
/// - `reset` — 清空某 key 的状态
/// - `stats` — 读全局命中 / 未命中 / 等待统计
///
/// **实现**: [`RateLimiterImpl`] 是唯一的具体实现, 其他 crate 借用此 trait 注入限流。
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// 非阻塞尝试扣 `cost` 个单位。返 `Ok(true)` = 成功, `Ok(false)` = 拒绝。
    async fn try_acquire(&self, key: &str, cost: u32) -> Result<bool>;

    /// 阻塞等待直到拿到 `cost` 个单位, 返 RAII permit。permit drop 时自动 release。
    async fn acquire(&self, key: &str, cost: u32) -> Result<AcquiredPermit>;

    /// 重置某 key 的状态（清空令牌 / 计数 / 时间戳）。
    async fn reset(&self, key: &str) -> Result<()>;

    /// 读全局统计。
    async fn stats(&self) -> RateLimiterStats;
}

// =====================================================================
// AcquiredPermit — RAII guard
// =====================================================================

/// `acquire` 返回的 RAII guard, drop 时自动把 `cost` 退回桶。
///
/// **设计要点**:
/// - 内部 `Option<Arc<InnerState>>` — `take()` 后变 None, 避免 double-release
/// - drop 直接在 `Arc<InnerState>` 上 release (parking_lot, 不需 .await)
/// - permit 可显式 `forget()` 跳过 release（不推荐, 但有时需要）
/// - "oversized cost" 测试: 持大 cost permit, drop 后桶回收完整 cost
pub struct AcquiredPermit {
    inner: Option<Arc<InnerState>>,
    key: String,
    cost: u32,
    acquired_at: Instant,
}

impl AcquiredPermit {
    /// 显式构造（仅 crate 内部使用, 外部通过 `RateLimiter::acquire` 获取）。
    pub(crate) fn new(inner: Arc<InnerState>, key: String, cost: u32) -> Self {
        Self {
            inner: Some(inner),
            key,
            cost,
            acquired_at: Instant::now(),
        }
    }

    /// 拿 permit 的 cost（拿到时扣了多少）。
    pub fn cost(&self) -> u32 {
        self.cost
    }

    /// permit 拿到后过了多久。
    pub fn held_for(&self) -> Duration {
        self.acquired_at.elapsed()
    }

    /// 关联的 key。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 显式放弃 permit（不 release, 给上层「我就看看不消耗」语义用）。
    pub fn forget(mut self) {
        // 阻止 Drop 跑 release
        self.inner.take();
    }
}

impl Drop for AcquiredPermit {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            // 同步 release（parking_lot, 不需 async runtime）— 直接锁 inner.state
            let mut map = inner.state.lock();
            if let Some(state) = map.get_mut(&self.key) {
                state.release(self.cost);
            }
        }
    }
}

impl std::fmt::Debug for AcquiredPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcquiredPermit")
            .field("key", &self.key)
            .field("cost", &self.cost)
            .field("held_for_ms", &self.held_for().as_millis())
            .finish()
    }
}

// =====================================================================
// RateLimiterStats — 全局统计
// =====================================================================

/// 全局限流统计。
///
/// 5 字段: 总尝试 / 命中 / 未命中 / 总等待时长 / 当前跟踪 key 数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimiterStats {
    /// `try_acquire` + `acquire` 总调用次数。
    pub total_attempts: u64,
    /// 成功扣减的次数。
    pub hits: u64,
    /// 拒绝的次数（`try_acquire` 返 false）。
    pub misses: u64,
    /// 累计等待时长（`acquire` 阻塞的总和）。
    pub total_wait: Duration,
    /// 当前 in-memory 跟踪的 key 数。
    pub tracked_keys: usize,
}

impl RateLimiterStats {
    /// 命中率（hits / attempts）, 0 attempts 时返 0.0。
    pub fn hit_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_attempts as f64
        }
    }

    /// 平均等待时长。
    pub fn avg_wait(&self) -> Duration {
        if self.hits == 0 {
            Duration::ZERO
        } else {
            self.total_wait / self.hits as u32
        }
    }
}

// =====================================================================
// PerKeyState — 4 算法的枚举分发
// =====================================================================

/// 单 key 状态 — 4 种算法共用枚举。
///
/// `try_acquire` / `release` / `reset` 都根据 `kind` 分发到具体算法。
#[derive(Debug)]
pub(crate) enum PerKeyState {
    /// 令牌桶。
    Token(TokenBucket),
    /// 漏桶。
    Leaky(LeakyBucket),
    /// 固定窗口。
    Fixed(FixedWindow),
    /// 滑窗。
    Sliding(SlidingWindow),
}

impl PerKeyState {
    /// 非阻塞尝试扣 `cost` 个单位。
    pub(crate) fn try_acquire(&mut self, cost: u32) -> bool {
        match self {
            PerKeyState::Token(b) => b.try_acquire(cost),
            PerKeyState::Leaky(b) => b.try_acquire(cost),
            PerKeyState::Fixed(w) => w.try_acquire(),
            PerKeyState::Sliding(w) => w.try_acquire(),
        }
    }

    /// permit drop 时调用 — 退回 `cost` 个单位。
    ///
    /// **注意**: fixed / sliding 没有"释放名额"语义（窗口计数一旦 +1 不可逆）,
    /// 仅 token / leaky 真正回退。
    pub(crate) fn release(&mut self, cost: u32) {
        match self {
            PerKeyState::Token(b) => b.release(cost),
            PerKeyState::Leaky(b) => b.release(cost),
            PerKeyState::Fixed(_) | PerKeyState::Sliding(_) => {
                // fixed / sliding 不可逆, 静默忽略
            }
        }
    }

    /// 重置 key 状态。
    pub(crate) fn reset(&mut self) {
        match self {
            PerKeyState::Token(b) => b.reset(),
            PerKeyState::Leaky(b) => b.reset(),
            PerKeyState::Fixed(w) => w.reset(),
            PerKeyState::Sliding(w) => w.reset(),
        }
    }
}

// =====================================================================
// RateLimiterImpl — 主具体实现
// =====================================================================

/// Rate limiter 主实现。
///
/// **设计**:
/// - 持有 config + storage + `Arc<InnerState>`(per-key map + stats)
/// - in-memory 模式下 per-key state 存 `InnerState.state`
/// - stub 模式下 storage 本身返 NotImplemented, 状态 map 不会增长
/// - `Arc<InnerState>` 让 `AcquiredPermit` drop 时能 release 到原桶
/// - parking_lot::Mutex 保护 state map（同步, 极快）
/// - stats 用 AtomicU64 (lock-free)
pub struct RateLimiterImpl {
    /// 不可变配置 + storage 派发。
    config: RateLimiterConfig,
    /// 共享 inner state — 持有 `Arc`, 让 permit 也能 release 到同一桶。
    inner: Arc<InnerState>,
    /// storage 后端。
    storage: Box<dyn Storage>,
    /// 构造时刻 — 用于 stats 报告 uptime。
    created_at: Instant,
}

/// 共享 inner state — per-key map + stats。
///
/// 抽出来用 `Arc` 是为了让 `AcquiredPermit` 也能 `release` 到原桶。
pub(crate) struct InnerState {
    /// per-key state — in-memory 模式下用, stub 模式下永不增长
    state: Mutex<HashMap<String, PerKeyState>>,
    /// 命中数（lock-free atomic）。
    hits: std::sync::atomic::AtomicU64,
    /// 未命中数。
    misses: std::sync::atomic::AtomicU64,
    /// 总尝试数。
    total_attempts: std::sync::atomic::AtomicU64,
    /// 总等待时长（毫秒, atomic u64, 简化不纳秒精度）。
    total_wait_ms: std::sync::atomic::AtomicU64,
}

impl InnerState {
    fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
            total_attempts: std::sync::atomic::AtomicU64::new(0),
            total_wait_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl RateLimiterImpl {
    /// 新建 RateLimiter。
    ///
    /// 失败: config 校验失败 / in-memory 初始化错误。
    pub fn new(config: RateLimiterConfig) -> Result<Self> {
        // 校验 bucket 配置（rate / burst / refill_interval > 0）
        config.bucket.validate()?;

        // 校验 strategy 专属参数
        if let Some(ws) = config.strategy.window_size {
            if ws.is_zero() {
                return Err(RateLimiterError::ZeroWindowSize);
            }
        }
        if let Some(si) = config.strategy.slide_interval {
            if si.is_zero() {
                return Err(RateLimiterError::InvalidParameter(
                    "slide_interval must be > 0".to_string(),
                ));
            }
        }
        if let Some(mr) = config.strategy.max_requests {
            if mr == 0 {
                return Err(RateLimiterError::InvalidParameter(
                    "max_requests must be > 0".to_string(),
                ));
            }
        }

        let storage = build_storage(&config.storage);
        Ok(Self {
            config,
            inner: Arc::new(InnerState::new()),
            storage,
            created_at: Instant::now(),
        })
    }

    /// 读 config（不可变借用）。
    pub fn config(&self) -> &RateLimiterConfig {
        &self.config
    }

    /// 当前策略种类。
    pub fn strategy_kind(&self) -> StrategyKind {
        self.config.strategy.kind
    }

    /// 当前存储后端种类。
    pub fn storage_kind(&self) -> StorageKind {
        self.config.storage.kind
    }

    /// 自构造以来运行时长。
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// 当前跟踪的 key 数。
    pub fn tracked_keys(&self) -> usize {
        self.inner.state.lock().len()
    }

    /// 拿或建某 key 的 state, 然后用闭包操作。
    fn with_state_or_create<R>(
        &self,
        key: &str,
        f: impl FnOnce(&mut PerKeyState) -> R,
    ) -> Result<R> {
        let mut map = self.inner.state.lock();
        if !map.contains_key(key) {
            let new_state = self.create_state()?;
            map.insert(key.to_string(), new_state);
        }
        let state = map.get_mut(key).expect("just inserted or already present");
        Ok(f(state))
    }

    /// 按当前 strategy 创建一个新 PerKeyState。
    fn create_state(&self) -> Result<PerKeyState> {
        let bucket = &self.config.bucket;
        let strategy = &self.config.strategy;
        match strategy.kind {
            StrategyKind::TokenBucket => {
                let tb = TokenBucket::new(bucket)?;
                Ok(PerKeyState::Token(tb))
            }
            StrategyKind::LeakyBucket => {
                let overflow = strategy
                    .overflow_policy
                    .unwrap_or(LeakyBucketOverflow::Drop);
                let lb = LeakyBucket::new(bucket, overflow)?;
                Ok(PerKeyState::Leaky(lb))
            }
            StrategyKind::FixedWindow => {
                let ws = strategy.window_size.ok_or_else(|| {
                    RateLimiterError::InvalidParameter("FixedWindow needs window_size".to_string())
                })?;
                let mr = strategy.max_requests.ok_or_else(|| {
                    RateLimiterError::InvalidParameter("FixedWindow needs max_requests".to_string())
                })?;
                let reset = strategy
                    .reset_strategy
                    .unwrap_or(FixedWindowReset::OnWindowEnd);
                let fw = FixedWindow::new(ws, mr, reset)?;
                Ok(PerKeyState::Fixed(fw))
            }
            StrategyKind::SlidingWindow => {
                let ws = strategy.window_size.ok_or_else(|| {
                    RateLimiterError::InvalidParameter(
                        "SlidingWindow needs window_size".to_string(),
                    )
                })?;
                let mr = strategy.max_requests.ok_or_else(|| {
                    RateLimiterError::InvalidParameter(
                        "SlidingWindow needs max_requests".to_string(),
                    )
                })?;
                let si = strategy.slide_interval.unwrap_or(Duration::ZERO);
                let precision = strategy.precision.unwrap_or(SlidingWindowPrecision::Log);
                let sw = SlidingWindow::new(ws, si, mr, precision)?;
                Ok(PerKeyState::Sliding(sw))
            }
        }
    }
}

#[async_trait]
impl RateLimiter for RateLimiterImpl {
    async fn try_acquire(&self, key: &str, cost: u32) -> Result<bool> {
        // 1. 委托 storage（非 in-memory 直接返 NotImplemented）
        if self.config.storage.kind != StorageKind::InMemory {
            return self.storage.get(key).await.map(|_| false).map_err(|e| e);
        }

        // 2. 更新 stats
        self.inner
            .total_attempts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // 3. 拿或建 state, 试 acquire
        let result = self.with_state_or_create(key, |s| s.try_acquire(cost));

        // 4. 更新 hits / misses
        match &result {
            Ok(true) => {
                self.inner
                    .hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(false) => {
                self.inner
                    .misses
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(_) => {}
        }

        result
    }

    async fn acquire(&self, key: &str, cost: u32) -> Result<AcquiredPermit> {
        // 1. 委托 storage
        if self.config.storage.kind != StorageKind::InMemory {
            return Err(RateLimiterError::NotImplemented(format!(
                "acquire via {} storage — R21+ 续真接",
                self.config.storage.kind_label()
            )));
        }

        let start = Instant::now();
        self.inner
            .total_attempts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // 2. 阻塞等令牌
        let max_wait = self
            .config
            .bucket
            .max_wait
            .unwrap_or(Duration::from_secs(5));
        let acquired = self.acquire_blocking(key, cost, max_wait).await?;

        // 3. 记录 wait 时长
        let wait = start.elapsed();
        self.inner.total_wait_ms.fetch_add(
            wait.as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        if acquired {
            self.inner
                .hits
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.inner
                .misses
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        if !acquired {
            return Err(RateLimiterError::MaxWaitExceeded {
                key: key.to_string(),
            });
        }

        // 4. 返 RAII permit, 共享同一份 InnerState (Arc clone)
        Ok(AcquiredPermit::new(
            Arc::clone(&self.inner),
            key.to_string(),
            cost,
        ))
    }

    async fn reset(&self, key: &str) -> Result<()> {
        // 1. 委托 storage
        if self.config.storage.kind != StorageKind::InMemory {
            return self.storage.delete(key).await;
        }

        // 2. 从 state map 移除（in-place）
        let mut map = self.inner.state.lock();
        map.remove(key);
        Ok(())
    }

    async fn stats(&self) -> RateLimiterStats {
        RateLimiterStats {
            total_attempts: self
                .inner
                .total_attempts
                .load(std::sync::atomic::Ordering::Relaxed),
            hits: self.inner.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: self.inner.misses.load(std::sync::atomic::Ordering::Relaxed),
            total_wait: Duration::from_millis(
                self.inner
                    .total_wait_ms
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            tracked_keys: self.inner.state.lock().len(),
        }
    }
}

// =====================================================================
// RateLimiterImpl — 内部辅助方法
// =====================================================================

impl RateLimiterImpl {
    /// 阻塞 acquire — 在 max_wait 内循环 try + sleep。
    async fn acquire_blocking(&self, key: &str, cost: u32, max_wait: Duration) -> Result<bool> {
        let start = Instant::now();
        // 步长不能比 max_wait 大, 否则 sleep 完已经超时
        // 用 max_wait/4 保证至少 4 次唤醒机会
        let refill_step = self
            .config
            .bucket
            .refill_interval
            .max(Duration::from_millis(1));
        let step = refill_step.min(max_wait.div_f64(4.0).max(Duration::from_millis(1)));
        loop {
            // 先试一次
            let acquired = self.with_state_or_create(key, |s| s.try_acquire(cost))?;
            if acquired {
                return Ok(true);
            }
            if start.elapsed() >= max_wait {
                return Ok(false);
            }
            // 看 strategy 是否要等（leaky block / token 默认等）
            let should_block = match self.config.strategy.kind {
                StrategyKind::LeakyBucket => {
                    self.config
                        .strategy
                        .overflow_policy
                        .unwrap_or(LeakyBucketOverflow::Drop)
                        == LeakyBucketOverflow::Block
                }
                _ => true,
            };
            if !should_block {
                return Ok(false);
            }
            // sleep 完再 check, 避免一次 sleep 跨过 max_wait
            let sleep_for = step.min(max_wait.saturating_sub(start.elapsed()));
            if sleep_for.is_zero() {
                return Ok(false);
            }
            tokio::time::sleep(sleep_for).await;
        }
    }
}

// =====================================================================
// StorageConfig 扩展方法 — kind_label 给 stats / 错误用
// =====================================================================

impl StorageConfig {
    /// 人类可读 kind 名（给错误消息 / 日志用）。
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            StorageKind::InMemory => "InMemory",
            StorageKind::Redis => "Redis",
            StorageKind::Memcached => "Memcached",
            StorageKind::File => "File",
            StorageKind::Distributed => "Distributed",
        }
    }
}

// =====================================================================
// 便捷构造器 — 4 算法各一个
// =====================================================================

/// 便捷构造: 令牌桶 + in-memory。
pub fn token_bucket_in_memory(
    rate_per_second: f64,
    burst: u32,
    max_wait: Option<Duration>,
) -> Result<RateLimiterImpl> {
    let cfg = RateLimiterConfig {
        bucket: BucketConfig {
            rate_per_second,
            burst,
            initial_tokens: None,
            max_wait,
            refill_interval: Duration::from_millis(100),
        },
        strategy: StrategyConfig {
            kind: StrategyKind::TokenBucket,
            ..Default::default()
        },
        storage: StorageConfig::default(),
        observability: ObservabilityConfig::default(),
    };
    RateLimiterImpl::new(cfg)
}

/// 便捷构造: 漏桶 + in-memory。
pub fn leaky_bucket_in_memory(
    rate_per_second: f64,
    capacity: u32,
    overflow: LeakyBucketOverflow,
) -> Result<RateLimiterImpl> {
    let cfg = RateLimiterConfig {
        bucket: BucketConfig {
            rate_per_second,
            burst: capacity,
            initial_tokens: Some(0),
            max_wait: Some(Duration::from_secs(5)),
            refill_interval: Duration::from_millis(100),
        },
        strategy: StrategyConfig {
            kind: StrategyKind::LeakyBucket,
            overflow_policy: Some(overflow),
            ..Default::default()
        },
        storage: StorageConfig::default(),
        observability: ObservabilityConfig::default(),
    };
    RateLimiterImpl::new(cfg)
}

/// 便捷构造: 固定窗口 + in-memory。
pub fn fixed_window_in_memory(window_size: Duration, max_requests: u32) -> Result<RateLimiterImpl> {
    let cfg = RateLimiterConfig {
        bucket: BucketConfig {
            // 桶配置对 fixed window 不重要, 但 rate 必须 > 0
            rate_per_second: 1.0,
            burst: 1,
            initial_tokens: Some(0),
            max_wait: None,
            refill_interval: Duration::from_millis(100),
        },
        strategy: StrategyConfig {
            kind: StrategyKind::FixedWindow,
            window_size: Some(window_size),
            max_requests: Some(max_requests),
            reset_strategy: Some(FixedWindowReset::OnWindowEnd),
            ..Default::default()
        },
        storage: StorageConfig::default(),
        observability: ObservabilityConfig::default(),
    };
    RateLimiterImpl::new(cfg)
}

/// 便捷构造: 滑窗 + in-memory。
pub fn sliding_window_in_memory(
    window_size: Duration,
    max_requests: u32,
    precision: SlidingWindowPrecision,
) -> Result<RateLimiterImpl> {
    let cfg = RateLimiterConfig {
        bucket: BucketConfig {
            rate_per_second: 1.0,
            burst: 1,
            initial_tokens: Some(0),
            max_wait: None,
            refill_interval: Duration::from_millis(100),
        },
        strategy: StrategyConfig {
            kind: StrategyKind::SlidingWindow,
            window_size: Some(window_size),
            slide_interval: Some(Duration::from_millis(50)),
            max_requests: Some(max_requests),
            precision: Some(precision),
            ..Default::default()
        },
        storage: StorageConfig::default(),
        observability: ObservabilityConfig::default(),
    };
    RateLimiterImpl::new(cfg)
}

// =====================================================================
// Lib 级单元测试 — 5 个 quick check
// =====================================================================

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[tokio::test]
    async fn token_bucket_constructor_basic() {
        let l = token_bucket_in_memory(10.0, 20, None).unwrap();
        assert!(l.try_acquire("k", 1).await.unwrap());
    }

    #[tokio::test]
    async fn leaky_bucket_constructor_basic() {
        let l = leaky_bucket_in_memory(10.0, 5, LeakyBucketOverflow::Drop).unwrap();
        for _ in 0..5 {
            assert!(l.try_acquire("k", 1).await.unwrap());
        }
        assert!(!l.try_acquire("k", 1).await.unwrap());
    }

    #[tokio::test]
    async fn fixed_window_constructor_basic() {
        let l = fixed_window_in_memory(Duration::from_secs(10), 3).unwrap();
        assert!(l.try_acquire("k", 1).await.unwrap());
        assert!(l.try_acquire("k", 1).await.unwrap());
        assert!(l.try_acquire("k", 1).await.unwrap());
        assert!(!l.try_acquire("k", 1).await.unwrap());
    }

    #[tokio::test]
    async fn sliding_window_log_basic() {
        let l = sliding_window_in_memory(Duration::from_secs(10), 3, SlidingWindowPrecision::Log)
            .unwrap();
        for _ in 0..3 {
            assert!(l.try_acquire("k", 1).await.unwrap());
        }
        assert!(!l.try_acquire("k", 1).await.unwrap());
    }

    #[tokio::test]
    async fn rate_limiter_impl_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RateLimiterImpl>();
    }
}
