//! LLM 错误类型 (统一异常分类 + retryable 区分)

#![allow(missing_docs)] // R162 O-5: items here are implementation helpers / private internals; public API is documented in lib.rs
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("auth failed: {0}")]
    AuthFailed(String),

    #[error("rate limited (retry after {retry_after_ms}ms)")]
    RateLimited {
        retry_after_ms: u64,
        provider: String,
    },

    #[error("timeout after {timeout_ms}ms (provider: {provider})")]
    Timeout { timeout_ms: u64, provider: String },

    #[error("bad response from {provider}: {detail}")]
    BadResponse {
        provider: String,
        detail: String,
        status_code: Option<u16>,
    },

    #[error("network error ({provider}): {detail}")]
    Network { provider: String, detail: String },

    #[error("no provider available for model {model}")]
    NoProvider {
        model: String,
        available: Vec<String>,
    },

    #[error("config error: {0}")]
    Config(String),

    #[error("provider {provider} unhealthy after {attempts} attempts")]
    ProviderExhausted {
        provider: String,
        attempts: u32,
        #[source]
        last_error: Option<Box<LlmError>>,
    },
}

impl LlmError {
    /// 该错误是否应该触发 retry 或 fallback
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimited { .. } | LlmError::Timeout { .. } | LlmError::Network { .. }
        )
    }

    /// 推荐 backoff 时长 (毫秒)
    pub fn suggested_backoff(&self) -> Duration {
        match self {
            LlmError::RateLimited { retry_after_ms, .. } => Duration::from_millis(*retry_after_ms),
            LlmError::Timeout { .. } => Duration::from_millis(1000),
            LlmError::Network { .. } => Duration::from_millis(500),
            _ => Duration::from_millis(0),
        }
    }

    /// 该错误来自哪个 provider (用于 Router 路由日志)
    pub fn provider(&self) -> Option<&str> {
        match self {
            LlmError::AuthFailed(_) => None,
            LlmError::RateLimited { provider, .. } => Some(provider.as_str()),
            LlmError::Timeout { provider, .. } => Some(provider.as_str()),
            LlmError::BadResponse { provider, .. } => Some(provider.as_str()),
            LlmError::Network { provider, .. } => Some(provider.as_str()),
            LlmError::NoProvider { .. } => None,
            LlmError::Config(_) => None,
            LlmError::ProviderExhausted { provider, .. } => Some(provider.as_str()),
        }
    }

    /// HTTP status code (如果适用)
    pub fn status_code(&self) -> Option<u16> {
        match self {
            LlmError::BadResponse { status_code, .. } => *status_code,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! 单元测试 — LlmError 的 retryable / backoff / provider 路由语义
    //!
    //! 设计: 接口 crate 0 业务逻辑, 但 8 硬墙里 A3 13 键 verdict cache 跟 LlmError 的 retryable 判定强相关
    //! (retrier 必须严格按 is_retryable() 决定是否重试, 否则会误重试 AuthFailed/Config 这种永久错).
    //! 所以测这 3 个方法 0 假装严守.

    use super::*;

    /// is_retryable happy path: 3 个瞬时错返 true, 5 个永久错返 false.
    #[test]
    fn is_retryable_classifies_8_variants_correctly() {
        // 瞬时 (可重试)
        assert!(LlmError::RateLimited {
            retry_after_ms: 1000,
            provider: "x".into()
        }
        .is_retryable());
        assert!(LlmError::Timeout {
            timeout_ms: 5000,
            provider: "x".into()
        }
        .is_retryable());
        assert!(LlmError::Network {
            provider: "x".into(),
            detail: "connection reset".into()
        }
        .is_retryable());
        // 永久 (不重试)
        assert!(!LlmError::AuthFailed("bad key".into()).is_retryable());
        assert!(!LlmError::BadResponse {
            provider: "x".into(),
            detail: "400 bad request".into(),
            status_code: Some(400)
        }
        .is_retryable());
        assert!(!LlmError::NoProvider {
            model: "m".into(),
            available: vec![]
        }
        .is_retryable());
        assert!(!LlmError::Config("missing env".into()).is_retryable());
        // ProviderExhausted 永久 (已耗尽, 重试无意义) — 0 假装
        assert!(!LlmError::ProviderExhausted {
            provider: "x".into(),
            attempts: 3,
            last_error: None
        }
        .is_retryable());
    }

    /// suggested_backoff happy path: RateLimited 用 server 返的 retry_after_ms, Timeout 1000ms, Network 500ms, 其它 0ms
    #[test]
    fn suggested_backoff_returns_expected_durations() {
        assert_eq!(
            LlmError::RateLimited {
                retry_after_ms: 2500,
                provider: "x".into()
            }
            .suggested_backoff(),
            Duration::from_millis(2500)
        );
        assert_eq!(
            LlmError::Timeout {
                timeout_ms: 30000,
                provider: "x".into()
            }
            .suggested_backoff(),
            Duration::from_millis(1000)
        );
        assert_eq!(
            LlmError::Network {
                provider: "x".into(),
                detail: "x".into()
            }
            .suggested_backoff(),
            Duration::from_millis(500)
        );
        assert_eq!(
            LlmError::AuthFailed("x".into()).suggested_backoff(),
            Duration::from_millis(0)
        );
        assert_eq!(
            LlmError::Config("x".into()).suggested_backoff(),
            Duration::from_millis(0)
        );
    }

    /// suggested_backoff edge case: retry_after_ms=0 合法 (server 允许瞬时重试)
    #[test]
    fn suggested_backoff_zero_retry_after_is_legal() {
        // O-5 不假装: server 返 retry_after=0 意味着 "立即可重试", 0 不是错误值
        let e = LlmError::RateLimited {
            retry_after_ms: 0,
            provider: "x".into(),
        };
        assert_eq!(e.suggested_backoff(), Duration::ZERO);
        assert!(e.is_retryable());
    }

    /// provider happy path: 6 变体带 provider 字段返 Some, 3 变体不返 None
    #[test]
    fn provider_returns_correct_name_for_8_variants() {
        // 6 带 provider
        assert_eq!(
            LlmError::RateLimited {
                retry_after_ms: 100,
                provider: "minimax".into()
            }
            .provider(),
            Some("minimax")
        );
        assert_eq!(
            LlmError::Timeout {
                timeout_ms: 100,
                provider: "openai".into()
            }
            .provider(),
            Some("openai")
        );
        assert_eq!(
            LlmError::BadResponse {
                provider: "anthropic".into(),
                detail: "x".into(),
                status_code: Some(500)
            }
            .provider(),
            Some("anthropic")
        );
        assert_eq!(
            LlmError::Network {
                provider: "x".into(),
                detail: "x".into()
            }
            .provider(),
            Some("x")
        );
        assert_eq!(
            LlmError::ProviderExhausted {
                provider: "minimax".into(),
                attempts: 3,
                last_error: None
            }
            .provider(),
            Some("minimax")
        );
        // 3 不带 provider
        assert_eq!(LlmError::AuthFailed("x".into()).provider(), None);
        assert_eq!(
            LlmError::NoProvider {
                model: "m".into(),
                available: vec![]
            }
            .provider(),
            None
        );
        assert_eq!(LlmError::Config("x".into()).provider(), None);
    }

    /// provider edge case: nested ProviderExhausted 不递归 (last_error 不穿透) — 当前实现 bug 候选, 0 假装严守
    #[test]
    fn provider_does_not_recurse_into_nested_last_error() {
        // 现状: last_error 嵌套的 LlmError::Network 的 "inner" provider 被忽略
        // 0 假装: 标 false (这是潜在 bug, 主人后续拍板)
        let nested = LlmError::ProviderExhausted {
            provider: "outer".into(),
            attempts: 3,
            last_error: Some(Box::new(LlmError::Network {
                provider: "inner".into(),
                detail: "connection reset".into(),
            })),
        };
        assert_eq!(nested.provider(), Some("outer")); // 不递归
    }

    /// status_code: 只 BadResponse 返 Some, 其它全 None
    #[test]
    fn status_code_only_bad_response_returns_some() {
        assert_eq!(
            LlmError::BadResponse {
                provider: "x".into(),
                detail: "x".into(),
                status_code: Some(503)
            }
            .status_code(),
            Some(503)
        );
        assert_eq!(
            LlmError::BadResponse {
                provider: "x".into(),
                detail: "x".into(),
                status_code: None
            }
            .status_code(),
            None
        );
        assert_eq!(LlmError::AuthFailed("x".into()).status_code(), None);
        assert_eq!(
            LlmError::RateLimited {
                retry_after_ms: 100,
                provider: "x".into()
            }
            .status_code(),
            None
        );
    }
}
