//! LLM provider trait + Request/Response 类型 + Capability 声明系统

#![allow(missing_docs)] // R162 O-5: items here are implementation helpers / private internals; public API is documented in lib.rs
use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};

use crate::error::LlmError;

// ============================================================
// ChatRole
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// 系统消息 (定义 LLM 行为)
    System,
    /// 用户消息
    User,
    /// 助手消息 (历史回复)
    Assistant,
}

// ============================================================
// ChatMessage
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

// ============================================================
// LlmRequest
// ============================================================

#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// 模型名 (e.g. "MiniMax-M3", "gpt-4o", "claude-sonnet-4")
    pub model: String,
    /// 对话消息历史
    pub messages: Vec<ChatMessage>,
    /// 温度 0.0 - 2.0 (0 = 确定性, 2 = 创造性)
    pub temperature: f32,
    /// 最大输出 token 数
    pub max_tokens: u32,
    /// 可选 trace_id (关联 apeireth-bus trace)
    pub trace_id: Option<u64>,
    /// 可选停止词
    #[allow(dead_code)]
    pub stop: Vec<String>,
}

impl LlmRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: 0.7,
            max_tokens: 1024,
            trace_id: None,
            stop: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t.clamp(0.0, 2.0);
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n.min(32_768);
        self
    }

    pub fn with_trace_id(mut self, id: u64) -> Self {
        self.trace_id = Some(id);
        self
    }
}

// ============================================================
// LlmResponse
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResponse {
    /// LLM 返回的文本内容
    pub content: String,
    /// Token 使用统计
    pub usage: TokenUsage,
    /// 端到端延迟 (毫秒)
    pub latency_ms: u64,
    /// 实际用的模型 (server 可能 normalize)
    pub model: String,
    /// 完成原因 ("stop" / "length" / "content_filter" / "tool_calls")
    pub finish_reason: String,
    /// 实际响应的 provider 名
    pub provider: String,
}

// ============================================================
// TokenUsage
// ============================================================

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn new(prompt: u32, completion: u32) -> Self {
        Self {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    }
}

// ============================================================
// ProviderCapabilities
// ============================================================

/// Provider 能力声明系统 (compile-time bitmap)
///
/// 每个 provider 实现时声明自己支持什么能力, Router / Council 等消费者
/// 可以据此决定是否路由到该 provider。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderCapabilities(u32);

impl ProviderCapabilities {
    pub const NONE: Self = Self(0);
    pub const CHAT: Self = Self(1 << 0); // Chat completion
    pub const STREAMING: Self = Self(1 << 1); // 流式响应 (SSE)
    pub const TOOLS: Self = Self(1 << 2); // Tool calling (function call)
    pub const VISION: Self = Self(1 << 3); // 图像输入
    pub const JSON_MODE: Self = Self(1 << 4); // JSON 强制输出
    pub const SYSTEM_PROMPT: Self = Self(1 << 5); // 系统提示词
    pub const THINKING: Self = Self(1 << 6); // 思考链 (CoT)
    pub const LONG_CONTEXT: Self = Self(1 << 7); // 长上下文 (>= 100k tokens)
    pub const CUSTOM_TEMPERATURE: Self = Self(1 << 8); // 自定义温度

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// 组合多个能力
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for ProviderCapabilities {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

// ============================================================
// ProviderHealth
// ============================================================

/// Provider 健康状态 (用于 Router 自动 fallback)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub healthy: bool,
    pub latency_p50_ms: u64,
    pub error_rate: f64,    // 0.0 - 1.0
    pub last_check_ms: i64, // unix epoch ms
    pub consecutive_failures: u32,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            healthy: true,
            latency_p50_ms: 0,
            error_rate: 0.0,
            last_check_ms: 0,
            consecutive_failures: 0,
        }
    }
}

// ============================================================
// LlmProvider trait
// ============================================================

