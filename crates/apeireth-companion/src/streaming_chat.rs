//! TP34 Streaming + Tool Loop 整合 — Phase A 状态机骨架.
//!
//! **任务**: `reports/tp34-streaming-tool-loop-spec-2026-08-20.md` Phase A.
//! **范围**: 基础类型 + 状态机骨架 + 双轨 CoT 跨 chunk 缓冲 + 工具循环回灌.
//! **不触碰**: 现有 `extract_minimax_cot` / `chat_once` / `stream_forward` / `dispatch` 签名.
//! **对齐**: spec §3.1 (5 态) + §3.3 (5 种 SSE 事件) + §4.1 (6+ 单测).
//!
//! # 设计原则
//!
//! 1. **0 假装严守**: 无 `<think>` / `<!-- -->` 标记时, 切不出 CoT (visible 全保留), 不假装 CoT 必有.
//! 2. **跨 chunk 缓冲**: MiniMax M3 流式 chunk 边界可能切 `<think>` / `</think>` / `<!--` / `-->`,
//!    状态机保留残余到下一 chunk (per 验证报告 §5 + §6).
//! 3. **双轨兼容**: `<think>` 优先 (8/20 实测), `<!-- -->` 兜底 (8/19 验证报告 + 兼容代理).
//!    与 `extract_minimax_cot` 语义 1:1 (per spec §3.2).
//! 4. **0 假装 panic**: 畸形 chunk (缺 `choices` / `delta` / `content`) → 空 Vec 返, 不 panic.
//! 5. **0 触碰约束**: 不引入新外部依赖, 不动 `Cargo.toml`, 不动任何 LOCKED 入口.
//!
//! # 状态机 (6 态)
//!
//! ```text
//!   Init → CollectingReasoning → CollectingToolCall → AwaitingToolResult
//!        → ResumedStreaming → CollectingReasoning (新轮) | Done
//! ```
//!
//! # 公开 API
//!
//! - [`StreamingChatState`] — 6 态枚举.
//! - [`StreamingChat`] — 状态机持有者 (`state` 字段 pub, 调用方可查/可驱动).
//! - [`SseEvent`] — 5 种 SSE 内部事件 (ReasoningDelta / ContentDelta / ToolCall / ToolResult / Stop).
//! - [`StreamingChatHandler`] — 3 回调 trait (on_reasoning / on_tool_call / on_content).
//! - [`StreamingChat::new`] — 构造空状态机 (Init).
//! - [`StreamingChat::feed_chunk`] — 主入口: 吃一个 OpenAI SSE chunk, 返 0-N 内部事件.
//! - [`StreamingChat::feed_tool_result`] — 工具循环回灌: 注入工具结果, 转 `ResumedStreaming`.
//!
//! # Phase B (后续, 本文件不实施)
//!
//! - `companion_serve.rs` `req.stream=true` 分支切换: `stream_forward` → `StreamingChat` 驱动.
//! - 真 SSE emit (axum `Sse::new(stream)`).
//! - `execute_if_allowed` 同步执行 + interround sleep 2s.
//! - E2E 3 场景 PowerShell 脚本.
//!
//! # 已知限制
//!
//! - 本模块仅处理 OpenAI 风格 SSE chunk JSON (`choices[0].delta.content` /
//!   `choices[0].delta.tool_calls`). MiniMax M3 实测 0 提供 `delta.reasoning_content` 字段
//!   (per 验证报告 §1-§3), CoT 嵌入 `delta.content` 字符串内, 由状态机切分.
//! - `feed_tool_result` 当前只负责状态机驱动 (`AwaitingToolResult` → `ResumedStreaming`),
//!   工具实际执行 + `execute_if_allowed` 桥接 Phase B 实施.

use serde_json::{json, Value};

// =====================================================================
// 公开类型: 6 态枚举
// =====================================================================

/// TP34 状态机 6 态 (per spec §3.1).
///
/// 状态转移:
/// - `Init` → 首次 `feed_chunk` → `CollectingReasoning`.
/// - `CollectingReasoning` → 遇 `finish_reason` 且 `tool_calls` 非空 → `CollectingToolCall`.
/// - `CollectingReasoning` → 遇 `finish_reason` 且 `tool_calls` 空 → `Done`.
/// - `CollectingToolCall` → 完整收集 → `AwaitingToolResult`.
/// - `AwaitingToolResult` → `feed_tool_result` 注入 → `ResumedStreaming`.
/// - `ResumedStreaming` → 下一 chunk 来 → `CollectingReasoning` (新轮).
/// - `Done` → 终态, 不再接收 chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingChatState {
    /// 初始; 还未收任何 chunk.
    Init,
    /// 收 MiniMax `delta.content` 跨 chunk (CoT 切分状态机).
    CollectingReasoning,
    /// 收完整轮, 拿到 tool_calls (累积多 tool_call delta).
    CollectingToolCall,
    /// 工具结果待注入 (同步执行 + interround sleep 倒计时, Phase B 实施).
    AwaitingToolResult,
    /// 收后续轮 (新一轮 chat_once), 拼到续流.
    ResumedStreaming,
    /// 收尾; 已 emit stop.
    Done,
}

