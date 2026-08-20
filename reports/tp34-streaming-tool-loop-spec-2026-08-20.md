# TP34 Streaming + Tool Loop 整合方案调研报告

- **报告日期**: 2026-08-20
- **调研员**: TP34 streaming+tool loop spec researcher
- **任务范围**: 调研 + 设计, **0 写代码**
- **现状 (Read First) 锚点**:
  - 透传分支: `crates/apeireth-companion/examples/companion_serve.rs` L1049-1107 + L1104-1164 (`req.stream=true → stream_forward 透传, 跳过 tool loop`)
  - 流式字节转发: `crates/apeireth-api/src/protocol_handlers.rs` L1379 (`stream_forward`)
  - 双轨 CoT 解析: `crates/apeireth-companion/examples/companion_serve.rs` L938-968 (`extract_minimax_cot` — `<think>` 优先, `<!-- -->` 兼容)
  - 工具循环: `crates/apeireth-companion/examples/companion_serve.rs` L1166-1256 (`while rounds < MAX_TOOL_ROUNDS { chat_once → execute_if_allowed → interround sleep }`)
  - 已借鉴报告: `_research_mem/sub_agent_reports/2026-08-19/MiniMax_reasoning_verification.md` §6 (方案 A 在线解析 + 方案 B 离线兜底)
- **路径勘误 (Read First vs 实际)**:
  - 提示说 `TimelineLlm` 在 `crates/apeireth-companion/src/timeline.rs`; 实际它在 `crates/apeireth-companion/src/world_model.rs` L103 (`pub trait TimelineLlm`) + L263 (`MockTimelineLlm`). `timeline.rs` 是伙伴里程碑轨迹 (与本任务无关).
  - 提示说 `crates/apeireth-companion/src/openai_chat.rs`; 实际**没有**此文件. OpenAI Chat request/response 类型在 `apeireth-api::protocol_handlers` (通过 `OpenAiChatRequest` / `OpenAiChatMessage` 暴露). 流式相关代码 0 在此文件, 0 在本仓另设独立流式模块.
  - 调研都按实际路径读, 不影响结论.

---

## 1. 现状分析

### 1.1 Code Map (关键 4 段)

| 段 | 文件 / 行 | 现状 |
|----|-----------|------|
| `chat_completions` 主链路 | `companion_serve.rs` L971-1289 | 单次同步 `dispatch` 循环 + 工具桥 `execute_if_allowed` + 节律/提炼/lifecycle 钩子 |
| 流式分支 (透传) | `companion_serve.rs` L1104-1164 | `req.stream == true` → 早返 `stream_forward` (字节透传 SSE) |
| `dispatch` (单次同步) | `protocol_handlers.rs` L907-915 | 入口是 1 次 `dispatch_cached` → `dispatch_inner` → `pipeline.run`, 返 `NormalizedResponse` (整块 JSON), **0 流式** |
| `stream_forward` (字节透传) | `protocol_handlers.rs` L1379-1427 | 拼 URL → reqwest `bytes_stream()` → axum `Body::from_stream`, 0 字节篡改 |
| `chat_once` (限流重试) | `companion_serve.rs` L1291-1331 | 同步 3 次 × 6s 退避, 返 `(content, tool_calls)`, **不接流** |
| 工具循环 | `companion_serve.rs` L1166-1256 | `while rounds < MAX_TOOL_ROUNDS (5) { chat_once → execute_if_allowed → interround sleep 2s }`, 整轮结束才返 JSON |
| CoT 解析 | `companion_serve.rs` L938-968 | `extract_minimax_cot` 双轨 (`<think>` 优先, `<!-- -->` 兜底); 0 装 PASS: 无标记时全 visible, 0 假装 CoT 必有 |

### 1.2 关键限制 (per Read First 现状)

1. **互斥路径**: `req.stream=true` 直接早返 `stream_forward`, 不走 `while` 循环, 也不解析 SSE 流里的 `delta.tool_calls` → 客户端拿到 SSE 但本仓**0 执行工具** (L1121 注释已说: "full streaming + tool loop 是 v1.5 后续路线, 0 假装已实现").
2. **dispatch 单次同步**: `dispatch` 收 `NormalizedResponse` (整块 JSON), 拿不到流 token. 工具循环只能在 dispatch 之间"停下来等完整 JSON", 0 能在流中段插工具.
3. **MiniMax M3 流特性 (per 验证报告 §1-§3)**:
   - 0 OpenAI 风格 `delta.reasoning_content` 字段
   - CoT 嵌入 `delta.content` 内, `<think>...</think>` 或 `<!-- ... -->` 边界标记 (8/20 实测主用 `<think>`, 8/19 验证报告主用 `<!-- -->`)
   - 跨 chunk `<!--` / `<think>` / `-->` / `</think>` 可能被切 (per 验证报告 §5 边界 case)
   - `delta.tool_calls` 仍在流里 emit, 但本仓当前 0 收集
