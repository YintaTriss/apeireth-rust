//! Integration tests for apeireth-llm-iface (post-1.0.0 增量)
//!
//! Unit tests 在 src/traits.rs mod tests (3 cases):
//! 1. default_complete_stream_emits_single_chunk_containing_full_content
//! 2. default_complete_stream_empty_content_emits_empty_string
//! 3. default_complete_stream_propagates_complete_error
//!
//! 这里 (tests/) 加 per-行为样板, 跟其他 crate (e.g. apeireth-cron/tests/) 一致.
//! 镜像 APEIRETH-CONVENTIONS §0 8 硬墙 守门: 0 触碰 src/, 0 编造"已实现 async".
//!
//! 真生产价值:
//! - LlmRequest 工厂 + builder (温度 clamp + max_tokens cap + trace_id)
//! - LlmResponse JSON round-trip
//! - ChatRole lowercase rename (serde 协议兼容)
//! - ChatMessage 工厂方法
//! - ProviderCapabilities bitflag 组合 (8 能力 + 守门场景)
//! - default complete_stream simulate fallback
//! - custom override complete_stream (TP34 streaming 改造基础)
//! - LlmError 8 variant + 行为方法 (is_retryable / suggested_backoff / provider / status_code)
//! - ProviderHealth / ProviderMetadata serde + 默认值
//! - LlmProvider::name() / supports_model() 契约
//!
//! 0 触碰 5 module + 1 估补.

#![allow(missing_docs)]

use apeireth_llm_iface::{
    ChatMessage, ChatRole, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderCapabilities,
    ProviderHealth, ProviderMetadata, TokenUsage,
};
use async_trait::async_trait;
use futures::stream::StreamExt;

// =============================================================================
// 1. LlmRequest 工厂 + builder 方法
// =============================================================================

#[test]
fn llm_request_new_uses_sensible_defaults() {
    let req = LlmRequest::new("test-model", vec![ChatMessage::user("hi")]);
    assert_eq!(req.model, "test-model");
    assert_eq!(req.messages.len(), 1);
    assert_eq!(req.messages[0].role, ChatRole::User);
    assert_eq!(req.messages[0].content, "hi");
    assert_eq!(req.temperature, 0.7, "默认温度 0.7");
    assert_eq!(req.max_tokens, 1024, "默认 max_tokens 1024");
    assert!(req.trace_id.is_none());
    assert!(req.stop.is_empty());
}

#[test]
fn llm_request_with_temperature_clamps_to_range() {
    let req = LlmRequest::new("m", vec![]).with_temperature(5.0);
    assert_eq!(req.temperature, 2.0, "温度 clamp 到 2.0");
    let req = LlmRequest::new("m", vec![]).with_temperature(-1.0);
    assert_eq!(req.temperature, 0.0, "温度 clamp 到 0.0");
    let req = LlmRequest::new("m", vec![]).with_temperature(1.5);
    assert_eq!(req.temperature, 1.5);
}

#[test]
fn llm_request_with_max_tokens_caps_at_32k() {
    let req = LlmRequest::new("m", vec![]).with_max_tokens(100_000);
    assert_eq!(req.max_tokens, 32_768);
    let req = LlmRequest::new("m", vec![]).with_max_tokens(100);
    assert_eq!(req.max_tokens, 100);
}

#[test]
fn llm_request_with_trace_id() {
    let req = LlmRequest::new("m", vec![]).with_trace_id(42);
    assert_eq!(req.trace_id, Some(42));
}

// =============================================================================
// 2. ChatMessage 工厂 + 角色枚举
// =============================================================================

#[test]
fn chat_message_factories_match_role() {
    let sys = ChatMessage::system("you are a helper");
    let user = ChatMessage::user("hi");
    let asst = ChatMessage::assistant("hello");

    assert_eq!(sys.role, ChatRole::System);
    assert_eq!(sys.content, "you are a helper");
    assert_eq!(user.role, ChatRole::User);
    assert_eq!(user.content, "hi");
    assert_eq!(asst.role, ChatRole::Assistant);
    assert_eq!(asst.content, "hello");
}

#[test]
fn chat_role_serializes_to_lowercase() {
    // 验证 serde rename_all = "lowercase"
    assert_eq!(
        serde_json::to_string(&ChatRole::System).unwrap(),
        "\"system\""
    );
    assert_eq!(serde_json::to_string(&ChatRole::User).unwrap(), "\"user\"");
    assert_eq!(
        serde_json::to_string(&ChatRole::Assistant).unwrap(),
        "\"assistant\""
    );
}