/// LLM provider 抽象 (Week 1 MVP)
///
/// Week 1 只实装 `complete()` (同步 chat completion)
/// Week 2+ 会加 `stream()` / `embed()` / `health_check()` 等
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider 名 (路由标识, 用于 fallback_order)
    fn name(&self) -> &str;

    /// 该 provider 是否支持指定 model (粗筛, 真正验证在 complete 时)
    fn supports_model(&self, model: &str) -> bool;

    /// 该 provider 的能力声明
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::CHAT
            | ProviderCapabilities::SYSTEM_PROMPT
            | ProviderCapabilities::CUSTOM_TEMPERATURE
    }

    /// Chat completion (Week 1 MVP)
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError>;

    /// **战役 4-1 真流式 (R17-2026-08-04)**:
    /// 返回 SSE 推流的 content delta 序列, 每个 chunk 是 LLM 返回的 content delta 字符串.
    ///
    /// **不假装策略** (主 17:58 O-5):
    /// - 默认实现 = simulate: 调 `complete()` 拿完整 reply, 然后 emit 单 chunk = 完整 content.
    /// - 真流式 (OpenAI / Anthropic 等协议支持) 在具体 provider 覆盖, 真接 SSE
    ///   (reqwest::Response::bytes_stream + 解析 `data: {JSON}\n\n` / `event: ...\ndata: {JSON}\n\n`).
    /// - TUI `chat_streaming` 调这个方法, 推流给 mpsc::Sender, 用户体验 = 真流式 (边生成边渲染).
    ///
    /// **API 形状**:
    /// - 返回 `Result<BoxStream<'static, Result<String, LlmError>>, LlmError>`:
    ///   - 外层 Result: HTTP 错误 / auth 失败 / 协议层错误 (在 stream 创建前)
    ///   - 内层 Result: 流中 chunk 错误 (流已开始但中途失败)
    /// - 终止语义: stream 拉到 `None` = LLM 已 `finish_reason: stop/length` 或 Anthropic `message_stop`
    /// - 用法: `provider.complete_stream(req).await?.next().await` 拉 chunk
    async fn complete_stream(
        &self,
        req: LlmRequest,
    ) -> Result<BoxStream<'static, Result<String, LlmError>>, LlmError> {
        // 默认实现 (simulate fallback): 调 complete() 拿完整 reply, emit 单 chunk.
        // 真流式 provider (OpenAI / Anthropic / 等) override 这个方法.
        let resp = self.complete(req).await?;
        let content = resp.content;
        let stream = stream::once(async move { Ok::<String, LlmError>(content) });
        Ok(Box::pin(stream))
    }

    /// 健康检查 (Week 2+ 实装, 默认直接返回 healthy)
    #[allow(dead_code)]
    async fn health_check(&self) -> Result<ProviderHealth, LlmError> {
        Ok(ProviderHealth {
            healthy: true,
            ..Default::default()
        })
    }

    /// Provider 元信息 (用于 dashboard / debug)
    #[allow(dead_code)]
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: self.name().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            endpoint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub name: String,
    pub version: String,
    pub endpoint: Option<String>,
}

