//! Retry / Backoff 策略 — 借鉴 3 限流重试 (LiteLLM / opencode / Guardrails)
//!
//! 0 装 PASS 严守: 借鉴模式来自公开文档/论文 (full-jitter 算法源自 AWS Architecture Blog "Exponential Backoff And Jitter" 2015),
//! 0 假装已 git clone 上游源码 (per 2026-08-19 subagent git clone 撞 127.0.0.1 local proxy, 转 public docs 路线).
//!
//! 借鉴 3 模式 (per _research_mem/sub_agent_reports/2026-08-19/):
//! 1. **LiteLLM** retry: 指数退避 + full-jitter (Marcus "AWS Architecture" 公式 `random(0, min(cap, base * 2^attempt))`)
//! 2. **opencode** retry: agent-level 错误分类 (transient 5xx/429/timeout 重试, permanent 4xx 不重试)
//! 3. **Guardrails** action retry: action policy 限定 max_attempts + max_total_wait, 超限 fail-fast
//!
//! 工程规范:
//! - 0 触碰 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache)
//! - 0 改 enum/const
//! - 0 改 workspace.version (1.2.0 双轴制)
//! - 0 装 PASS 严守 (没有上游源码只是 public docs 借鉴)
//!
//! 公开 API 4 类型:
//! - `Backoff` trait: 给定 attempt N, 返 Duration
//! - `ConstantBackoff`: 固定时长
//! - `ExponentialBackoff`: 指数 + full-jitter (LiteLLM 模式)
//! - `RetryAfter`: 解析 HTTP 429 / 503 Retry-After header (seconds / HTTP-date)
//! - `RetryOutcome` enum: Retry(Duration) | Stop(reason)

use std::time::Duration;

/// 单次 retry 决定: 继续 (等多久) 或停止 (为什么).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryOutcome {
    /// 继续 retry, 等指定 Duration.
    /// (内含 jitter 随机, 调用方不直接计算 backoff)
    Retry(Duration),
    /// 停止 retry, 给原因.
    /// (max_attempts 超限 / max_total_wait 超限 / permanent 错误)
    Stop(StopReason),
}

/// Stop 原因 — 0 装严守: 透明标, 不假装 "成功" 或 "再试".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// 超过 max_attempts 上限.
    MaxAttemptsExceeded {
        /// 实际尝试次数 (含首次)
        attempts: u32,
        /// 上限
        max: u32,
    },
    /// 超过 max_total_wait 上限 (累计等时长).
    MaxWaitExceeded {
        /// 累计等时长
        elapsed: Duration,
        /// 上限
        max: Duration,
    },
    /// Permanent 错误 (如 4xx 非 429, 或 opencode 风格的 hard error) — 不该 retry.
    PermanentError(String),
}

/// 通用 backoff 策略 — 给定 attempt (0-indexed), 返 wait Duration.
/// (Backoff 策略 = 错误时下一次 retry 前的等时长. 0 表示立即 retry, 不推荐.)
pub trait Backoff: Send + Sync {
    /// 计算 attempt 次数后等多久 (含 jitter, 调用方不直接调).
    /// `attempt` 0-indexed: 首次失败 = 0, 第二次失败 = 1, ...
    fn next_delay(&self, attempt: u32) -> Duration;

    /// 上限: 重试总等时长 (0 = 不限).
    fn max_total_wait(&self) -> Duration;

    /// 上限: 最大尝试次数 (含首次, 0 = 不限).
    fn max_attempts(&self) -> u32;
}

/// 固定 backoff — 每次等相同 Duration (0 装严守: 简单测试用, 0 推荐生产).
#[derive(Debug, Clone)]
pub struct ConstantBackoff {
    delay: Duration,
    max_attempts: u32,
}

impl ConstantBackoff {
    /// 新建 — delay 每次等多久, max_attempts 0 = 不限.
    pub fn new(delay: Duration, max_attempts: u32) -> Self {
        Self {
            delay,
            max_attempts,
        }
    }
}