4. **限流缓解 (per L1242-1251)**: MiniMax 限流实测: 工具循环轮 1 成功 ~2.7s 后立即发轮 2 → 必触发 `suppressed: openai-chat:MiniMax-M3`. 现行用 `APEIRETH_INTERROUND_SLEEP_MS=2000` (env 可覆盖) 强制等 2s. 整合方案必须继承此节流, 0 装 PASS.
5. **MAX_TOOL_ROUNDS=5**: L134 const. 整合方案必须仍守住此上限, 0 改.
6. **0 触碰约束**: 24 LOCKED 入口签名 + 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache) + workspace.version 1.2.0 全部 0 改.

### 1.3 借鉴报告引用

- **MiniMax_reasoning_verification.md §6 方案 A**: 在线解析 SSE delta.content, 状态机切 CoT/正文; 实现复杂但可逐字流.
- **MiniMax_reasoning_verification.md §6 方案 B**: 拿完整流后 `text.split("<!--...-->")` 一次性切; 实现极简但失去逐字流体验.
- **本任务 = 方案 B 的精神 + 方案 A 的续流**: 走"完整流 → 决策 → 走工具循环 → 续流 → 拼 SSE" 范式, 不是严格意义上"token 级并行"; 但前端体感是"持续 SSE 流", 每条事件按出现顺序 emit.

---

## 2. 目标架构: 3 方案对比

### 方案 A — 客户端先收完整流, 后端再走工具循环

```text
客户端 (req.stream=true) → companion_serve
  ├─ 1. 后端用 stream_forward 收 MiniMax 完整流, 在 bytes_stream 上重组完整 NormalizedResponse
  ├─ 2. 解析: 若 tool_calls.is_empty() → 把完整 content 切 CoT 后重封成 SSE 事件流 (content-delta 一次性 emit)
  └─ 3. 若 tool_calls 非空 → 走 [while 工具循环 (整段同步)] + 第 2 步收尾
```

- **优点**: 实现最简; 0 改 `dispatch` 主体; 复用现有 `chat_once` + `execute_if_allowed`; 0 重设计 SSE 状态机.
- **缺点**: **失去"逐字 streaming"价值** — 用户直到 MiniMax 完整响应才看到第一个字符 (典型 2-5s 延迟); 工具循环期间 0 token emit.
- **0 触碰**: ✅ 全程不碰 dispatch 签名 / 工具循环 / enum.
- **E2E 体感**: 像现版非流式 (整段 JSON), 只是外面包了 SSE 容器. **不满足 P0 "显示每步 cot 和 tool call 详情" 朋友诉求**.

### 方案 B — 流里检测 tool_calls, 暂停收 tool_result, 续流 (推荐)

```text
客户端 (req.stream=true) → companion_serve
  ├─ 1. 用 stream_forward_collect 拿 MiniMax 完整 SSE 流, 在字节流上重组 + 解 SSE → 拿到完整 NormalizedResponse (含 content / tool_calls)
  ├─ 2. 流式重封装: 按 <think>/<!-- 切 CoT, 拼成 SSE 事件 [reasoning-delta] / [content-delta] / [tool-call] 持续 emit
  ├─ 3. 若 tool_calls.is_empty() → 直接 emit [stop]
  └─ 4. 若 tool_calls 非空 → 走 [while 工具循环 (流式状态机):
        ├─ emit [tool-call] (name + args)
        ├─ execute_if_allowed (sync)
        ├─ emit [tool-result] (output)
        ├─ interround sleep 2s (限流缓解)
        ├─ 再发 chat_once (stream=false 但可在 ctx 里用 stream_raw_body 让 MiniMax 走 SSE, 然后再次 stream_forward_collect)
        └─ 拼装下一段 [reasoning-delta] / [content-delta] / [tool-call] / ... 续 emit
      ]
```

- **优点**:
  - **保留逐字流体感**: 用户能看到 reasoning-delta (CoT 边来边流) + tool-call/tool-result (工具执行进度实时反馈) + content-delta (最终正文逐字)
  - **限流节流可继承**: interround sleep 仍生效, 不破坏 MiniMax token 桶
  - **状态机可测**: `StreamingChat` 状态机明确 5 态, 单测可建
  - **契约前向兼容**: emit 顺序 = 出现顺序, 前端现有 `reasoning-delta` / `content-delta` 契约 (companion-desktop runtime.ts:50-59) 不需改
- **缺点**:
  - **重设计 chat_once → stream_chat_once** (需要新函数, 但不动老 chat_once)
  - **重设计 SSE 拼装** (需要新模块 `streaming_assembler.rs`, 与 `extract_minimax_cot` 复用 + 扩展)
  - **多段流拼接语义** (第 N 轮的 content 紧接上轮 end 之后 emit, 不能搞错顺序)