// ============================================================
// 单元测试 — LlmProvider::complete_stream 默认实现 (战役 4-1)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::StreamExt;

    /// 战役 4-1 验证: 默认 `complete_stream` 调用 `complete()` 拿完整 reply, emit 单 chunk.
    /// Provider 不重写时走 simulate fallback.
    struct StubProvider;

    #[async_trait]
    impl LlmProvider for StubProvider {
        fn name(&self) -> &str {
            "stub"
        }
        fn supports_model(&self, _model: &str) -> bool {
            true
        }
        async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, LlmError> {
            Ok(LlmResponse {
                content: "hello stub".to_string(),
                usage: TokenUsage::new(1, 1),
                latency_ms: 1,
                model: "stub-m".to_string(),
                finish_reason: "stop".to_string(),
                provider: "stub".to_string(),
            })
        }
        // 不重写 complete_stream, 用默认 (simulate fallback)
    }

    #[tokio::test]
    async fn default_complete_stream_emits_single_chunk_containing_full_content() {
        // 战役 4-1: 默认 complete_stream 调 complete() 拿完整 reply, emit 单 chunk
        let p = StubProvider;
        let req = LlmRequest::new("m", vec![ChatMessage::user("hi")]);
        let mut stream = p.complete_stream(req).await.expect("stream");
        // 默认实现: 1 个 chunk = 完整 reply, 然后 stream 结束
        let chunk1 = stream.next().await.expect("first chunk");
        assert_eq!(chunk1.expect("Ok"), "hello stub");
        assert!(stream.next().await.is_none(), "default stream 完结 = None");
    }

    #[tokio::test]
    async fn default_complete_stream_empty_content_emits_empty_string() {
        // 边界: complete() 返空 content, stream 仍 emit 1 个 "" chunk 然后 None
        struct EmptyProvider;
        #[async_trait]
        impl LlmProvider for EmptyProvider {
            fn name(&self) -> &str {
                "empty"
            }
            fn supports_model(&self, _model: &str) -> bool {
                true
            }
            async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, LlmError> {
                Ok(LlmResponse {
                    content: String::new(),
                    usage: TokenUsage::default(),
                    latency_ms: 0,
                    model: "m".to_string(),
                    finish_reason: "stop".to_string(),
                    provider: "empty".to_string(),
                })
            }
        }
        let p = EmptyProvider;
        let req = LlmRequest::new("m", vec![ChatMessage::user("x")]);
        let mut stream = p.complete_stream(req).await.expect("stream");
        let chunk = stream.next().await.expect("chunk");
        assert_eq!(chunk.expect("Ok"), "");
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn default_complete_stream_propagates_complete_error() {
        // 边界: complete() 返 Err → complete_stream 也返 Err (不 emit 任何 chunk)
        struct ErrProvider;
        #[async_trait]
        impl LlmProvider for ErrProvider {
            fn name(&self) -> &str {
                "err"
            }
            fn supports_model(&self, _model: &str) -> bool {
                true
            }
            async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, LlmError> {
                Err(LlmError::Config("test config error".into()))
            }
        }
        let p = ErrProvider;
        let req = LlmRequest::new("m", vec![ChatMessage::user("x")]);
        let result = p.complete_stream(req).await;
        assert!(matches!(result, Err(LlmError::Config(_))));
    }

    // ============================================================
    // 单元测试 — ChatMessage / LlmRequest / ProviderCapabilities
    // (per 2026-08-19 zero-test report P0: 接口 crate 真正 0 业务测试)
    // ============================================================

    /// ChatMessage 3 构造器 happy path: 各自 role 正确
    #[test]
    fn chat_message_constructors_set_correct_role() {
        let s = ChatMessage::system("sys prompt");
        let u = ChatMessage::user("hi");
        let a = ChatMessage::assistant("reply");
        assert_eq!(s.role, ChatRole::System);
        assert_eq!(s.content, "sys prompt");
        assert_eq!(u.role, ChatRole::User);
        assert_eq!(u.content, "hi");
        assert_eq!(a.role, ChatRole::Assistant);
        assert_eq!(a.content, "reply");
    }

    /// LlmRequest builder 链 happy path: new + with_temperature + with_max_tokens + with_trace_id 字段正确
    #[test]
    fn llm_request_builder_chain_populates_fields_correctly() {
        let req = LlmRequest::new("claude-sonnet-4", vec![ChatMessage::user("hi")])
            .with_temperature(0.7)
            .with_max_tokens(2048)
            .with_trace_id(42);
        assert_eq!(req.model, "claude-sonnet-4");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, ChatRole::User);
        assert!((req.temperature - 0.7).abs() < 1e-6);
        assert_eq!(req.max_tokens, 2048);
        assert_eq!(req.trace_id, Some(42));
        // stop 字段空 (无 with_stop 构造器)
        assert!(req.stop.is_empty());
    }

    /// LlmRequest edge case: with_temperature clamp 到 [0, 2], with_max_tokens 限 32_768
    #[test]
    fn llm_request_builder_clamps_out_of_range_values() {
        // 边界: temperature 越界 clamp
        let r1 = LlmRequest::new("m", vec![]).with_temperature(5.0);
        assert!(
            (r1.temperature - 2.0).abs() < 1e-6,
            "temperature 上限 clamp 到 2.0"
        );
        let r2 = LlmRequest::new("m", vec![]).with_temperature(-1.0);
        assert!(
            (r2.temperature - 0.0).abs() < 1e-6,
            "temperature 下限 clamp 到 0.0"
        );
        // 边界: max_tokens 越界 clamp
        let r3 = LlmRequest::new("m", vec![]).with_max_tokens(100_000);
        assert_eq!(r3.max_tokens, 32_768, "max_tokens 上限 clamp 到 32_768");
    }

    /// LlmRequest edge case: 空 messages vec 合法 (某些 provider 支持纯 system prompt)
    #[test]
    fn llm_request_empty_messages_is_legal() {
        let req = LlmRequest::new("gpt-4o", vec![]);
        assert_eq!(req.messages.len(), 0);
        assert_eq!(req.model, "gpt-4o");
    }

    /// ProviderCapabilities bitflag: NONE=0, 各常量独立位, union + contains 正确
    #[test]
    fn provider_capabilities_bitflag_works() {
        assert_eq!(ProviderCapabilities::NONE.bits(), 0);
        assert_eq!(ProviderCapabilities::CHAT.bits(), 1 << 0);
        assert_eq!(ProviderCapabilities::STREAMING.bits(), 1 << 1);
        assert_eq!(ProviderCapabilities::TOOLS.bits(), 1 << 2);
        assert_eq!(ProviderCapabilities::VISION.bits(), 1 << 3);
        assert_eq!(ProviderCapabilities::JSON_MODE.bits(), 1 << 4);
        assert_eq!(ProviderCapabilities::SYSTEM_PROMPT.bits(), 1 << 5);
        assert_eq!(ProviderCapabilities::THINKING.bits(), 1 << 6);
        assert_eq!(ProviderCapabilities::LONG_CONTEXT.bits(), 1 << 7);
        assert_eq!(ProviderCapabilities::CUSTOM_TEMPERATURE.bits(), 1 << 8);
        // union + contains
        let caps = ProviderCapabilities::CHAT | ProviderCapabilities::STREAMING;
        assert!(caps.contains(ProviderCapabilities::CHAT));
        assert!(caps.contains(ProviderCapabilities::STREAMING));
        assert!(!caps.contains(ProviderCapabilities::TOOLS));
        // contains 严格 (subset): caps ⊇ target → true; caps ⊃ target (有 target 没的) → true
        let subset = ProviderCapabilities::CHAT;
        assert!(caps.contains(subset));
        // 全部 9 个位开 (0x1FF)
        let all = ProviderCapabilities::CHAT
            | ProviderCapabilities::STREAMING
            | ProviderCapabilities::TOOLS
            | ProviderCapabilities::VISION
            | ProviderCapabilities::JSON_MODE
            | ProviderCapabilities::SYSTEM_PROMPT
            | ProviderCapabilities::THINKING
            | ProviderCapabilities::LONG_CONTEXT
            | ProviderCapabilities::CUSTOM_TEMPERATURE;
        assert_eq!(all.bits(), 0x1FF);
    }

    /// ProviderHealth::default 是 healthy=true
    #[test]
    fn provider_health_default_is_healthy() {
        let h = ProviderHealth::default();
        assert!(h.healthy);
        assert_eq!(h.latency_p50_ms, 0);
        assert_eq!(h.error_rate, 0.0);
        assert_eq!(h.consecutive_failures, 0);
        assert_eq!(h.last_check_ms, 0);
    }

    /// LlmProvider trait contract: mock impl 实现 4 个必需方法 + capabilities 默认值
    #[tokio::test]
    async fn llm_provider_trait_contract_mock_completes() {
        // 验证: 实现 LlmProvider 的 mock provider 在 dyn dispatch 下能正常调 complete()
        struct MockProvider {
            resp_content: String,
        }
        #[async_trait]
        impl LlmProvider for MockProvider {
            fn name(&self) -> &str {
                "mock"
            }
            fn supports_model(&self, model: &str) -> bool {
                model == "mock-v1"
            }
            async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, LlmError> {
                Ok(LlmResponse {
                    content: self.resp_content.clone(),
                    usage: TokenUsage::new(5, 2),
                    latency_ms: 10,
                    model: "mock-v1".to_string(),
                    finish_reason: "stop".to_string(),
                    provider: "mock".to_string(),
                })
            }
        }
        let p = MockProvider {
            resp_content: "hi from mock".to_string(),
        };
        // 1) dyn dispatch
        let dyn_p: &dyn LlmProvider = &p;
        assert_eq!(dyn_p.name(), "mock");
        assert!(dyn_p.supports_model("mock-v1"));
        assert!(!dyn_p.supports_model("gpt-4o"));
        // 2) capabilities 默认值
        let caps = dyn_p.capabilities();
        assert!(caps.contains(ProviderCapabilities::CHAT));
        assert!(caps.contains(ProviderCapabilities::SYSTEM_PROMPT));
        assert!(caps.contains(ProviderCapabilities::CUSTOM_TEMPERATURE));
        // 3) complete 返 Ok
        let req = LlmRequest::new("mock-v1", vec![ChatMessage::user("x")]);
        let resp = dyn_p.complete(req).await.expect("complete ok");
        assert_eq!(resp.content, "hi from mock");
        assert_eq!(resp.usage.prompt_tokens, 5);
        assert_eq!(resp.usage.completion_tokens, 2);
        assert_eq!(resp.usage.total_tokens, 7); // new(prompt, completion) 算的
        assert_eq!(resp.finish_reason, "stop");
        assert_eq!(resp.provider, "mock");
    }

    /// LlmProvider trait contract: metadata() 默认实现用 name() + CARGO_PKG_VERSION
    #[test]
    fn llm_provider_metadata_default_uses_name_and_cargo_version() {
        struct P;
        #[async_trait]
        impl LlmProvider for P {
            fn name(&self) -> &str {
                "p"
            }
            fn supports_model(&self, _: &str) -> bool {
                true
            }
            async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, LlmError> {
                unimplemented!()
            }
        }
        let m = P.metadata();
        assert_eq!(m.name, "p");
        assert_eq!(m.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(m.endpoint, None);
    }
}