impl StreamingChatState {
    /// 状态名 (供日志 / 调试 / 测试断言).
    pub fn as_str(&self) -> &'static str {
        match self {
            StreamingChatState::Init => "Init",
            StreamingChatState::CollectingReasoning => "CollectingReasoning",
            StreamingChatState::CollectingToolCall => "CollectingToolCall",
            StreamingChatState::AwaitingToolResult => "AwaitingToolResult",
            StreamingChatState::ResumedStreaming => "ResumedStreaming",
            StreamingChatState::Done => "Done",
        }
    }

    /// 是否终态 (`Done`).
    pub fn is_terminal(&self) -> bool {
        matches!(self, StreamingChatState::Done)
    }
}

// =====================================================================
// 公开类型: SseEvent (5 种)
// =====================================================================

/// 内部 SSE 事件 (Phase B 由 axum Sse emit 到客户端).
///
/// 与 spec §3.3 一致:
/// - `ReasoningDelta` → `event: reasoning-delta`
/// - `ContentDelta`   → `event: content-delta`
/// - `ToolCall`       → `event: tool-call`
/// - `ToolResult`     → `event: tool-result`
/// - `Stop`           → `event: stop`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// CoT 段增量 (含 `<think>...</think>` 完整标记或其中字符, 由状态机切分保证语义).
    ReasoningDelta {
        /// 本段增量文本.
        delta: String,
        /// 当前轮数 (1-indexed).
        round: usize,
    },
    /// 正文段增量 (`</think>` 之后或无 CoT 时的全文).
    ContentDelta {
        /// 本段增量文本.
        delta: String,
        /// 当前轮数 (1-indexed).
        round: usize,
    },
    /// 工具调用 (`delta.tool_calls` 累积完整后, 一条事件).
    ToolCall {
        /// OpenAI 风格 tool_call id (`call_xxx`).
        id: String,
        /// 工具名 (`save_memory` / `recall_memory` / ...).
        name: String,
        /// 工具参数 (JSON Value, 已尝试解析 `function.arguments` 字符串).
        args: Value,
        /// 当前轮数 (1-indexed).
        round: usize,
    },
    /// 工具执行结果 (Phase B 由 `execute_if_allowed` 真执行后 emit).
    ToolResult {
        /// 对应 tool_call id.
        tool_call_id: String,
        /// 成功标记.
        success: bool,
        /// 输出 (`serde_json::Value`; 失败时为错误信息).
        output: Value,
        /// 当前轮数 (1-indexed).
        round: usize,
    },
    /// 收尾事件 (一轮或多轮结束后 emit).
    Stop {
        /// OpenAI 风格 finish_reason (`stop` / `tool_calls` / `length` / `null`).
        finish_reason: String,
        /// 累计轮数.
        total_rounds: usize,
    },
}

impl SseEvent {
    /// 事件名 (对齐 spec §3.3 前端契约).
    pub fn event_name(&self) -> &'static str {
        match self {
            SseEvent::ReasoningDelta { .. } => "reasoning-delta",
            SseEvent::ContentDelta { .. } => "content-delta",
            SseEvent::ToolCall { .. } => "tool-call",
            SseEvent::ToolResult { .. } => "tool-result",
            SseEvent::Stop { .. } => "stop",
        }
    }
}

// =====================================================================
// 公开类型: StreamingChatHandler (3 回调 trait)
// =====================================================================

/// 流式处理回调钩子 (Phase B 接入 Sse::new(stream) emit 通道).
///
/// **Send + Sync 强制**: 钩子可能跨 task 持有 (axum SSE body 跨 `tokio::spawn`).
#[allow(dead_code)]
pub trait StreamingChatHandler: Send + Sync {
    /// CoT 段增量回调.
    fn on_reasoning(&self, delta: &str, round: usize);
    /// 工具调用回调 (已累积完整).
    fn on_tool_call(&self, id: &str, name: &str, args: &Value, round: usize);
    /// 正文段增量回调.
    fn on_content(&self, delta: &str, round: usize);
}

// =====================================================================
// 公开类型: StreamingChat (状态机持有者)
// =====================================================================

/// TP34 Streaming + Tool Loop 整合状态机.
///
/// **字段 (per spec §3.2)**:
/// - `state`: 当前态 (pub, 调用方可查).
/// - `round`: 当前轮数 (1-indexed).
/// - `max_rounds`: 工具循环上限 (Phase B 由 `MAX_TOOL_ROUNDS=5` 注入).
/// - `cot_buf`: 跨 chunk 残余 (`<think>` 切在中).
/// - `reasoning_acc` / `content_acc`: 完整轮 CoT / 正文累计 (Phase B 用于 emit 校验).
/// - `tool_calls_acc`: 完整轮 `delta.tool_calls` 累积 (跨 chunk delta 拼接).
/// - `finish_reason`: 当前轮 `finish_reason` (判定去 `CollectingToolCall` 还是 `Done`).
/// - `total_rounds`: 累计轮数 (写 `Stop.total_rounds`).
///
/// **0 触碰**: 不持 pipeline / ToolBridge / LLM impl 引用 (Phase B 才注入).
#[derive(Debug, Clone)]
pub struct StreamingChat {
    /// 当前态 (pub: 调用方 state 转移观察).
    pub state: StreamingChatState,

    /// 当前轮数 (1-indexed; 1 = 首轮 chat_once).
    pub round: usize,

    /// 工具循环上限 (默认 5, per `MAX_TOOL_ROUNDS`; Phase B 由外部注入).
    pub max_rounds: usize,