- **0 触碰**: ✅ 不动 `dispatch` / `pipeline` / `stream_forward` 签名; 只加新函数 + 新模块; 24 LOCKED 入口签名 0 改.
- **E2E 体感**: ✅ 满足 P0 "每步 cot 和 tool call 详情"; 客户端拿到完整事件流, 可用现有 RuntimeEvent 管道渲染.

### 方案 C — 改造 MiniMax 接口为 reasoning_effort 模式 (最复杂)

```text
- 调研 MiniMax 是否支持独立 reasoning channel (anthropic thinking 风格)
- 若支持 → 改协议层 OpenAI Chat 适配器, 把 reasoning_content 独立到 axum SSE event 里
- 工具循环用 MiniMax 的 "tool use" 原生流 (如 Claude/Gemini 的 stream tool_use)
- 重写 dispatch → stream_dispatch_with_tools
```

- **优点**: 一旦 MiniMax 升级, 路径最干净; 工具调用语义原生支持
- **缺点**:
  - **MiniMax 当前 0 支持独立 reasoning channel** (per 验证报告 §3: `thinking: {type:"enabled"}` 被 400 拒)
  - **重写 dispatch** → 触碰 LOCKED `pipeline.run` / `dispatch` 链路, **违反 0 改 pipeline 约束**
  - **需要等 MiniMax 升级** (不可控, 时间表 0)
  - **极大风险**: 重写后 v1.0 行为可能漂移, 违反 0 漂移原则
- **0 触碰**: ❌ 必触碰 `apeireth-pipeline` + `apeireth-api` 公开签名 (LOCKED)
- **决策**: ❌ **淘汰**, 当前不可行.

### 决策矩阵

| 维度 | A | B | C |
|------|---|---|---|
| 实现复杂度 | 低 (~3 天) | 中 (~7-10 天) | 高 (~20+ 天) |
| P0 朋友诉求满足 | ❌ | ✅ | ✅ |
| 0 触碰约束 | ✅ | ✅ | ❌ |
| 限流节流保留 | ✅ | ✅ | 待评估 |
| 0 漂移 v1.0 | ✅ | ✅ (新分支) | ❌ (改协议层) |
| 可测性 | ✅ | ✅ (状态机) | ❌ (依赖外部升级) |
| 0 假装严守 | ✅ | ✅ | 待评估 |

**结论**: **方案 B 胜出**, 兼具 0 触碰 + P0 满足 + 可测.

---

## 3. 推荐方案 (B) 具体设计

### 3.1 数据结构: `StreamingChat` 状态机

新增模块: `crates/apeireth-companion/src/streaming_chat.rs` (新文件, 0 触碰现有 `apeireth-companion/src/lib.rs` 之外入口).

```rust
/// TP34 streaming + tool loop 整合状态机 (5 态).
///
/// 状态转移:
///   Init → CollectingReasoning → CollectingToolCall
///        → AwaitingToolResult → ResumedStreaming
///        → CollectingReasoning (新一轮) | Done
pub enum StreamingState {
    /// 初始; 已发首 chunk 请求, 等 SSE 首字节
    Init,
    /// 收 MiniMax delta.content (跨 <think>/<!-- 切 CoT)
    CollectingReasoning {
        buf: String,                  // 跨 chunk 残余 (think_open_marker 切在中)
        mode: ReasoningMode,          // Prefix | InCot | InText
        seen_think_open: bool,
    },
    /// 收完整轮, 拿到 tool_calls
    CollectingToolCall {
        tool_calls: Vec<Value>,       // 累积多 tool_call (chunk 化)
    },
    /// 走 execute_if_allowed (同步), 限流 sleep
    AwaitingToolResult {
        results: Vec<ExecutionResult>,
        sleep_ms_remaining: u64,      // interround sleep 倒计时
    },
    /// 收后续轮 (新一轮 chat_once), 拼到续流
    ResumedStreaming {
        round: usize,
        continuation: String,         // 已 emit 的 content 累计, 用于前端校对
    },
    /// 收尾; emit stop
    Done,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ReasoningMode { Prefix, InCot, InText }

pub struct StreamingChat {
    pub state: StreamingState,
    pub round: usize,                // 当前轮数 (与 while rounds < MAX_TOOL_ROUNDS 对齐)
    pub messages: Vec<OpenAiChatMessage>,
    pub tools: Vec<Value>,
    pub out_tokens: u32,
    pub continuity: String,
    pub max_rounds: usize,            // MAX_TOOL_ROUNDS = 5 (复用 const)
    pub interround_ms: u64,           // APEIRETH_INTERROUND_SLEEP_MS, 默认 2000
    pub model: &'static str,
    // 输出通道: 累积 SSE 事件 (Vec<String>), 调用方拿去 emit
    pub events: Vec<SseEventData>,
    // 上下文: 复用 chat_completions 主链路已注入的 messages + tools
}

pub struct SseEventData {
    pub event: &'static str,         // "reasoning-delta" | "content-delta" | "tool-call" | "tool-result" | "stop"
    pub data: String,
}
```