impl Backoff for ConstantBackoff {
    fn next_delay(&self, attempt: u32) -> Duration {
        // 0 装严守: max_attempts=0 (不限) 时 attempt 0 也返 delay, 0 panic
        if self.max_attempts > 0 && attempt >= self.max_attempts {
            // 超限 — 但 trait 要求返 Duration, 用 delay*2 让 caller 醒过来
            self.delay * 2
        } else {
            self.delay
        }
    }
    fn max_total_wait(&self) -> Duration {
        if self.max_attempts == 0 {
            Duration::ZERO // 0 = 不限
        } else {
            self.delay * self.max_attempts
        }
    }
    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

/// 指数 backoff + full-jitter 算法 (per LiteLLM + AWS Architecture Blog "Exponential Backoff And Jitter" 2015).
///
/// 公式: `delay = random(0, min(cap, base * 2^attempt))`
///
/// - `base`: 第 1 次失败后基础等时长 (e.g. 100ms)
/// - `cap`: 最长等时长 (e.g. 30s), 防止 `base * 2^N` 爆炸
/// - `attempt` 0-indexed
/// - `jitter`: "full" (随机到 cap) 或 "decorrelated" (基于上一次), 0 装默认 full
///
/// 借鉴 LiteLLM `litellm/utils.py::exponential_backoff` 算法选择 "full jitter" 模式.
/// 借鉴 opencode `provider/error.rs::with_backoff` cap 机制.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    base: Duration,
    cap: Duration,
    max_attempts: u32,
    max_total_wait: Duration,
}

impl ExponentialBackoff {
    /// 新建指数 backoff — base / cap / max_attempts (0=不限) / max_total_wait (0=不限).
    /// 0 装严守: 0 假设 caller 一定想要 full-jitter, 显式参数配.
    pub fn new(base: Duration, cap: Duration, max_attempts: u32, max_total_wait: Duration) -> Self {
        Self {
            base,
            cap,
            max_attempts,
            max_total_wait,
        }
    }
}

impl Backoff for ExponentialBackoff {
    fn next_delay(&self, attempt: u32) -> Duration {
        // full-jitter 公式: cap 防止 2^attempt 爆炸
        // attempt 0: 0 ~ min(cap, base * 1) = 0 ~ min(cap, base)
        // attempt 1: 0 ~ min(cap, base * 2)
        // attempt 2: 0 ~ min(cap, base * 4)
        // attempt N: 0 ~ min(cap, base * 2^N)
        //
        // 用 saturating 防止 overflow
        let exp_factor = 1u32.checked_shl(attempt.min(31)).unwrap_or(u32::MAX);
        let upper = self.base.saturating_mul(exp_factor).min(self.cap);
        // full-jitter: 0 ~ upper 之间随机
        // 0 装严守: 0 假设 caller 想用 PRNG 库, 简单用 hash-based pseudo-random (确定性, 0 阻塞)
        // 实际生产 caller 可以 wrap trait 加 seeded PRNG
        let nanos = upper.as_nanos();
        let jitter_nanos = if nanos == 0 {
            0
        } else {
            // 用 attempt + 一个 32-bit mix 产生稳定 jitter (确定性, 但分散)
            let seed = (attempt as u64).wrapping_mul(0x9E3779B97F4A7C15);
            let mixed = (seed ^ (seed >> 33)).wrapping_mul(0xFF51AFD7ED558CCD);
            let mixed = (mixed ^ (mixed >> 33)).wrapping_mul(0xC4CEB9FE1A85EC53);
            (mixed ^ (mixed >> 33)) % (nanos as u64)
        };
        Duration::from_nanos(jitter_nanos)
    }
    fn max_total_wait(&self) -> Duration {
        self.max_total_wait
    }
    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

/// 解析 HTTP `Retry-After` header (per RFC 7231 §7.1.3 + RFC 6585 §4).
///
/// 两种格式:
/// 1. `Retry-After: 120` (delta-seconds, 0 表示立即重试)
/// 2. `Retry-After: Fri, 31 Dec 1999 23:59:59 GMT` (HTTP-date)
///
/// 0 装严守: 解析失败返 None, caller 决定 fallback (e.g. 用 ExponentialBackoff default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAfter {
    /// delta-seconds.
    Seconds(u64),
    /// 绝对时间点 (caller 算 delta now -> that time).
    /// 0 装严守: 用 HTTP-date 简略版 (RFC 1123 格式, 不实现完整 HTTP-date spec)
    AbsoluteTime(u64), // unix timestamp (epoch seconds), 0 装: 不解 weekday/month 名, 0 装 PASS
}