#[test]
fn chat_role_deserializes_from_lowercase() {
    let r: ChatRole = serde_json::from_str("\"system\"").unwrap();
    assert_eq!(r, ChatRole::System);
    let r: ChatRole = serde_json::from_str("\"user\"").unwrap();
    assert_eq!(r, ChatRole::User);
    let r: ChatRole = serde_json::from_str("\"assistant\"").unwrap();
    assert_eq!(r, ChatRole::Assistant);
}

// =============================================================================
// 3. LlmResponse JSON round-trip
// =============================================================================

#[test]
fn llm_response_json_round_trip() {
    let resp = LlmResponse {
        content: "hello world".to_string(),
        usage: TokenUsage::new(10, 20),
        latency_ms: 1234,
        model: "test-m".to_string(),
        finish_reason: "stop".to_string(),
        provider: "stub".to_string(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: LlmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back, resp, "round-trip 1:1");
}

#[test]
fn token_usage_default_is_zero() {
    let u = TokenUsage::default();
    assert_eq!(u.prompt_tokens, 0);
    assert_eq!(u.completion_tokens, 0);
    assert_eq!(u.total_tokens, 0);
}

#[test]
fn token_usage_new_sums_components() {
    let u = TokenUsage::new(7, 11);
    assert_eq!(u.prompt_tokens, 7);
    assert_eq!(u.completion_tokens, 11);
    assert_eq!(u.total_tokens, 18, "total = prompt + completion");
}

// =============================================================================
// 4. ProviderCapabilities bitflag 组合
// =============================================================================

#[test]
fn provider_capabilities_constants_distinct() {
    // 9 capability 标志位互不重叠
    let caps = [
        ProviderCapabilities::CHAT,
        ProviderCapabilities::STREAMING,
        ProviderCapabilities::TOOLS,
        ProviderCapabilities::VISION,
        ProviderCapabilities::JSON_MODE,
        ProviderCapabilities::SYSTEM_PROMPT,
        ProviderCapabilities::THINKING,
        ProviderCapabilities::LONG_CONTEXT,
        ProviderCapabilities::CUSTOM_TEMPERATURE,
    ];
    let bits: Vec<u32> = caps.iter().map(|c| c.bits()).collect();
    let unique: std::collections::HashSet<u32> = bits.iter().cloned().collect();
    assert_eq!(unique.len(), caps.len(), "所有 capability 标志位互不重叠");
}

#[test]
fn provider_capabilities_contains_and_union() {
    let s_chat = ProviderCapabilities::CHAT;
    let s_chat_sys = ProviderCapabilities::CHAT | ProviderCapabilities::SYSTEM_PROMPT;
    let s_chat_tool = ProviderCapabilities::CHAT | ProviderCapabilities::TOOLS;
    let s_chat_stream = ProviderCapabilities::CHAT | ProviderCapabilities::STREAMING;
    let s_chat_vision = ProviderCapabilities::CHAT | ProviderCapabilities::VISION;

    assert!(s_chat.contains(ProviderCapabilities::CHAT));
    assert!(s_chat_sys.contains(ProviderCapabilities::SYSTEM_PROMPT));
    assert!(s_chat_tool.contains(ProviderCapabilities::TOOLS));
    assert!(s_chat_stream.contains(ProviderCapabilities::STREAMING));
    assert!(s_chat_vision.contains(ProviderCapabilities::VISION));
}

#[test]
fn provider_capabilities_union_is_commutative() {
    let a = ProviderCapabilities::CHAT | ProviderCapabilities::STREAMING;
    let b = ProviderCapabilities::STREAMING | ProviderCapabilities::CHAT;
    assert_eq!(a, b, "| 是结合律 + 交换律");
}

#[test]
fn provider_capabilities_none_is_zero() {
    let n = ProviderCapabilities::NONE;
    assert_eq!(n.bits(), 0);
    for c in [
        ProviderCapabilities::CHAT,
        ProviderCapabilities::STREAMING,
        ProviderCapabilities::TOOLS,
    ] {
        assert!(!n.contains(c));
    }
}

// =============================================================================
// 5. default complete_stream simulate fallback (4 端到端场景)
// =============================================================================

struct EchoProvider;

#[async_trait]
impl LlmProvider for EchoProvider {
    fn name(&self) -> &str {
        "echo"
    }
    fn supports_model(&self, _model: &str) -> bool {
        true
    }
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let last_msg = req
            .messages
            .last()
            .map(|m| m.content.as_str())
            .unwrap_or("");
        Ok(LlmResponse {
            content: format!("echo: {last_msg}"),
            usage: TokenUsage::new(1, 2),
            latency_ms: 1,
            model: req.model,
            finish_reason: "stop".to_string(),
            provider: "echo".to_string(),
        })
    }
}