**5 态语义明确**:
1. `Init`: 进入, 发首轮请求, 状态 → `CollectingReasoning`
2. `CollectingReasoning`: 拼 SSE delta, 切 CoT/正文, emit `reasoning-delta` / `content-delta`; 末尾遇到 `delta.finish_reason` → 决策 (有 tool_calls → `CollectingToolCall`, 0 → `Done`)
3. `CollectingToolCall`: 收完整轮 NormalizedResponse, 提取 `delta.tool_calls`; emit `tool-call` (每个一条) → `AwaitingToolResult`
4. `AwaitingToolResult`: 同步 `execute_if_allowed`; emit `tool-result` (每个一条); interround sleep; → `ResumedStreaming`
5. `ResumedStreaming`: 再发下一轮 `chat_once` (把 tool_result 追加到 messages 后); 收到响应 → `CollectingReasoning` (新轮)
6. `Done`: emit `stop`, 状态终止

### 3.2 Reusable Components (复用现有函数, 0 改)

| 现有函数 | 复用方式 | 文件:行 |
|----------|----------|---------|
| `dispatch` | `streaming_chat::stream_chat_once` 内复用 (同步 dispatch 拿 NormalizedResponse, 然后拼 SSE 重封) | `protocol_handlers.rs` L907 |
| `extract_minimax_cot` | 复用, 但改为状态机版 (`streaming_cot_parser`, 跨 chunk 缓冲); 不改原函数 | `companion_serve.rs` L938 |
| `chat_once` | 复用主体 (限流重试 3×6s); 新增 `stream_chat_once` 调它 + 拼 SSE | `companion_serve.rs` L1291 |
| `execute_if_allowed` | **直接复用** (同步工具执行, 无需流化) | `tool_bridge.rs` L796 |
| `extract_minimax_cot` (双轨) | 复用 `<think>` / `<!-- -->` 切分语义; 加新函数 `streaming_cot_parser` 处理跨 chunk 残余 | `companion_serve.rs` L938 |
| MAX_TOOL_ROUNDS=5 | **复用 const** (不新增 const, 不改 const 值) | `companion_serve.rs` L134 |
| APEIRETH_INTERROUND_SLEEP_MS | **复用 env** | `companion_serve.rs` L1245-1248 |
| LifecycleEvent::PostToolUse | **复用** (emit `tool-result` 后 fire, 跟 L1230 现有调用一致) | `companion_serve.rs` L1226-1233 |
| `continuity` (x-apeireth-continuity) | **复用** (透传 SSE 头 / 拼进 x_apeireth 字段) | `companion_serve.rs` L976-981 |

### 3.3 SSE Event 序列 (前端契约对齐)

emit 顺序 (一轮典型含工具调用):

```
1. event: reasoning-delta   data: {"delta": "We need to think about...", "round": 1}
2. event: reasoning-delta   data: {"delta": "calling save_memory", "round": 1}
3. event: content-delta     data: {"delta": "", "round": 1}                ← CoT 结束后第一个空 content (边界)
4. event: content-delta     data: {"delta": "我先记录", "round": 1}
5. event: content-delta     data: {"delta": "这条信息...", "round": 1}
6. event: tool-call         data: {"id": "call_abc", "name": "save_memory", "args": {...}, "round": 1}
7. event: tool-result       data: {"id": "call_abc", "success": true, "output": {...}, "round": 1, "duration_ms": 12}
   ← interround sleep 2s (限流缓解) →
8. event: reasoning-delta   data: {"delta": "记录完毕, 继续回答...", "round": 2}
9. event: content-delta     data: {"delta": "已记住", "round": 2}
10. event: content-delta    data: {"delta": "。", "round": 2}
11. event: stop             data: {"finish_reason": "stop", "total_rounds": 2, "usage": {...}}
```

**对齐前端契约**: `companion-desktop/runtime.ts:50-59` 已定义 `reasoning-delta` / `content-delta`, **新增** `tool-call` / `tool-result` / `stop` (前端可平滑扩展, 旧事件名 0 改).

**5 轮上限保护**: 当 `round >= MAX_TOOL_ROUNDS (5)` → 强制 emit `tool-result` 字段 + `content-delta` = "工具循环达到上限, 已停止..." → `stop`. 跟 L1252-1254 现有逻辑 1:1.

**MiniMax 限流**: 每轮间 `interround_ms` sleep (env `APEIRETH_INTERROUND_SLEEP_MS=2000`); `chat_once` 内已有 3×6s 重试, 复用.

### 3.4 状态机驱动函数