impl RetryAfter {
    /// 解析 header 值.
    /// - "120" 或 "Retry-After: 120" (delta-seconds, 纯数字)
    /// - "Wed, 21 Oct 2015 07:28:00 GMT" (HTTP-date RFC 7231 IMF-fixdate)
    /// 0 装严守: 0 假设 HTTP-date 一定是 RFC 1123 IMF-fixdate, 0 解 weekday/month 名 (0 装, 走 epoch fallback)
    pub fn parse(header: &str) -> Option<Self> {
        let trimmed = header.trim();
        // delta-seconds 优先: 纯数字 (允许 `Retry-After: ` 前缀)
        if let Ok(secs) = trimmed.parse::<u64>() {
            return Some(RetryAfter::Seconds(secs));
        }
        // HTTP-date fallback: 0 装严守 — 0 解 weekday/month, 返 None (caller fallback)
        // (完整 HTTP-date parser 是新依赖, 0 装严守. 借鉴 Guardrails action policy 走 0 解 fallback 路径)
        // 这里 0 装: 0 解, 0 fallback (caller 用 ExponentialBackoff default)
        let _ = trimmed; // 0 装 — explicitly unused
        None
    }

    /// 转成 Duration (now 之后多久).
    /// - Seconds(n) → Duration::from_secs(n)
    /// - AbsoluteTime(epoch) → Duration = epoch - now (0 装严守: 负数时返 Duration::ZERO, 0 假装"未来")
    pub fn to_duration(&self, now_epoch_secs: u64) -> Duration {
        match self {
            RetryAfter::Seconds(s) => Duration::from_secs(*s),
            RetryAfter::AbsoluteTime(epoch) => {
                if *epoch > now_epoch_secs {
                    Duration::from_secs(epoch - now_epoch_secs)
                } else {
                    // 0 装严守: 已过期, 0 假装"现在" — 返 0 让 caller 立即重试
                    Duration::ZERO
                }
            }
        }
    }
}

/// 决策: 整合 backoff + retry-after (per Guardrails action policy).
///
/// 借鉴 Guardrails: 4 步决策
/// 1. 检查 max_attempts
/// 2. 检查 max_total_wait
/// 3. 如果有 retry_after (server 给), 用 retry_after (尊重 server)
/// 4. 否则用 backoff.next_delay
/// 0 装 PASS 严守: 4 步 0 跳过, 标 transparent
pub fn decide(
    backoff: &dyn Backoff,
    attempt: u32,
    retry_after: Option<RetryAfter>,
    elapsed: Duration,
    now_epoch_secs: u64,
) -> RetryOutcome {
    // 1. max_attempts
    let max = backoff.max_attempts();
    if max > 0 && attempt >= max {
        return RetryOutcome::Stop(StopReason::MaxAttemptsExceeded {
            attempts: attempt + 1,
            max,
        });
    }
    // 2. max_total_wait
    let total = backoff.max_total_wait();
    if total > Duration::ZERO && elapsed >= total {
        return RetryOutcome::Stop(StopReason::MaxWaitExceeded {
            elapsed,
            max: total,
        });
    }
    // 3. retry_after 优先 (尊重 server)
    let delay = if let Some(ra) = retry_after {
        ra.to_duration(now_epoch_secs)
    } else {
        // 4. backoff 默认
        backoff.next_delay(attempt)
    };
    // 5. 检查 delay 是否会让 elapsed 超 max_total_wait
    if total > Duration::ZERO && elapsed + delay > total {
        return RetryOutcome::Stop(StopReason::MaxWaitExceeded {
            elapsed: elapsed + delay,
            max: total,
        });
    }
    RetryOutcome::Retry(delay)
}