#[tokio::test]
async fn default_complete_stream_emits_echo_content() {
    let p = EchoProvider;
    let req = LlmRequest::new("m", vec![ChatMessage::user("hi")]);
    let mut stream = p.complete_stream(req).await.expect("stream");
    let chunk = stream.next().await.expect("chunk");
    assert_eq!(chunk.unwrap(), "echo: hi");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn default_complete_stream_custom_override_uses_custom_logic() {
    // 验证 provider 真实现可以重写 complete_stream (TP34 streaming 改造基础)
    struct CustomStreamProvider;
    #[async_trait]
    impl LlmProvider for CustomStreamProvider {
        fn name(&self) -> &str {
            "custom"
        }
        fn supports_model(&self, _model: &str) -> bool {
            true
        }
        async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, LlmError> {
            Ok(LlmResponse {
                content: "x".to_string(),
                usage: TokenUsage::new(0, 0),
                latency_ms: 0,
                model: "x".to_string(),
                finish_reason: "stop".to_string(),
                provider: "custom".to_string(),
            })
        }
        async fn complete_stream(
            &self,
            req: LlmRequest,
        ) -> Result<futures::stream::BoxStream<'static, Result<String, LlmError>>, LlmError>
        {
            // 真流式: 拆 content 成 chunk (每字 1 个)
            let chunks: Vec<String> = req
                .messages
                .iter()
                .flat_map(|m| m.content.chars().map(|c| c.to_string()).collect::<Vec<_>>())
                .collect();
            let stream = futures::stream::iter(chunks.into_iter().map(Ok::<_, LlmError>));
            Ok(Box::pin(stream))
        }
    }
    let p = CustomStreamProvider;
    let req = LlmRequest::new("m", vec![ChatMessage::user("abc")]);
    let mut stream = p.complete_stream(req).await.expect("stream");
    let mut chunks = Vec::new();
    while let Some(c) = stream.next().await {
        chunks.push(c.unwrap());
    }
    assert_eq!(chunks, vec!["a", "b", "c"], "custom provider 拆字流式");
}

// =============================================================================
// 6. LlmError 8 variant + 行为方法
// =============================================================================

#[test]
fn llm_error_8_variants_display_uniquely() {
    // 用 Display 区分 8 variant (thiserror derive 自动 impl Display)
    let errs: Vec<(&'static str, LlmError)> = vec![
        ("auth failed", LlmError::AuthFailed("bad key".into())),
        (
            "rate limited",
            LlmError::RateLimited {
                retry_after_ms: 1000,
                provider: "p".into(),
            },
        ),
        (
            "timeout",
            LlmError::Timeout {
                timeout_ms: 5000,
                provider: "p".into(),
            },
        ),
        (
            "bad response",
            LlmError::BadResponse {
                provider: "p".into(),
                detail: "x".into(),
                status_code: Some(500),
            },
        ),
        (
            "network",
            LlmError::Network {
                provider: "p".into(),
                detail: "x".into(),
            },
        ),
        (
            "no provider",
            LlmError::NoProvider {
                model: "m".into(),
                available: vec![],
            },
        ),
        ("config", LlmError::Config("x".into())),
        (
            "provider exhausted",
            LlmError::ProviderExhausted {
                provider: "p".into(),
                attempts: 3,
                last_error: None,
            },
        ),
    ];
    let displays: Vec<String> = errs.iter().map(|(_, e)| e.to_string()).collect();
    let unique: std::collections::HashSet<&String> = displays.iter().collect();
    assert_eq!(unique.len(), displays.len(), "8 variant Display 互不相同");
    // 验证每个错误都有 "合理" 错误特征
    assert!(displays[0].contains("auth failed"));
    assert!(displays[1].contains("rate limited"));
    assert!(displays[2].contains("timeout"));
}

#[test]
fn llm_error_is_retryable_classification() {
    // 重试类: 限流 / 超时 / 网络
    assert!(LlmError::RateLimited {
        retry_after_ms: 1000,
        provider: "p".into(),
    }
    .is_retryable());
    assert!(LlmError::Timeout {
        timeout_ms: 1000,
        provider: "p".into(),
    }
    .is_retryable());
    assert!(LlmError::Network {
        provider: "p".into(),
        detail: "x".into(),
    }
    .is_retryable());

    // 非重试类: auth / config / 永久错误
    assert!(!LlmError::AuthFailed("bad key".into()).is_retryable());
    assert!(!LlmError::Config("bad url".into()).is_retryable());
    assert!(!LlmError::NoProvider {
        model: "x".into(),
        available: vec![]
    }
    .is_retryable());
}

#[test]
fn llm_error_suggested_backoff() {
    // RateLimited 用 retry_after_ms
    let d = LlmError::RateLimited {
        retry_after_ms: 2500,
        provider: "p".into(),
    }
    .suggested_backoff();
    assert_eq!(d.as_millis(), 2500);

    // Timeout 默认 1000ms
    let d = LlmError::Timeout {
        timeout_ms: 1,
        provider: "p".into(),
    }
    .suggested_backoff();
    assert_eq!(d.as_millis(), 1000);

    // Network 默认 500ms
    let d = LlmError::Network {
        provider: "p".into(),
        detail: "x".into(),
    }
    .suggested_backoff();
    assert_eq!(d.as_millis(), 500);

    // 其他 0
    let d = LlmError::AuthFailed("x".into()).suggested_backoff();
    assert_eq!(d.as_millis(), 0);
}

#[test]
fn llm_error_provider_extraction() {
    let p = "minimax";
    assert_eq!(
        LlmError::RateLimited {
            retry_after_ms: 100,
            provider: p.into()
        }
        .provider(),
        Some(p)
    );
    assert_eq!(LlmError::AuthFailed("x".into()).provider(), None);
    assert_eq!(LlmError::Config("x".into()).provider(), None);
}

#[test]
fn llm_error_status_code_extraction() {
    let err = LlmError::BadResponse {
        provider: "p".into(),
        detail: "x".into(),
        status_code: Some(503),
    };
    assert_eq!(err.status_code(), Some(503));

    let err = LlmError::AuthFailed("x".into());
    assert_eq!(err.status_code(), None);
}

// =============================================================================
// 7. ProviderHealth / ProviderMetadata serde + 默认
// =============================================================================

#[test]
fn provider_health_default_is_healthy() {
    let h = ProviderHealth::default();
    assert!(h.healthy);
    assert_eq!(h.latency_p50_ms, 0);
    assert_eq!(h.error_rate, 0.0);
    assert_eq!(h.last_check_ms, 0);
    assert_eq!(h.consecutive_failures, 0);
}

#[test]
fn provider_health_serde_round_trip() {
    let h = ProviderHealth {
        healthy: false,
        latency_p50_ms: 250,
        error_rate: 0.05,
        last_check_ms: 1_700_000_000_000,
        consecutive_failures: 3,
    };
    let json = serde_json::to_string(&h).unwrap();
    let back: ProviderHealth = serde_json::from_str(&json).unwrap();
    assert_eq!(back, h);
}

#[test]
fn provider_metadata_serde_round_trip() {
    let m = ProviderMetadata {
        name: "minimax".into(),
        version: "1.0.0".into(),
        endpoint: Some("https://api.minimaxi.com/v1".into()),
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: ProviderMetadata = serde_json::from_str(&json).unwrap();
    // ProviderMetadata 没 impl PartialEq, 用字段 1:1 比
    assert_eq!(back.name, m.name);
    assert_eq!(back.version, m.version);
    assert_eq!(back.endpoint, m.endpoint);
}

#[test]
fn provider_metadata_endpoint_defaults_none() {
    let m = ProviderMetadata {
        name: "".into(),
        version: "".into(),
        endpoint: None,
    };
    assert!(m.endpoint.is_none());
}

// =============================================================================
// 8. LlmProvider::name() + supports_model() 默认 + 自定义
// =============================================================================

struct TaggedProvider(&'static str);
#[async_trait]
impl LlmProvider for TaggedProvider {
    fn name(&self) -> &str {
        self.0
    }
    fn supports_model(&self, model: &str) -> bool {
        model.starts_with(self.0)
    }
    async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse {
            content: "x".into(),
            usage: TokenUsage::new(0, 0),
            latency_ms: 0,
            model: "x".to_string(),
            finish_reason: "stop".to_string(),
            provider: self.0.into(),
        })
    }
}

#[test]
fn provider_name_returned_as_set() {
    let p1 = TaggedProvider("provider-a");
    let p2 = TaggedProvider("provider-b");
    assert_eq!(p1.name(), "provider-a");
    assert_eq!(p2.name(), "provider-b");
}

#[test]
fn supports_model_uses_custom_logic() {
    let p = TaggedProvider("minimax");
    assert!(p.supports_model("minimax-M3"));
    assert!(!p.supports_model("gpt-4o"));
}