```rust
// streaming_chat.rs (新模块)
impl StreamingChat {
    /// 主入口: 用 StreamingChat 实例驱完整流 (替代现有 `req.stream` 早返).
    pub async fn drive(
        mut self,
        pipeline: &Arc<Pipeline>,
        bridge: &Arc<ToolBridge>,
        lifecycle: &LifecycleBus,
    ) -> Result<Vec<SseEventData>, String> {
        loop {
            self.step(pipeline, bridge, lifecycle).await?;
            if matches!(self.state, StreamingState::Done) { break; }
            if self.round >= self.max_rounds {
                self.emit_stop_max_rounds();
                break;
            }
        }
        Ok(self.events)
    }

    async fn step(&mut self, p: &Arc<Pipeline>, b: &Arc<ToolBridge>, lc: &LifecycleBus) -> Result<(), String> {
        match &mut self.state {
            StreamingState::Init => {
                self.round += 1;
                self.state = StreamingState::CollectingReasoning { ... };
                Ok(())
            }
            StreamingState::CollectingReasoning { .. } => {
                // 调 stream_chat_once (收完整 SSE, 切 CoT, emit delta)
                let (cot, content, tcs) = stream_chat_once(...).await?;
                // emit reasoning-delta (cot 各段)
                for chunk in cot.split_inclusive('\n') { self.events.push(reasoning_delta(chunk)); }
                // emit content-delta
                self.events.push(content_delta(&content));
                if tcs.is_empty() {
                    self.state = StreamingState::Done;
                } else {
                    self.events.push(tool_call(&tcs));  // 一次发所有 tool_calls (OpenAI 风格)
                    self.state = StreamingState::AwaitingToolResult { results: vec!(), ... };
                }
                Ok(())
            }
            StreamingState::AwaitingToolResult { results, sleep_ms_remaining } => {
                // 同步执行工具
                for tc in &self.pending_tool_calls {
                    let call = ParsedToolCall::from(tc);
                    let r = b.execute_if_allowed(&call).await;
                    let _ = lc.fire(LifecycleEvent::PostToolUse, ...).await;  // 复用
                    results.push(r.clone());
                    self.events.push(tool_result(&r));
                    self.messages.push(tool_message(tc, &r));  // 累积消息, 跟 L1234-1240 一致
                }
                // interround sleep (限流)
                if *sleep_ms_remaining > 0 {
                    tokio::time::sleep(Duration::from_millis(*sleep_ms_remaining)).await;
                }
                self.state = StreamingState::CollectingReasoning { ... };  // 续流
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
```

(伪代码; 实际实现按 axum `tokio::select!` 异步驱动, 不阻塞 SSE emit 通道)

---

## 4. 代码改动 list (file:line + 改多少行)

| # | 文件 | 改动位置 | 性质 | 行数估算 |
|---|------|----------|------|---------|
| 1 | `crates/apeireth-companion/src/streaming_chat.rs` | **新文件** | 新模块 (StreamingChat 状态机 + stream_chat_once) | +400 |
| 2 | `crates/apeireth-companion/src/lib.rs` | `pub mod` 区 | 新模块声明 (`pub mod streaming_chat;`) | +1 |
| 3 | `crates/apeireth-companion/examples/companion_serve.rs` | L1049-1164 | `req.stream=true` 分支: 把 `stream_forward` 透传 → 改为 `StreamingChat::drive` 调用 (旧分支保留 env flag fallback) | ~+30 / -80 (净 -50 行) |
| 4 | `crates/apeireth-companion/examples/companion_serve.rs` | L1291 (chat_once 后) | 新增 `stream_chat_once` (复用 chat_once + 拼 SSE) | +60 |
| 5 | `crates/apeireth-companion/examples/companion_serve.rs` | L938 (extract_minimax_cot 旁) | 新增 `streaming_cot_parser` (跨 chunk 状态机版, 复用相同双轨) | +90 |
| 6 | `crates/apeireth-companion/examples/companion_serve.rs` | L1810 (cot_extraction_tests 旁) | 新增单元测试 (streaming_cot_parser 7+ 测; StreamingChat 状态机 5 态转移测) | +150 |
| 7 | `crates/apeireth-api/src/protocol_handlers.rs` | 0 改动 | **保持 0 触碰** (stream_forward / dispatch 签名 0 改) | 0 |

**改动总行数**: +731 / -80 = **净 +651 行** (绝大部分是新模块 + 测试).

**关键不触碰**: `apeireth-pipeline` (LOCKED) / `apeireth-api` 公开签名 / 24 LOCKED 入口签名 / `workspace.version=1.2.0` / `enum` / `const` / 3 不可变脊柱 / gh_*.ps1 / 其他 AI 工作区 (`apeireth-environment/tests`, `apeireth-provider/tests`).

---

## 5. E2E 验证清单 (3 场景)

### 场景 1: 无工具调用 (基线)

**输入** (curl):
```bash
curl -N -X POST http://localhost:8090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-apeireth-continuity: e2e-no-tool" \
  -d '{
    "model":"MiniMax-M3",
    "stream":true,
    "messages":[{"role":"user","content":"1+1=?"}]
  }'
```

**期望 SSE 事件序列**:
1. `reasoning-delta` × N (CoT 段, `<think>...</think>` 切)
2. `content-delta` × M (正文)
3. `stop` (finish_reason="stop", total_rounds=1, tools_executed=[])