    /// CoT 跨 chunk 残余 (双轨: `<think>` / `<!--` 都共用此 buf, 由 mode 区分).
    cot_buf: String,

    /// CoT 模式 (Prefix / InCot / InText; 跨 chunk 切 CoT 状态).
    cot_mode: CotMode,

    /// 是否已见 `<think>` open marker (双轨优先级标记).
    seen_think_open: bool,

    /// 完整轮 reasoning 累计 (CoT 拼回, 含原始 marker).
    reasoning_acc: String,

    /// 完整轮正文累计 (`</think>` 之后或无 CoT 时全文).
    content_acc: String,

    /// 完整轮 tool_calls 累积 (跨 chunk delta 拼 id/name/args).
    tool_calls_acc: Vec<Value>,

    /// 当前轮 `finish_reason` (`stop` / `tool_calls` / `length`).
    finish_reason: Option<String>,

    /// 累计轮数 (写 `Stop.total_rounds`).
    pub total_rounds: usize,

    /// 流式模型名 (`MiniMax-M3` 默认; Phase B 由 chat_completions 注入).
    pub model: String,
}

/// CoT 解析内部模式 (跨 chunk 切 CoT 状态机, 双轨).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CotMode {
    /// 文本前缀态 (未遇 `<think>` / `<!--` open).
    Prefix,
    /// 在 CoT 内 (`<think>...</think>` 或 `<!-- ... -->`).
    InCot,
    /// 在正文内 (`</think>` / `-->` 之后).
    InText,
}

impl Default for StreamingChat {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingChat {
    /// 构造空状态机 (Init).
    pub fn new() -> Self {
        Self {
            state: StreamingChatState::Init,
            round: 0,
            max_rounds: 5, // 与 companion_serve.rs:230 MAX_TOOL_ROUNDS 对齐; Phase B 由外部注入.
            cot_buf: String::new(),
            cot_mode: CotMode::Prefix,
            seen_think_open: false,
            reasoning_acc: String::new(),
            content_acc: String::new(),
            tool_calls_acc: Vec::new(),
            finish_reason: None,
            total_rounds: 0,
            model: "MiniMax-M3".to_string(),
        }
    }

    /// 设置模型名 (Phase B 由 chat_completions 注入).
    #[allow(dead_code)]
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// 设置工具循环上限 (Phase B 由外部 `MAX_TOOL_ROUNDS` 注入).
    #[allow(dead_code)]
    pub fn with_max_rounds(mut self, max_rounds: usize) -> Self {
        self.max_rounds = max_rounds;
        self
    }

    /// 主入口: 吃一个 OpenAI SSE chunk, 返 0-N 个内部事件.
    ///
    /// **OpenAI 风格 chunk 格式** (per `/v1/chat/completions` 流式):
    /// ```json
    /// {
    ///   "id": "chatcmpl-xxx",
    ///   "object": "chat.completion.chunk",
    ///   "choices": [{
    ///     "index": 0,
    ///     "delta": {
    ///       "content": "增量文本",
    ///       "tool_calls": [{"index": 0, "id": "call_abc", "function": {"name": "...", "arguments": "..."}}]
    ///     },
    ///     "finish_reason": null | "stop" | "tool_calls" | "length"
    ///   }]
    /// }
    /// ```
    ///
    /// **0 假装**:
    /// - 畸形 chunk (缺 `choices` / `delta` / `content`) → 返空 Vec, 不 panic.
    /// - 状态机 `Done` 后再喂 chunk → 返空 Vec, 0 假装.
    /// - 跨 chunk `<think>` / `<!--` / `-->` / `</think>` 切分 → 残余保留到 `cot_buf`, 不丢不重.
    pub fn feed_chunk(&mut self, chunk: &Value) -> Result<Vec<SseEvent>, String> {
        if self.state.is_terminal() {
            return Ok(Vec::new());
        }

        // Init → CollectingReasoning (首次 chunk 触发).
        // ResumedStreaming → CollectingReasoning (新一轮开; round 自增).
        match self.state {
            StreamingChatState::Init => {
                self.round += 1;
                self.total_rounds = self.total_rounds.max(self.round);
                self.state = StreamingChatState::CollectingReasoning;
            }
            StreamingChatState::ResumedStreaming => {
                self.round += 1;
                self.total_rounds = self.total_rounds.max(self.round);
                // per-round 累积保留到 emit 阶段; 这里仅切态.
                self.state = StreamingChatState::CollectingReasoning;
            }
            _ => {}
        }

        let mut events: Vec<SseEvent> = Vec::new();

        // 提取 choices[0] (容错: 缺 / 多 → 跳过本次).
        let choice = match chunk.get("choices").and_then(|c| c.as_array()) {
            Some(arr) if !arr.is_empty() => &arr[0],
            _ => return Ok(events),
        };

        let Some(delta) = choice.get("delta") else {
            return Ok(events);
        };

        // 处理 `delta.content` (跨 chunk 切 CoT).
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            let buf_drained = self.feed_content(content, &mut events)?;
            // 0 装 PASS: buf_drained 残余保留到下次 chunk (供下次 feed_chunk 切分).
            let _ = buf_drained;
        }

        // 处理 `delta.tool_calls` (累积).
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            self.accumulate_tool_calls(tcs);
        }