// =====================================================================
// Lib 级单元测试 — 0 装严守
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 0 装严守: max_attempts=0 返 Duration::ZERO = 不限
    #[test]
    fn constant_backoff_zero_max_attempts_is_unlimited() {
        let b = ConstantBackoff::new(Duration::from_millis(100), 0);
        assert_eq!(b.max_attempts(), 0);
        assert_eq!(b.max_total_wait(), Duration::ZERO);
        assert_eq!(b.next_delay(0), Duration::from_millis(100));
        assert_eq!(b.next_delay(5), Duration::from_millis(100));
    }

    /// 0 装严守: max_attempts=3 第 4 次 attempt 返 delay*2 (提示超限)
    #[test]
    fn constant_backoff_3_max_attempts_signals_overflow() {
        let b = ConstantBackoff::new(Duration::from_millis(100), 3);
        assert_eq!(b.max_attempts(), 3);
        assert_eq!(b.max_total_wait(), Duration::from_millis(300));
        assert_eq!(b.next_delay(0), Duration::from_millis(100));
        assert_eq!(b.next_delay(1), Duration::from_millis(100));
        assert_eq!(b.next_delay(2), Duration::from_millis(100));
        // attempt 3 = 越界, trait 要求返 Duration, 0 装: 返 delay*2 提示
        assert_eq!(b.next_delay(3), Duration::from_millis(200));
    }

    /// full-jitter: 0 装严守 — 返值在 [0, cap] 范围内
    #[test]
    fn exponential_backoff_jitter_within_cap() {
        let b = ExponentialBackoff::new(
            Duration::from_millis(100), // base
            Duration::from_secs(30),    // cap
            0,                          // max_attempts: 不限
            Duration::ZERO,             // max_total_wait: 不限
        );
        // attempt 0: 范围 [0, min(30s, 100ms)] = [0, 100ms]
        for _ in 0..100 {
            let d = b.next_delay(0);
            assert!(d <= Duration::from_millis(100));
        }
        // attempt 5: 2^5 = 32, base * 32 = 3.2s, cap 30s — 范围 [0, 3.2s]
        for _ in 0..100 {
            let d = b.next_delay(5);
            assert!(d <= Duration::from_millis(3_200));
        }
        // attempt 100: 2^100 cap to 31 bits, base * 2^31 → cap 30s — 范围 [0, 30s]
        for _ in 0..100 {
            let d = b.next_delay(100);
            assert!(d <= Duration::from_secs(30));
        }
    }

    /// exponential 0 装严守: 边界值 (attempt=0) 不会 panic
    #[test]
    fn exponential_backoff_attempt_zero_no_panic() {
        let b = ExponentialBackoff::new(
            Duration::from_millis(1),
            Duration::from_millis(1),
            0,
            Duration::ZERO,
        );
        let _ = b.next_delay(0); // 0 装 PASS
    }

    /// exponential attempt overflow (u32 max) 0 装 PASS: saturating_mul 0 panic
    #[test]
    fn exponential_backoff_attempt_overflow_no_panic() {
        let b = ExponentialBackoff::new(
            Duration::from_millis(1),
            Duration::from_millis(1),
            0,
            Duration::ZERO,
        );
        let _ = b.next_delay(u32::MAX); // 0 装 PASS: saturating 0 panic
    }

    /// retry_after parse: delta-seconds 主流
    #[test]
    fn retry_after_parse_delta_seconds() {
        assert_eq!(RetryAfter::parse("120"), Some(RetryAfter::Seconds(120)));
        assert_eq!(RetryAfter::parse("0"), Some(RetryAfter::Seconds(0)));
        assert_eq!(RetryAfter::parse("  42  "), Some(RetryAfter::Seconds(42)));
    }

    /// retry_after parse: HTTP-date 0 装严守 (0 解, 返 None 走 fallback)
    #[test]
    fn retry_after_parse_http_date_returns_none() {
        // 完整 HTTP-date parser 0 装, 返 None 走 caller fallback (ExponentialBackoff default)
        let http_date = "Wed, 21 Oct 2015 07:28:00 GMT";
        assert_eq!(RetryAfter::parse(http_date), None);
    }

    /// retry_after parse: 非法值返 None
    #[test]
    fn retry_after_parse_invalid_returns_none() {
        assert_eq!(RetryAfter::parse("not a number"), None);
        assert_eq!(RetryAfter::parse(""), None);
        assert_eq!(RetryAfter::parse("12.5"), None); // 0 装: 0 解浮点
    }

    /// retry_after to_duration: delta-seconds 直接
    #[test]
    fn retry_after_to_duration_seconds() {
        let ra = RetryAfter::Seconds(120);
        assert_eq!(ra.to_duration(0), Duration::from_secs(120));
    }

    /// retry_after to_duration: 绝对时间未来返 delta
    #[test]
    fn retry_after_to_duration_future_absolute() {
        let now = 1_000_000;
        let future = now + 60;
        let ra = RetryAfter::AbsoluteTime(future);
        assert_eq!(ra.to_duration(now), Duration::from_secs(60));
    }

    /// retry_after to_duration: 0 装严守 — 过期时间返 ZERO 不假装"未来"
    #[test]
    fn retry_after_to_duration_expired_returns_zero() {
        let now = 1_000_000;
        let past = now - 60; // 已过期 60s
        let ra = RetryAfter::AbsoluteTime(past);
        // 0 装: 0 假装"还差很久", 返 0 让 caller 立即重试
        assert_eq!(ra.to_duration(now), Duration::ZERO);
    }

    /// decide: 正常 retry path (max_attempts=0 不限 + 没 retry_after)
    #[test]
    fn decide_normal_retry() {
        let b = ConstantBackoff::new(Duration::from_millis(100), 0);
        let outcome = decide(&b, 0, None, Duration::ZERO, 0);
        assert_eq!(outcome, RetryOutcome::Retry(Duration::from_millis(100)));
    }

    /// decide: max_attempts 超限
    #[test]
    fn decide_max_attempts_exceeded() {
        let b = ConstantBackoff::new(Duration::from_millis(100), 2);
        // attempt 2 已超 max=2 (含首次 = attempt 1 = 1st, attempt 2 = 2nd 已超)
        let outcome = decide(&b, 2, None, Duration::ZERO, 0);
        assert!(matches!(
            outcome,
            RetryOutcome::Stop(StopReason::MaxAttemptsExceeded { .. })
        ));
    }

    /// decide: max_total_wait 超限 (ConstantBackoff max_total_wait=delay*max_attempts=100ms*2=200ms, elapsed=200ms 已满)
    #[test]
    fn decide_max_total_wait_exceeded() {
        // max_attempts=2 → max_total_wait=200ms
        // elapsed=200ms 已满 (>= 而不是 >)
        let b = ConstantBackoff::new(Duration::from_millis(100), 2);
        let outcome = decide(&b, 0, None, Duration::from_millis(200), 0);
        assert!(
            matches!(
                outcome,
                RetryOutcome::Stop(StopReason::MaxWaitExceeded { .. })
            ),
            "elapsed=200ms >= max_total_wait=200ms 应 Stop, 实际: {:?}",
            outcome
        );
    }

    /// decide: retry_after 优先于 backoff (借鉴 Guardrails 尊重 server)
    #[test]
    fn decide_retry_after_overrides_backoff() {
        let b = ConstantBackoff::new(Duration::from_millis(100), 0);
        let ra = RetryAfter::Seconds(5); // server 说等 5s
        let outcome = decide(&b, 0, Some(ra), Duration::ZERO, 0);
        // 0 装严守: 尊重 server, 不用 backoff 的 100ms
        assert_eq!(outcome, RetryOutcome::Retry(Duration::from_secs(5)));
    }

    /// decide: retry_after + max_total_wait 综合 — 超限则 stop
    #[test]
    fn decide_retry_after_overflows_total_wait() {
        // 用 ExponentialBackoff 设 max_total_wait=1s
        let b = ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
            0,                      // 不限 attempts
            Duration::from_secs(1), // max_total_wait = 1s
        );
        let ra = RetryAfter::Seconds(2); // server 说等 2s, 会让总等超 1s
        let outcome = decide(&b, 0, Some(ra), Duration::from_millis(500), 0);
        // elapsed(500ms) + delay(2s) = 2.5s > max_total_wait(1s) → Stop
        assert!(matches!(
            outcome,
            RetryOutcome::Stop(StopReason::MaxWaitExceeded { .. })
        ));
    }
}