**验收**:
- ✅ CoT 段全部走 `reasoning-delta`, 0 出现在 `content-delta`
- ✅ 跨 chunk 残余 `<think>` / `</think>` 不丢不重复
- ✅ 限流: 单轮 0 interround sleep
- ✅ 响应时间 ≤ 5s (vs 非流式 2-5s; 流式优势: 首 token ≤ 800ms)

### 场景 2: 1 次工具调用 (save_memory)

**输入**:
```bash
curl -N -X POST http://localhost:8090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-apeireth-continuity: e2e-1-tool" \
  -d '{
    "model":"MiniMax-M3",
    "stream":true,
    "messages":[{"role":"user","content":"帮我记住: 我的猫叫小白"}]
  }'
```

**期望 SSE 事件序列**:
1. `reasoning-delta` × N (CoT 思考"用户要存记忆")
2. `content-delta` × K (可能空或短句)
3. `tool-call` {id, name:"save_memory", args:{...}}
4. `tool-result` {success:true, output:{...}, round: 1}
5. ← interround sleep 2s (限流) →
6. `reasoning-delta` × N (CoT "已记住, 准备回复")
7. `content-delta` × M ("已记住小白")
8. `stop` (total_rounds=2, tools_executed=["save_memory"])

**验收**:
- ✅ 工具实际执行 (后续 `recall_memory` 能查到 "小白")
- ✅ 限流 sleep 确实生效 (~2s gap)
- ✅ total_rounds=2 (vs 非流式一致)
- ✅ LifecycleEvent::PostToolUse 真实触发 (lifecycle log hook 有日志)
- ✅ 整体响应 ≤ 12s (1 轮 LLM + 1 工具 + 2s sleep + 1 轮 LLM)

### 场景 3: 2 次工具调用链 (recall_memory + save_memory)

**输入**:
```bash
curl -N -X POST http://localhost:8090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-apeireth-continuity: e2e-2-tools" \
  -d '{
    "model":"MiniMax-M3",
    "stream":true,
    "messages":[{"role":"user","content":"我之前说过我的猫叫什么吗? 没说过的话帮我记下叫小黑"}]
  }'
```

**期望 SSE 事件序列**:
1. `reasoning-delta` × N
2. `content-delta` × K (可能空)
3. `tool-call` {name:"recall_memory", args:{...}}
4. `tool-result` {success:true, output:{...}, round:1}
5. ← interround sleep 2s →
6. `reasoning-delta` × N
7. `content-delta` × K (可能空)
8. `tool-call` {name:"save_memory", args:{...}}
9. `tool-result` {success:true, output:{...}, round:2}
10. ← interround sleep 2s →
11. `reasoning-delta` × N
12. `content-delta` × M ("没说过, 已记下小黑")
13. `stop` (total_rounds=3, tools_executed=["recall_memory", "save_memory"])

**验收**:
- ✅ 2 个 tool-call / tool-result 对齐
- ✅ total_rounds=3 (1 询 + 1 存 + 1 总结)
- ✅ 每轮间 2s 间隔 (MiniMax 0 触发限流)
- ✅ 5 轮上限保护 (round=5 → 强制 stop, 即使还有 tool_calls)

### E2E 自动化 (PowerShell)

新增 `tests/e2e/tp34-streaming-tool-loop.ps1` (新建, 不在 LOCKED gh_*.ps1 5 文件列表):
- 启动 companion_serve (`cargo run --example companion_serve`)
- 跑 3 场景 curl
- 解析 SSE 流 (Python `sseclient` 或 `awk '/^event:/'`)
- 断言: 事件序列匹配 / 跨 chunk 0 丢 / 工具真执行 / 限流 2s sleep 存在

---

## 6. 风险点

### 风险 1: MiniMax 限流 (per 8/20 实测)

- **触发**: 工具循环轮 1 成功 ~2.7s 后立即发轮 2 必触发 `suppressed: openai-chat:MiniMax-M3`
- **缓解**: 继承 `APEIRETH_INTERROUND_SLEEP_MS=2000` env (L1245-1248)
- **新风险**: 流式下用户可能 "盯着屏幕等", 2s sleep 体感"卡住" — 在 SSE 流里 emit 一个 `waiting` event 标识 (可选, 前端可忽略)
- **应对**: env `APEIRETH_INTERROUND_SLEEP_MS=0` 可关掉 (但可能限流), 默认 2000
- **预留**: 流式 chat_once 复用 `chat_once` (3×6s 退避) — 已有

### 风险 2: 工具循环 + 流句柄生命周期