        // 处理 `finish_reason` (决策: tool_calls → CollectingToolCall, else → Done).
        if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            self.finish_reason = Some(fr.to_string());

            // 收尾: emit 当前轮累积的 SseEvent.
            // CoT / Content 已经在 feed_content 里按段 emit; 这里只负责 tool_calls / stop.
            if !self.tool_calls_acc.is_empty() {
                // 转入 CollectingToolCall (下一步喂 tool_calls_acc → ToolCall 事件).
                self.state = StreamingChatState::CollectingToolCall;
                events.extend(self.emit_tool_calls());
                // 完整轮结束 → 等下一轮 (AwaitingToolResult 由外部驱动).
                self.state = StreamingChatState::AwaitingToolResult;
            } else {
                // 无工具调用 → emit Stop + 转 Done.
                self.state = StreamingChatState::Done;
                events.push(SseEvent::Stop {
                    finish_reason: fr.to_string(),
                    total_rounds: self.total_rounds,
                });
            }
        }

        Ok(events)
    }

    /// 工具循环回灌: 注入工具结果, 转 `ResumedStreaming` → 下次 chunk 来 → `CollectingReasoning`.
    ///
    /// **参数**:
    /// - `tool_call_id`: 对应 `ToolCall.id` (`call_xxx`).
    /// - `result`: 工具执行结果字符串 (Phase B 由 `execute_if_allowed` 真实产出).
    ///
    /// **状态转移**: `AwaitingToolResult` / `CollectingToolCall` / `CollectingReasoning` (任意非终态)
    /// → `ResumedStreaming` → emit `ToolResult` 事件.
    ///
    /// **0 装**: 若状态机已在 `Done` 或 `Init`, 返空 Vec (不接受结果).
    pub fn feed_tool_result(
        &mut self,
        tool_call_id: &str,
        result: &str,
    ) -> Result<Vec<SseEvent>, String> {
        if self.state.is_terminal() || matches!(self.state, StreamingChatState::Init) {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();
        // 尝试解析为 JSON Value; 失败 → 包装成字符串 Value (0 假装: 不丢工具输出).
        let output: Value =
            serde_json::from_str(result).unwrap_or_else(|_| Value::String(result.to_string()));

        events.push(SseEvent::ToolResult {
            tool_call_id: tool_call_id.to_string(),
            success: true, // Phase B 由 execute_if_allowed.real_result 注入; 当前默认 true (诚实: 调用方传入即视为成功).
            output,
            round: self.round,
        });

        // 状态转移: AwaitingToolResult → ResumedStreaming.
        // 也兼容 CollectingReasoning / CollectingToolCall (测试场景: 手动驱动).
        self.state = StreamingChatState::ResumedStreaming;

        // 重置 per-round 累积 (跨轮复用状态机实例).
        self.tool_calls_acc.clear();
        self.finish_reason = None;
        // cot_buf / reasoning_acc / content_acc / mode 不重置: 续流应拼续, 不丢.

        Ok(events)
    }

    // ============================================================
    // 内部: feed_content — 跨 chunk CoT 切分状态机
    // ============================================================

    /// 喂一段 `delta.content`, 按双轨 (<think> / <!-- -->) 切 CoT / 正文, emit 事件.
    /// 跨 chunk 残余保留在 `cot_buf`, 下次 feed_chunk 来时优先 flush.
    ///
    /// **关键策略**:
    /// - `Prefix` 模式: 字符 `<` 触发"潜在 marker 起始",累积到 pending 直到:
    ///   - 命中 `<think>` / `<!--` → 切 `InCot`;
    ///   - 命中其他闭合标签 (如 `</a>`, `<br>`, 或纯文本 `<` 后跟非 `t`/`!`) → 把 pending 当 visible flush,继续 Prefix;
    ///   - chunk 末尾未闭合 → 把 pending 保留到 `cot_buf` 等下次.
    /// - `InCot` 模式: 累积到 pending 直到:
    ///   - 命中 `</think>` / `-->` → 切 `InText`;
    ///   - chunk 末尾未闭合 → 把 pending 保留到 `cot_buf`.
    /// - `InText` 模式: 字符直接当 ContentDelta emit (单字符 emit, 与 chunk 边界天然兼容).
    ///
    /// 返回: 实际 emit 的事件数 (供测试断言).
    fn feed_content(&mut self, raw: &str, events: &mut Vec<SseEvent>) -> Result<usize, String> {
        if raw.is_empty() {
            return Ok(0);
        }

        // 跨 chunk 处理: 先 flush 上次残余, 再喂新 chunk.
        let input = format!("{}{}", self.cot_buf, raw);
        self.cot_buf.clear();

        // 按字符迭代 + 状态机.
        let mut chars = input.chars().peekable();
        let mut pending: String = String::new();
        let mut emitted = 0usize;

        while let Some(c) = chars.next() {
            match self.cot_mode {
                CotMode::Prefix => {
                    pending.push(c);
                    // 检测 marker: pending 必须以 `<` 开头才会是 marker 候选.
                    if pending.starts_with('<') {
                        if pending.ends_with("<think>") {
                            // 切 InCot.
                            let cot_open = "<think>";
                            let cut = pending.len() - cot_open.len();
                            let visible = &pending[..cut];
                            if !visible.is_empty() {
                                self.content_acc.push_str(visible);
                                events.push(SseEvent::ContentDelta {
                                    delta: visible.to_string(),
                                    round: self.round,
                                });
                                emitted += 1;
                            }
                            self.reasoning_acc.push_str(cot_open);
                            self.cot_mode = CotMode::InCot;
                            self.seen_think_open = true;
                            pending.clear();
                        } else if pending.ends_with("<!--") {
                            let cot_open = "<!--";
                            let cut = pending.len() - cot_open.len();
                            let visible = &pending[..cut];
                            if !visible.is_empty() {
                                self.content_acc.push_str(visible);
                                events.push(SseEvent::ContentDelta {
                                    delta: visible.to_string(),
                                    round: self.round,
                                });
                                emitted += 1;
                            }
                            self.reasoning_acc.push_str(cot_open);
                            self.cot_mode = CotMode::InCot;
                            pending.clear();
                        } else if pending.len() >= 2
                            && !is_marker_prefix(&pending)
                            && (c == '>' || c == ' ')
                        {
                            // 不闭合也不像 marker 前缀 → 把 pending 当 visible flush.
                            // (例如 "<a", "<b", "< " 全作 visible, 不假装 CoT 必有)
                            self.content_acc.push_str(&pending);
                            events.push(SseEvent::ContentDelta {
                                delta: std::mem::take(&mut pending),
                                round: self.round,
                            });
                            emitted += 1;
                        }
                    } else if c == '<' {
                        // pending 不以 '<' 开头, 但新字符是 '<' — 这不可能发生 (前面已 push).
                        // (防御: 保持 pending 累积)
                    }
                }
                CotMode::InCot => {
                    pending.push(c);
                    if pending.ends_with("</think>") {
                        let cot_block = std::mem::take(&mut pending);
                        self.reasoning_acc.push_str(&cot_block);
                        self.reasoning_acc.push('\n');
                        events.push(SseEvent::ReasoningDelta {
                            delta: cot_block,
                            round: self.round,
                        });
                        emitted += 1;
                        self.cot_mode = CotMode::InText;
                    } else if pending.ends_with("-->") {
                        let cot_block = std::mem::take(&mut pending);
                        self.reasoning_acc.push_str(&cot_block);
                        self.reasoning_acc.push('\n');
                        events.push(SseEvent::ReasoningDelta {
                            delta: cot_block,
                            round: self.round,
                        });
                        emitted += 1;
                        self.cot_mode = CotMode::InText;
                    }
                }
                CotMode::InText => {
                    self.content_acc.push(c);
                    events.push(SseEvent::ContentDelta {
                        delta: c.to_string(),
                        round: self.round,
                    });
                    emitted += 1;
                }
            }
        }

        // 收尾: Prefix 模式若 pending 不为空且以 '<' 开头 → 保留到 cot_buf (跨 chunk 用).
        // InCot 模式若 pending 不为空 → 同样保留.
        if !pending.is_empty() {
            let needs_buf = match self.cot_mode {
                CotMode::Prefix => pending.starts_with('<'),
                CotMode::InCot => true,
                CotMode::InText => false,
            };
            if needs_buf {
                self.cot_buf.push_str(&pending);
            } else {
                // Prefix 模式, pending 是纯文本 → 直接 emit ContentDelta.
                self.content_acc.push_str(&pending);
                events.push(SseEvent::ContentDelta {
                    delta: pending,
                    round: self.round,
                });
                emitted += 1;
            }
        }

        Ok(emitted)
    }

    // ============================================================
    // 内部: accumulate_tool_calls — 跨 chunk 累积 delta.tool_calls
    // ============================================================

    /// 累积 `delta.tool_calls` 数组. OpenAI 风格 chunk 可能分散在多 chunk:
    /// - chunk 1: `[{"index": 0, "id": "call_abc", "function": {"name": "save_memory", "arguments": ""}}]`
    /// - chunk 2: `[{"index": 0, "function": {"arguments": "{\"content\":"}}]`
    /// - chunk 3: `[{"index": 0, "function": {"arguments": "\"hello\"}"}}]`
    ///
    /// 合并策略: 按 `index` 字段归并; `arguments` 字符串拼接; `id` / `name` 取首次.
    fn accumulate_tool_calls(&mut self, tcs: &[Value]) {
        for tc in tcs {
            let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            // 扩展 tool_calls_acc 到 idx+1.
            while self.tool_calls_acc.len() <= idx {
                self.tool_calls_acc.push(json!({}));
            }
            let entry = &mut self.tool_calls_acc[idx];

            // 合并 id (仅首次写入, 后序 chunk 可能省略).
            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    entry["id"] = Value::String(id.to_string());
                }
            }

            // 合并 function.name.
            if let Some(name) = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
            {
                if !name.is_empty() {
                    // 确保 entry["function"] 是 Object.
                    if !entry
                        .get("function")
                        .map(|v| v.is_object())
                        .unwrap_or(false)
                    {
                        entry["function"] = json!({});
                    }
                    entry["function"]["name"] = Value::String(name.to_string());
                }
            }

            // 合并 function.arguments (字符串拼接).
            if let Some(args_str) = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
            {
                let current = entry
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("")
                    .to_string();
                let merged = format!("{}{}", current, args_str);
                if !entry
                    .get("function")
                    .map(|v| v.is_object())
                    .unwrap_or(false)
                {
                    entry["function"] = json!({});
                }
                entry["function"]["arguments"] = Value::String(merged);
            }

            // type 字段 (OpenAI 风格: "function").
            if let Some(typ) = tc.get("type").and_then(|t| t.as_str()) {
                if !typ.is_empty() {
                    entry["type"] = Value::String(typ.to_string());
                }
            }
        }
    }

    // ============================================================
    // 内部: emit_tool_calls — 从 tool_calls_acc emit ToolCall 事件
    // ============================================================

    /// emit ToolCall 事件 (每条 tool_call 一条事件), 并返回.
    fn emit_tool_calls(&self) -> Vec<SseEvent> {
        let mut out = Vec::with_capacity(self.tool_calls_acc.len());
        for tc in &self.tool_calls_acc {
            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let args_str = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            out.push(SseEvent::ToolCall {
                id,
                name,
                args,
                round: self.round,
            });
        }
        out
    }
}

// =====================================================================
// 内部 helper: is_marker_prefix — 判断 pending 是否仍可能是 <think> / <!-- 的真前缀
// =====================================================================

/// 判断 `s` 是否仍是 `<think>` 或 `<!--` 的真前缀 (即还可能继续扩展匹配).
///
/// **设计**:
/// - `<think>` 7 字符前缀: `<`, `t`, `h`, `i`, `n`, `k` (再 + `>` 才闭合).
///   `s` 长度 ≤ 6 且是前缀 → true.
/// - `<!--` 4 字符前缀: `<`, `!`, `-` (再 + `-` 才闭合).
///   `s` 长度 ≤ 3 且是前缀 → true.
/// - 已匹配 (长度 ≥) → false (已闭合, 走 emit).
/// - 其他开头字符 (如 `<a`) → false (不是 marker 前缀, 应 flush visible).
fn is_marker_prefix(s: &str) -> bool {
    // 优先 think_open.
    if "<think>".starts_with(s) && s.len() < "<think>".len() {
        return true;
    }
    // 其次 html_comment_open.
    if "<!--".starts_with(s) && s.len() < "<!--".len() {
        return true;
    }
    false
}

// =====================================================================
// 单测 (per spec §4.1, 6+ 测覆盖)
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------
    // 测试 1: 状态转换 Init → CollectingReasoning → Done (无工具调用, 无 CoT)
    // -------------------------------------------------------------
    #[test]
    fn state_init_to_done_no_tool_no_cot() {
        let mut chat = StreamingChat::new();
        assert_eq!(chat.state, StreamingChatState::Init);

        // chunk 1: content delta (纯正文)
        let chunk = json!({
            "id": "chatcmpl-1",
            "choices": [{
                "index": 0,
                "delta": {"content": "你好"},
                "finish_reason": null
            }]
        });
        let events = chat.feed_chunk(&chunk).unwrap();
        assert_eq!(chat.state, StreamingChatState::CollectingReasoning);
        assert_eq!(chat.round, 1);
        // 期望: 1 个 ContentDelta (无 CoT 标记)
        assert_eq!(events.len(), 1);
        match &events[0] {
            SseEvent::ContentDelta { delta, round } => {
                assert_eq!(delta, "你好");
                assert_eq!(*round, 1);
            }
            _ => panic!("期望 ContentDelta"),
        }

        // chunk 2: finish_reason=stop (无工具)
        let chunk = json!({
            "choices": [{
                "delta": {},
                "finish_reason": "stop"
            }]
        });
        let events = chat.feed_chunk(&chunk).unwrap();
        assert_eq!(chat.state, StreamingChatState::Done);
        // 期望: 1 个 Stop 事件
        assert_eq!(events.len(), 1);
        match &events[0] {
            SseEvent::Stop {
                finish_reason,
                total_rounds,
            } => {
                assert_eq!(finish_reason, "stop");
                assert_eq!(*total_rounds, 1);
            }
            _ => panic!("期望 Stop"),
        }
    }

    // -------------------------------------------------------------
    // 测试 2: 跨 chunk CoT 切分 — `<think>` 被 chunk 边界切分, 完整还原
    // -------------------------------------------------------------
    #[test]
    fn cot_split_across_chunk_boundary_think() {
        let mut chat = StreamingChat::new();

        // chunk 1: "<thi" (跨 chunk 切 think_open)
        let chunk1 = json!({
            "choices": [{
                "delta": {"content": "<thi"},
                "finish_reason": null
            }]
        });
        let events1 = chat.feed_chunk(&chunk1).unwrap();
        // 残余保留在 cot_buf, 0 emit (pending 未闭合 think_open)
        assert_eq!(events1.len(), 0);
        assert_eq!(chat.state, StreamingChatState::CollectingReasoning);
        // cot_buf 应保留 "<thi"
        assert!(!chat.cot_buf.is_empty(), "残余保留到 cot_buf");

        // chunk 2: "nk>思考</thi" (续切)
        let chunk2 = json!({
            "choices": [{
                "delta": {"content": "nk>思考</thi"},
                "finish_reason": null
            }]
        });
        let events2 = chat.feed_chunk(&chunk2).unwrap();
        // 期望: 0 ContentDelta (没 visible); 1 ReasoningDelta (含 <think> 闭标记)
        // 实际: 切分状态机按字符流, "<thi"+"nk>思考</thi" 累积到 pending, 末 "nk>思考</thi"
        // 不含闭合 `</think>`, 因为切在 "</thi", 而 InText 起始需 `</think>` (8 字符).
        // 容错: pending "<think>思考</thi" → InCot 模式累积, 不 emit (等下 chunk).
        // 此测断言: 没误把 CoT 当 visible, 没 emit ContentDelta.
        let content_count = events2
            .iter()
            .filter(|e| matches!(e, SseEvent::ContentDelta { .. }))
            .count();
        assert_eq!(content_count, 0, "CoT 不能当 visible emit");
        let reasoning_count = events2
            .iter()
            .filter(|e| matches!(e, SseEvent::ReasoningDelta { .. }))
            .count();
        assert_eq!(reasoning_count, 0, "未闭合 think, 不 emit CoT 段");

        // chunk 3: "nk>已答" (闭合 think_open + 切 InText)
        let chunk3 = json!({
            "choices": [{
                "delta": {"content": "nk>已答"},
                "finish_reason": null
            }]
        });
        let events3 = chat.feed_chunk(&chunk3).unwrap();
        // 期望: 1 ReasoningDelta (含 <think>...</think>), 3 ContentDelta (已答)
        let reasoning = events3
            .iter()
            .find(|e| matches!(e, SseEvent::ReasoningDelta { .. }));
        assert!(reasoning.is_some(), "闭合 think_open 后 emit CoT 段");
        let contents: Vec<_> = events3
            .iter()
            .filter_map(|e| match e {
                SseEvent::ContentDelta { delta, .. } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        let joined: String = contents.concat();
        assert_eq!(joined, "已答", "CoT 后接正文正确切分");
    }

    // -------------------------------------------------------------
    // 测试 3: tool_call 完整收集 — 跨 chunk delta.tool_calls 累积
    // -------------------------------------------------------------
    #[test]
    fn tool_call_accumulate_across_chunks() {
        let mut chat = StreamingChat::new();

        // chunk 1: 首条 tool_call 头 (id + name, arguments 起始)
        let chunk1 = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc",
                        "type": "function",
                        "function": {"name": "save_memory", "arguments": "{\"co"}
                    }]
                },
                "finish_reason": null
            }]
        });
        chat.feed_chunk(&chunk1).unwrap();

        // chunk 2: arguments 续
        let chunk2 = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {"arguments": "ntent\":\"hello\"}"}
                    }]
                },
                "finish_reason": null
            }]
        });
        chat.feed_chunk(&chunk2).unwrap();

        // chunk 3: finish_reason=tool_calls (决策)
        let chunk3 = json!({
            "choices": [{
                "delta": {},
                "finish_reason": "tool_calls"
            }]
        });
        let events = chat.feed_chunk(&chunk3).unwrap();

        // 期望: 1 ToolCall 事件 (累积完整)
        let tool_call = events
            .iter()
            .find(|e| matches!(e, SseEvent::ToolCall { .. }))
            .expect("期望 ToolCall 事件");
        match tool_call {
            SseEvent::ToolCall {
                id,
                name,
                args,
                round,
            } => {
                assert_eq!(id, "call_abc", "id 正确");
                assert_eq!(name, "save_memory", "name 正确");
                assert_eq!(args["content"], "hello", "args 跨 chunk 拼接完整");
                assert_eq!(*round, 1);
            }
            _ => panic!(),
        }
        assert_eq!(chat.state, StreamingChatState::AwaitingToolResult);
    }

    // -------------------------------------------------------------
    // 测试 4: tool_result 注入 — AwaitingToolResult → ResumedStreaming
    // -------------------------------------------------------------
    #[test]
    fn tool_result_injection_transitions_state() {
        let mut chat = StreamingChat::new();

        // 先走到 AwaitingToolResult
        chat.round = 1;
        chat.tool_calls_acc.push(json!({
            "id": "call_xyz",
            "type": "function",
            "function": {"name": "recall_memory", "arguments": "{}"}
        }));
        chat.state = StreamingChatState::AwaitingToolResult;

        // 注入工具结果 (JSON 形式)
        let result_json = r#"{"memory":"猫叫小白"}"#;
        let events = chat.feed_tool_result("call_xyz", result_json).unwrap();

        // 期望: 1 ToolResult 事件 + 状态转移
        assert_eq!(events.len(), 1);
        match &events[0] {
            SseEvent::ToolResult {
                tool_call_id,
                success,
                output,
                round,
            } => {
                assert_eq!(tool_call_id, "call_xyz");
                assert!(*success);
                assert_eq!(output["memory"], "猫叫小白", "JSON 正确解析");
                assert_eq!(*round, 1);
            }
            _ => panic!("期望 ToolResult"),
        }
        assert_eq!(chat.state, StreamingChatState::ResumedStreaming);

        // 验证: tool_result 后, 下一个 chunk 来 → CollectingReasoning (新轮)
        let chunk = json!({
            "choices": [{
                "delta": {"content": "已答"},
                "finish_reason": null
            }]
        });
        chat.feed_chunk(&chunk).unwrap();
        assert_eq!(chat.state, StreamingChatState::CollectingReasoning);
    }

    // -------------------------------------------------------------
    // 测试 5: 错误路径 — 畸形 chunk 不 panic
    // -------------------------------------------------------------
    #[test]
    fn malformed_chunk_does_not_panic() {
        let mut chat = StreamingChat::new();

        // case A: 空对象
        let events = chat.feed_chunk(&json!({})).unwrap();
        assert!(events.is_empty(), "空对象返空事件");

        // case B: choices 空数组
        let events = chat.feed_chunk(&json!({"choices": []})).unwrap();
        assert!(events.is_empty(), "空 choices 数组返空事件");

        // case C: choices 存在但无 delta
        let events = chat
            .feed_chunk(&json!({"choices": [{"index": 0, "finish_reason": null}]}))
            .unwrap();
        assert!(events.is_empty(), "无 delta 字段返空事件");

        // case D: delta 但 content 不是字符串
        let events = chat
            .feed_chunk(&json!({"choices": [{"delta": {"content": 42}}]}))
            .unwrap();
        assert!(events.is_empty(), "content 非字符串返空事件");

        // case E: delta.tool_calls 不是数组
        let events = chat
            .feed_chunk(&json!({"choices": [{"delta": {"tool_calls": "bad"}}]}))
            .unwrap();
        assert!(events.is_empty(), "tool_calls 非数组返空事件");

        // 验证: 状态机仍处于 CollectingReasoning (Init 后被 feed_chunk 推进过)
        assert_eq!(chat.state, StreamingChatState::CollectingReasoning);
    }

    // -------------------------------------------------------------
    // 测试 6: 双轨兼容 — `<!-- -->` 与 `<think>` 互通
    // -------------------------------------------------------------
    #[test]
    fn cot_dual_rail_compatibility_html_comment() {
        let mut chat = StreamingChat::new();

        // 单 chunk 含完整 `<!-- 思考 -->回答`
        let chunk = json!({
            "choices": [{
                "delta": {"content": "<!-- 思考 -->回答"},
                "finish_reason": null
            }]
        });
        let events = chat.feed_chunk(&chunk).unwrap();

        // 期望: 1 ReasoningDelta (含 <!-- 思考 -->), 1 ContentDelta (回答)
        // 实际: 状态机切分 — "<!--" 切 InCot, "思考" + " " 累积, "-->" 切 InText, "回答" 各 1 emit
        let reasoning_count = events
            .iter()
            .filter(|e| matches!(e, SseEvent::ReasoningDelta { .. }))
            .count();
        assert_eq!(reasoning_count, 1, "`<!-- -->` 兜底 CoT 段");
        let content_count = events
            .iter()
            .filter(|e| matches!(e, SseEvent::ContentDelta { .. }))
            .count();
        assert!(content_count >= 1, "CoT 后接正文");
        // 累计 content_acc 应含 "回答"
        assert!(chat.content_acc.contains("回答"));
        // 累计 reasoning_acc 应含 "<!-- 思考 -->"
        assert!(chat.reasoning_acc.contains("<!--"));
        assert!(chat.reasoning_acc.contains("-->"));
    }

    // -------------------------------------------------------------
    // 测试 7 (额外): Done 状态后再喂 chunk 返空 (0 假装)
    // -------------------------------------------------------------
    #[test]
    fn done_state_ignores_subsequent_chunks() {
        let mut chat = StreamingChat::new();
        // 推进到 Done
        chat.feed_chunk(&json!({
            "choices": [{
                "delta": {"content": "hi"},
                "finish_reason": null
            }]
        }))
        .unwrap();
        chat.feed_chunk(&json!({
            "choices": [{
                "delta": {},
                "finish_reason": "stop"
            }]
        }))
        .unwrap();
        assert_eq!(chat.state, StreamingChatState::Done);

        // 再喂 chunk: 应返空
        let events = chat
            .feed_chunk(&json!({
                "choices": [{
                    "delta": {"content": "再问一次"},
                    "finish_reason": null
                }]
            }))
            .unwrap();
        assert!(events.is_empty(), "Done 态不接受 chunk");
    }

    // -------------------------------------------------------------
    // 测试 8 (额外): Init 状态未推进时 feed_tool_result 返空 (0 假装)
    // -------------------------------------------------------------
    #[test]
    fn init_state_ignores_tool_result() {
        let mut chat = StreamingChat::new();
        assert_eq!(chat.state, StreamingChatState::Init);

        let events = chat.feed_tool_result("call_xxx", "result").unwrap();
        assert!(events.is_empty(), "Init 态不接受 tool_result");
    }

    // -------------------------------------------------------------
    // 测试 9 (额外): `with_max_rounds` 配置 + 多轮计数
    // -------------------------------------------------------------
    #[test]
    fn with_max_rounds_and_total_rounds_tracking() {
        let mut chat = StreamingChat::new().with_max_rounds(3);
        assert_eq!(chat.max_rounds, 3);

        // 走 1 轮
        chat.feed_chunk(&json!({
            "choices": [{
                "delta": {"content": "first"},
                "finish_reason": "stop"
            }]
        }))
        .unwrap();
        assert_eq!(chat.total_rounds, 1);

        // 第二轮 (新一轮开 Init 重入)
        chat.state = StreamingChatState::Init; // 模拟新轮
        chat.feed_chunk(&json!({
            "choices": [{
                "delta": {"content": "second"},
                "finish_reason": "stop"
            }]
        }))
        .unwrap();
        assert_eq!(chat.total_rounds, 2);
    }
}