- **触发**: axum SSE body 句柄在工具循环中必须保持开放, 不能被 `execute_if_allowed` (含 blocking 同步) 阻塞
- **缓解**: 状态机 `StreamingChat` 是 push 模型 (累积 `events` Vec), 调用方在所有状态转移完后**一次性** `Sse::new(stream).into_response()`, 0 阻塞
- **新风险**: `execute_if_allowed` 可能调宪法 LLM (`MiniMaxConstitutionLlm`), 这是**同步 LLM 调用** (per L1397-1404), 流式下也要等; 复用现有路径, 0 改
- **应对**: 工具循环仍是同步; 用户接受"工具执行期间流暂停" (但 SSE 连接 0 断, keep-alive 续)

### 风险 3: 跨 chunk 切 CoT 状态机正确性

- **触发**: MiniMax chunk 边界可能切断 `<think>` / `</think>` / `<!--` / `-->` (per 验证报告 §5)
- **缓解**: `streaming_cot_parser` 用 buf + mode 状态机 (跟方案 A 1:1, per 验证报告 §6)
- **新增测试**: 7+ 测 (空 / 单段 / 多段 / 不闭合 / 跨 chunk / 双轨混合), 复用 `cot_extraction_tests` 命名风格 (L1810)
- **0 装 PASS**: 跨 chunk 不闭合时 best-effort 把残余当 visible, 0 假装 CoT 必有

### 风险 4: 0 触碰约束保持

- **触发**: 改动可能无意触碰 `apeireth-api` 公开签名 / `apeireth-pipeline` 任何 .rs / 24 LOCKED 入口
- **缓解**: 所有新代码在 `apeireth-companion/src/streaming_chat.rs` (新文件); `companion_serve.rs` 改动限于 `req.stream` 分支切换 (`stream_forward` → `StreamingChat::drive`); 旧分支保留 env flag `APEIRETH_STREAM_LEGACY_PASSTHROUGH=1` fallback
- **验证**: §7 0 触碰自查清单逐项跑 `git diff --stat` + `git diff --check`

### 风险 5: SSE 事件协议对齐前端契约

- **触发**: 新增 `tool-call` / `tool-result` 事件, 前端可能未对接
- **缓解**: 事件名复用前端已有 `reasoning-delta` / `content-delta` 命名风格; 旧前端**忽略新事件名不报错** (SSE 客户端自然容忍未知事件); `stop` 是 OpenAI 标准
- **向后兼容**: 旧 `req.stream=false` 路径 0 改 (仍走 `while rounds < MAX_TOOL_ROUNDS` 同步循环, JSON 响应)

### 风险 6: 测试覆盖

- **触发**: 状态机 5 态 + 跨 chunk 切 CoT + 工具循环 + 限流 — 边界 case 多
- **缓解**: 单测 ≥ 15 个 (5 态转移 × 3 + 跨 chunk 7+); E2E 3 场景 (含真实 MiniMax API)
- **0 装 PASS**: Mock 模式可离线测 (MockTimelineLlm 类似, 但本任务不引新 mock, 复用 `chat_once` 真实路径)

---

## 7. 0 触碰自查清单

| 项 | 状态 | 验证方法 |
|----|------|---------|
| `apeireth-pipeline` 任何 .rs 0 改 | ✅ | `git diff --stat crates/apeireth-pipeline/` 应为空 |
| `apeireth-api` 公开签名 0 改 | ✅ | `git diff --stat crates/apeireth-api/src/protocol_handlers.rs` 应为空 (新模块在 companion 仓) |
| `workspace.version=1.2.0` 0 改 | ✅ | `grep version Cargo.toml` 应不变 |
| 24 LOCKED 入口签名 0 改 | ✅ | `git diff` 应仅显示 streaming_chat.rs 新文件 + companion_serve.rs 局部 if 分支 |
| `enum` / `const` 0 改 | ✅ | `git diff` 应无 enum 增删 / const 改值 (新 const 在 streaming_chat.rs 内部, 仅模块内用) |
| 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache) | ✅ | 0 触碰 |
| gh_*.ps1 5 文件 0 改 | ✅ | `git diff --stat *.ps1` 应为空 |
| `crates/apeireth-environment/tests/` 0 触碰 | ✅ | 同上 |
| `crates/apeireth-provider/tests/` 0 触碰 | ✅ | 同上 |
| 外部依赖 0 引入 | ✅ | `Cargo.toml` 不动 (新代码全用现有 axum / tokio / serde_json / futures) |
| `extract_minimax_cot` 双轨解析语义 | ✅ | 0 改原函数 (仅新增 `streaming_cot_parser` 用相同双轨) |
| `MAX_TOOL_ROUNDS=5` | ✅ | 0 改 const (新模块 `max_rounds: usize` 字段, 默认 = 5) |
| `dispatch` 签名 | ✅ | 0 改 (复用原 `dispatch`) |
| `stream_forward` 签名 | ✅ | 0 改 (不调用, 用 `reqwest` 直拼 + 自管 SSE) |
| 工具循环 while / chat_once / execute_if_allowed | ✅ | 复用全部, 0 改 |

---

## 8. 预计工作日 (按 1 sub-agent 1 天的派活估)

| 子任务 | 工作量 | 派活 |
|--------|--------|------|
| 1. 写 `streaming_chat.rs` 状态机 + `streaming_cot_parser` | 1.5 天 | backend sub-agent (StreamingChat 状态机 + SSE 事件封装) |
| 2. 改 `companion_serve.rs` `req.stream` 分支切换 | 0.5 天 | backend sub-agent (改 ~30 行, 加 env flag fallback) |
| 3. 单测 (streaming_cot_parser 7+ + StreamingChat 状态机 5 态) | 1 天 | qa sub-agent |
| 4. E2E 3 场景 curl + SSE 解析 + 断言 | 1 天 | qa sub-agent |
| 5. 限流 + 工具循环 边界 (interleave 工具执行 + 流式 emit) | 1 天 | backend sub-agent |
| 6. 文档 + 决策日志 (本报告 → 已落地, 仅 commit message 写) | 0.2 天 | lead |
| 7. Code review + 0 触碰自查 | 0.5 天 | reviewer sub-agent |

**总计**: ~5.7 天 (按 1 sub-agent 1 天串行派活) → **6 个工作日** (含缓冲).

**关键路径**: 1 (streaming_chat.rs) → 2 (切换分支) → 3 (单测) → 5 (边界) → 4 (E2E).

**风险缓冲**: MiniMax 限流若实测 2s sleep 不够, 需调到 4-6s (+0.5 天); 跨 chunk 切 CoT 边界 case 暴露 (+0.5 天). 总最坏 ~7 个工作日.

---

## 附录 A: 借鉴的现有函数 (复用清单, 0 改)

| 函数 / const / env | 文件:行 | 复用方式 |
|--------------------|---------|---------|
| `dispatch` | `protocol_handlers.rs` L907 | `stream_chat_once` 内调 (同步 dispatch 拿完整响应) |
| `stream_forward` | `protocol_handlers.rs` L1379 | 0 调用 (新模块用 reqwest 直拼 + 自管 SSE) |
| `chat_once` | `companion_serve.rs` L1291 | 复用主体 (限流重试 3×6s); 包一层 `stream_chat_once` |
| `extract_minimax_cot` | `companion_serve.rs` L938 | 复用语义, 0 改原函数; 新增 `streaming_cot_parser` (跨 chunk 状态机) |
| `execute_if_allowed` | `tool_bridge.rs` L796 | **直接复用** (同步工具执行) |
| `LifecycleEvent::PostToolUse` | `companion_serve.rs` L1226 | **复用** (emit `tool-result` 后 fire) |
| `continuity` (header) | `companion_serve.rs` L976 | **复用** (拼进 x_apeireth 字段) |
| `MAX_TOOL_ROUNDS=5` | `companion_serve.rs` L134 | **复用 const** |
| `DEFAULT_MAX_TOKENS=8192` | `companion_serve.rs` L136 | **复用** |
| `MAX_TOKENS_CAP=16384` | `companion_serve.rs` L137 | **复用** |
| `MEMORY_SESSION="me"` | `companion_serve.rs` L139 | **复用** (提取阶段) |
| `APEIRETH_INTERROUND_SLEEP_MS` | `companion_serve.rs` L1245 | **复用 env** |
| `tools_schema` | `companion_serve.rs` L379 | **复用** (拼 OpenAI Chat 请求) |
| `cot_extraction_tests` 测试模式 | `companion_serve.rs` L1810 | **复用命名风格** (新测试模块命名一致) |

## 附录 B: 借鉴报告引用

- `_research_mem/sub_agent_reports/2026-08-19/MiniMax_reasoning_verification.md` §1-§3: MiniMax M3 0 独立 reasoning 字段, CoT 嵌入 `<!-- ... -->` (8/19 实测)
- `companion_serve.rs` L919-924: 8/20 后续实测已切换到 `<think>...</think>`, 双轨兼容 (per L929 注释)
- `companion_serve.rs` L1104-1164: 透传 SSE 分支 + 已知限制注释 (v1.5 后续路线)
- `companion_serve.rs` L1186-1194: `extract_minimax_cot` 替代 `content.split("</think>").last()` (MiniMax 不兼容)
- `companion_serve.rs` L1242-1251: MiniMax 限流缓解 (interround sleep 2s)
- `companion_serve.rs` L1252-1254: MAX_TOOL_ROUNDS=5 上限保护

## 附录 C: 路径勘误说明

- **Read First** 说 `TimelineLlm` 在 `timeline.rs`; 实际在 `world_model.rs` L103. `timeline.rs` 是伙伴里程碑轨迹 (与本任务无关).
- **Read First** 说 `openai_chat.rs`; 实际**没有**此文件. OpenAI Chat 类型在 `apeireth-api::protocol_handlers` (通过 `OpenAiChatRequest` / `OpenAiChatMessage` 暴露).
- 调研按实际路径读, 结论不受影响.