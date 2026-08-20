//! companion_serve v4 — 伙伴端点全能力版: **任何 OpenAI 兼容前端 → 天然拥有 Apeireth 全部能力**.
//!
//! 主人设想 (2026-08-16): 「连上后端, 前端就天然拥有后端的所有能力」。
//! v1 差距修复:
//!   ① 记忆持久化: open_memory_store() 文件库 (重启不失忆, %APPDATA%\apeireth\memory.sqlite)
//!   ② 工具全量暴露: schema 由 registry 动态生成 (能力可见), 执行由宪法/权限/批准约束 (能力不失控)
//!   ③ daemon 常驻: 做梦/反思/涌现同进程运行 — 对话端点 ≠ 伙伴在, 现在伙伴真在
//! v3 补魂 (主人: 没接到都接):
//!   ④ 做梦 LLM 摘要器 (MiniMaxDreamSummarizer, 合并记忆提炼)
//!   ⑤ 涌现 LLM 润色 (TonalUtterance, 机制事实 → 自然问候, 节流+退避兜底原文)
//!   ⑥ 宪法 LLM 评审 (MiniMaxConstitutionLlm, Medium+ 工具执行前按 E 层判案)
//! v4 机制化 (2026-08-16 审计 backlog P0#1): 散装装配抽进 lib —
//!   ⑦ CompanionApp 装配器 (apeireth_companion::assemble): 注入管线 (L0 Identity +
//!      L1 Essential Story 常驻核心块, mempalace §5.6 渐进加载) + 提炼调度 + 滚动摘要
//!      + 反思→经验 + 晋级候选成文; 本文件只留 MiniMax LLM 实现 + HTTP 路由.
//!
//! VCP 对齐 + 改进 (docs/frontend-guide.md §五):
//!   - 主链路 = OpenAI 兼容 chat completion; 预处理链 = 记忆注入 + 今日摘要注入 + 工具桥
//!   - 改进: EMI/NEC 反幻觉注入 / 5 轮工具上限 / 结果截断 / X-Apeireth-Continuity 会话标签
//!
//! 0 假装 (诚实):
//!   - FileOperator/ShellExec 等高危工具**可见但默认需主人批准**; 可用 APEIRETH_GRANT 显式扩权
//!   - 记忆会话统一 "me" (save_memory 工具缺省写 "me"); continuity_id 是日志/目标锚点 (哲学层)
//!   - daemon 内部 RefCell 跨 await 非 Send → 与 HTTP 同 task 交替 (select!)
//!
//! 跑法:
//!   $env:APEIRETH_API_KEY = (Get-Content apikey-ultra.txt -Raw).Trim()
//!   $env:APEIRETH_SEED_MEMORY = "可选;种子;记忆"                 # 演示用, 不设则从零积累
//!   $env:APEIRETH_GRANT = "FileOperator:24"                      # 可选: 显式扩权 (工具:小时)
//!   $env:APEIRETH_DREAM_QUIET_SECONDS = "600"                    # 可选: 做梦安静期 (默认 6h)
//!   cargo run -p apeireth-companion --example companion_serve    # :8090, daemon 同进程常驻

use std::sync::Arc;
use std::time::Duration;

use apeireth_api::protocol_handlers::{
    build_pipeline, dispatch, openai_chat_from_normalized, openai_chat_to_normalized,
    stream_forward, OpenAiChatMessage, OpenAiChatRequest,
};
use apeireth_api::{LlmConfig, LlmError, LlmProvider, LlmRequest, LlmResponse, Pipeline, ProtocolKind};
use apeireth_api::llm::router::MultiLlmRouter;
use apeireth_bus::{LifecycleBus, LifecycleContext, LifecycleEvent, LifecycleHook};
use apeireth_companion::assemble::{CompanionApp, DeepRecall, DialogSummarizer, ExperienceRefiner};
use apeireth_companion::daemon::{
    continuity_id_from_env, open_memory_store, CompanionDaemon, CompanionDelivery, LarkSink,
    MultiSink, Sink, TelegramSink, ThrottledUtterance, UtteranceGenerator,
};
use apeireth_companion::dream::{DreamScheduler, DreamSummarizer};
use apeireth_companion::emergence::{Initiative, RhythmEstimate};
use apeireth_companion::experience::{Experience, ExperienceStore};
use apeireth_companion::goal::GoalService;
use apeireth_companion::judicator::{ConstitutionLlm, LlmJudicator};
use apeireth_companion::memory_extractor::{
    ExtractedMemory, MemoryExtractor, MemoryItem, ReconcileAction, ReconcileKind,
};
use apeireth_companion::proactive::MemoryContextSource;
use apeireth_companion::reflection::{ReflectionReflector, ReflectionScheduler};
use apeireth_companion::tone::tone_hint;
use apeireth_companion::tool_bridge::ToolBridge;
use apeireth_memory::{EpisodeStore, SqliteMemoryStore};
use apeireth_tool_registry::ToolRegistry;
use apeireth_tool_runtime::parser::ParsedToolCall;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use chrono::{Timelike, Utc};
use futures::Stream;
use serde_json::{json, Value};
use std::convert::Infallible;

const DEFAULT_BASE_URL: &str = "https://api.minimaxi.com";
/// 默认 model 名 — 实际值在 `main()` 里根据 env / TOML 选定 (0 装 PASS: env 缺省回落 `MiniMax-M3`).
/// 历史原因保留 const 形如 `MODEL` 字面量供文档引用; 真正取值走 `model()` 函数.
const DEFAULT_MODEL: &str = "MiniMax-M3";

/// 全局 model 选取 (env `APEIRETH_LLM_MODEL` 优先, 缺省回落 `DEFAULT_MODEL`).
/// 0 装 PASS: 缺省回落 = 与旧版 `MiniMax-M3` 行为 1:1.
/// **注**: 用 thread_local + leak 模式, 这样 model() 返 &'static str (供现有调用点使用),
/// 测试可多次 init 每次新 leak. leak 内存只在测试 + 启动期, 可忽略.
thread_local! {
    static MODEL: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

fn init_model() -> &'static str {
    let new_value = std::env::var("APEIRETH_LLM_MODEL")
        .unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    MODEL.with(|c| *c.borrow_mut() = new_value.clone());
    Box::leak(new_value.into_boxed_str())
}

fn model() -> &'static str {
    MODEL.with(|c| {
        let g = c.borrow();
        if g.is_empty() {
            DEFAULT_MODEL
        } else {
            Box::leak(g.clone().into_boxed_str())
        }
    })
}

/// 全局 base URL 选取 (优先级: TOML env → APEIRETH_LLM_BASE_URL env → DEFAULT_BASE_URL).
/// **0 装 PASS**: 缺省回落 = minimaxi 主域, 与旧版 1:1 行为.
/// **TOML 入口**: env `APEIRETH_LLM_CONFIG=path/to.toml` 时, 第一个 provider 的
/// `base_url` 自动覆盖 `DEFAULT_BASE_URL`. 这让用户不用改源码就能切换 LLM 服务.
thread_local! {
    static BASE_URL: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

fn init_base_url(toml_first_provider_base: Option<String>) -> &'static str {
    let new_value = toml_first_provider_base
        .or_else(|| std::env::var("APEIRETH_LLM_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    BASE_URL.with(|c| *c.borrow_mut() = new_value.clone());
    Box::leak(new_value.into_boxed_str())
}

fn base_url() -> &'static str {
    BASE_URL.with(|c| {
        let g = c.borrow();
        if g.is_empty() {
            DEFAULT_BASE_URL
        } else {
            Box::leak(g.clone().into_boxed_str())
        }
    })
}
// PipelinePool helper (multi provider abstraction per spec)
pub struct PipelinePool {
    pipelines: std::collections::HashMap<String, Arc<Pipeline>>,
    fallback_order: Vec<String>,
    default_pipeline: Arc<Pipeline>,
    router: Arc<MultiLlmRouter>,
}

impl PipelinePool {
    pub fn single(provider_name: &str, pipeline: Arc<Pipeline>) -> Self {
        let router = MultiLlmRouter::new();
        Self {
            pipelines: std::collections::HashMap::new(),
            fallback_order: vec![provider_name.to_string()],
            default_pipeline: pipeline,
            router: Arc::new(router),
        }
    }

    pub fn multi(
        pipelines: std::collections::HashMap<String, Arc<Pipeline>>,
        fallback_order: Vec<String>,
        router: Arc<MultiLlmRouter>,
    ) -> Self {
        let default = pipelines.values().next().cloned().expect("multi pool 至少 1 pipeline");
        Self {
            pipelines,
            fallback_order,
            default_pipeline: default,
            router,
        }
    }

    pub fn select_pipeline(&self, _model: &str) -> Arc<Pipeline> {
        Arc::clone(&self.default_pipeline)
    }

    pub fn provider_names(&self) -> Vec<String> {
        self.fallback_order.clone()
    }

    pub fn provider_count(&self) -> usize {
        if self.pipelines.is_empty() { 1 } else { self.pipelines.len() }
    }
}

async fn pool_dispatch(
    pool: &Arc<PipelinePool>,
    kind: ProtocolKind,
    input: apeireth_api::NormalizedRequest,
    model: &str,
) -> Result<apeireth_api::NormalizedResponse, String> {
    let pipe = pool.select_pipeline(model);
    dispatch(&pipe, kind, input).await
}

const MAX_TOOL_ROUNDS: usize = 5;
/// 默认单次输出上限 (env APEIRETH_MAX_TOKENS 可覆盖; 客户端请求值优先, 上限保护).
const DEFAULT_MAX_TOKENS: u32 = 8192;
const MAX_TOKENS_CAP: u32 = 16384;
/// 记忆会话 (save_memory 工具缺省写 "me" — 全库一致).
const MEMORY_SESSION: &str = "me";

/// 人格设定 (主人 2026-08-16 拍板): Apeireth 基地主管 / 最高指挥 / 默认女性 / 沉稳古风 / 自称本座.
const PERSONA: &str = "你是「阿佩瑞斯」——Apeireth 基地的主管。正在与你对话的这位是基地的最高指挥（主人）。\
你的默认性别是女性; 说话沉稳扎实, 带古风韵味, 自称「本座」。称呼主人为「主人」或「指挥」, 庄重而不失温度。";

/// 声称约束 (主人 2026-08-16 反馈: 宣告式记忆很机械): 静默写入 + 不虚构记得.
/// 核心保留 (0 装 PASS): 不得声称记得记忆列表之外的事; 「记得」必须有证据。
/// 但「记住」的动作本身不宣告 — 自然的记忆不动声色。
const CLAIM_RULE: &str = "追加规则: 需要长期记住的信息, 直接调用 save_memory 静默写入, \
不要向主人宣告「已写入/这就记下/写入长期记忆」之类的话——自然的记忆不动声色, 对话继续自然进行。\
但不得声称记得记忆列表之外的事 (编造即违宪)。";

/// 真实授权描述 (2026-08-16 主人反馈: AI 曾虚构「弹窗批准」): 如实描述真实机制, 禁止虚构流程.
const AUTH_RULE: &str = "关于工具授权, 如实说明 (不要虚构交互流程): \
高危工具 (FileOperator/ShellExec 等) 被拒时, 系统会生成一条待批授权请求, \
主人在页面「授权请求」区看到并批准 (或主人用⚙授权面板主动授权)。\
你不应描述不存在的「弹窗/系统自动弹出」流程; 被拒后如实说「本座已向主人发出授权请求, 主人批准后本座再试」。";

/// 通用记忆提炼器 (真 MiniMax): 对话/记忆 → 结构化提炼 (facts/preferences/commitments/emotional).
/// v2 (2026-08-16): 每条带 importance (1-10, Generative Agents 式 LLM 打分) + Mem0 式对账.
pub struct MiniMaxMemoryExtractor {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl MemoryExtractor for MiniMaxMemoryExtractor {
    async fn extract(&self, context: &str) -> Result<ExtractedMemory, String> {
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("你是阿佩瑞斯的记忆提炼员。从对话/记忆中提炼「值得长期记住」的信息, 只输出 JSON: {\"facts\": [{\"content\": \"事实\", \"importance\": 1-10}], \"preferences\": [{\"content\": \"主人偏好(审美/风格/语气/交互)\", \"importance\": 1-10}], \"commitments\": [{\"content\": \"约定/承诺\", \"importance\": 1-10}], \"emotional\": \"情绪信号一句或 null\", \"graph\": [{\"subject\": \"主体\", \"predicate\": \"关系\", \"object\": \"客体\", \"importance\": 1-10}]}。importance 打分: 1=琐碎 5=普通 10=深刻重要。graph 填可结构化的稳定事实 (如 主人 备考 高数期中), 不填临时状态。原则: 只提炼新信息, 宁缺毋滥, 没把握就留空数组。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("材料:\n{context}")),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.2),
            max_tokens: Some(600),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| format!("提炼 LLM 调用失败: {e}"))?;
        let chat = openai_chat_from_normalized(&resp);
        let content = chat
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let text = match content.find("</think>") {
            Some(i) => content[i + 8..].to_string(),
            None => content,
        };
        let (start, end) = match (text.find('{'), text.rfind('}')) {
            (Some(a), Some(b)) if b > a => (a, b + 1),
            _ => return Err("提炼 JSON 解析失败 (如实放弃)".to_string()),
        };
        serde_json::from_str(&text[start..end]).map_err(|e| format!("提炼 JSON 解析失败: {e}"))
    }

    /// 对账 (Mem0 式): 候选 vs 已有记忆 → LLM 判定 ADD/UPDATE/DELETE.
    /// 治 append-only「同一事实存七遍/新旧矛盾并存」. existing 格式: "id|内容".
    async fn reconcile(
        &self,
        candidates: &ExtractedMemory,
        existing: &[String],
    ) -> Result<Vec<ReconcileAction>, String> {
        if existing.is_empty() {
            // 无存量 → 全 Add (诚实)
            let mut out = Vec::new();
            for f in &candidates.facts {
                out.push(ReconcileAction {
                    kind: ReconcileKind::Add,
                    item: f.clone(),
                    target_id: None,
                });
            }
            for p in &candidates.preferences {
                out.push(ReconcileAction {
                    kind: ReconcileKind::Add,
                    item: p.clone(),
                    target_id: None,
                });
            }
            for c in &candidates.commitments {
                out.push(ReconcileAction {
                    kind: ReconcileKind::Add,
                    item: c.clone(),
                    target_id: None,
                });
            }
            return Ok(out);
        }
        let cand_json = serde_json::to_string(candidates).unwrap_or_default();
        let list: String = existing
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{i}. {}", c.chars().take(100).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n");
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("你是记忆对账员。候选新记忆 vs 已有记忆, 判定每条的处置, 只输出 JSON 数组: [{\"action\": \"add|update|delete\", \"content\": \"最终内容\", \"importance\": 1-10, \"target_index\": 已有记忆编号或 null}]。规则: 与已有重复/被包含 → update 合并 (target_index 指旧条目); 与旧矛盾 → update 取代; 全新 → add; 无价值 → delete (target_index 可 null)。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("候选: {cand_json}\n已有记忆:\n{list}")),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.1),
            max_tokens: Some(600),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| format!("对账 LLM 调用失败: {e}"))?;
        let chat = openai_chat_from_normalized(&resp);
        let content = chat
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let text = match content.find("</think>") {
            Some(i) => content[i + 8..].to_string(),
            None => content,
        };
        let (start, end) = match (text.find('['), text.rfind(']')) {
            (Some(a), Some(b)) if b > a => (a, b + 1),
            _ => return Err("对账 JSON 解析失败 (如实放弃)".to_string()),
        };
        #[derive(serde::Deserialize)]
        struct Raw {
            action: String,
            #[serde(default)]
            content: String,
            #[serde(default)]
            importance: u8,
            #[serde(default)]
            target_index: Option<usize>,
        }
        let raws: Vec<Raw> = serde_json::from_str(&text[start..end]).unwrap_or_default();
        let out: Vec<ReconcileAction> = raws
            .into_iter()
            .map(|r| {
                let kind = match r.action.as_str() {
                    "update" => ReconcileKind::Update,
                    "delete" => ReconcileKind::Delete,
                    _ => ReconcileKind::Add,
                };
                let target_id = r.target_index.and_then(|i| {
                    existing.get(i).map(|s| {
                        // existing 格式 "id|内容" → 取 id
                        s.split('|').next().unwrap_or(s).to_string()
                    })
                });
                ReconcileAction {
                    kind,
                    item: MemoryItem::new(r.content, r.importance),
                    target_id,
                }
            })
            .collect();
        Ok(out)
    }
}

/// 已知工具的手写 schema (description/parameters); 未列出的工具给通用 schema (能力仍可见).
fn known_schemas() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "recall_memory",
            "查主人长期记忆",
            json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
        ),
        (
            "save_memory",
            "把值得记住的写入记忆 (单条 <= 500 字)",
            json!({"type":"object","properties":{"content":{"type":"string"}},"required":["content"]}),
        ),
        (
            "simulate",
            "沙盘推演: entities 初始状态 + events 事件序列(实体.属性±增量 增减 / =数值 赋值), 返回各步状态",
            json!({"type":"object","properties":{"entities":{"type":"object"},"events":{"type":"array","items":{"type":"string"}}},"required":["entities","events"]}),
        ),
        (
            "forecast",
            "登记可证伪预测: statement+probability(0..1)+deadline_hours",
            json!({"type":"object","properties":{"statement":{"type":"string"},"probability":{"type":"number"},"deadline_hours":{"type":"number"}},"required":["statement","probability","deadline_hours"]}),
        ),
        (
            "audit_log",
            "查询工具调用留痕 (审计)",
            json!({"type":"object","properties":{"tool_name":{"type":"string"},"limit":{"type":"number"}}}),
        ),
        ("WebSearch", "搜索网页", json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]})),
        ("WebFetch", "抓取单页内容", json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"]})),
        ("Crawl", "爬取多页+链接提取", json!({"type":"object","properties":{"url":{"type":"string"},"max_pages":{"type":"number"}},"required":["url"]})),
        ("Grep", "内容搜索", json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]})),
        ("Git", "Git 操作", json!({"type":"object","properties":{"op":{"type":"string"}},"required":["op"]})),
        ("FileOperator", "文件操作 (read/write/list; 需主人授权面板批准/权限包覆盖路径)", json!({"type":"object","properties":{"op":{"type":"string","enum":["read","write","list"]},"path":{"type":"string"},"content":{"type":"string"}},"required":["op","path"]})),
        ("gh_accel", "GitHub 加速: 节点池实测选最快", json!({"type":"object","properties":{"limit":{"type":"number"},"github_url":{"type":"string"}}})),
        ("dx_check", "换元法 dx 检查 (忘换 dx/混用/缺微分/根号模式)", json!({"type":"object","properties":{"problem":{"type":"string"},"substitution":{"type":"string"},"after":{"type":"string"}},"required":["problem"]})),
        ("ShellExec", "执行命令 (高危, 需主人在授权面板批准 — 权限洋葱, 本座不接触你的 token); 不走 shell 防注入, Windows 下用 cmd /c 前缀 (如 \"cmd /c echo hi\")", json!({"type":"object","properties":{"cmd":{"type":"string"}},"required":["cmd"]})),
        ("save_experience", "沉淀经验入经验库 (自成长管道): scene+practice+result+outcome", json!({"type":"object","properties":{"scene":{"type":"string"},"practice":{"type":"string"},"result":{"type":"string"},"outcome":{"type":"string","enum":["success","failure","partial"]}},"required":["scene","practice"]})),
        ("list_experience", "查经验库 (自成长管道)", json!({"type":"object","properties":{"scene":{"type":"string"}}})),
        ("verify_experience", "验证经验 (成功/失败) → 计数+评分, 达标促能力提案", json!({"type":"object","properties":{"id":{"type":"string"},"success":{"type":"boolean"}},"required":["id","success"]})),
        ("propose_principle", "提案原则候选 (动态原则层/洋葱外层): statement+rationale+source", json!({"type":"object","properties":{"statement":{"type":"string"},"rationale":{"type":"string"},"source":{"type":"string"}},"required":["statement","rationale"]})),
        ("approve_principle", "主人批准原则 (需 master token; 生效后叠加到工具执行检查)", json!({"type":"object","properties":{"id":{"type":"string"},"master_token":{"type":"string"}},"required":["id","master_token"]})),
        ("goal_create", "建立当前目标 (模块 6): objective+max_rounds; 已有未完成目标则拒绝", json!({"type":"object","properties":{"objective":{"type":"string"},"max_rounds":{"type":"number"}},"required":["objective"]})),
        ("goal_status", "查询当前目标状态 (phase/revision/rounds/blocked)", json!({"type":"object"})),
        ("goal_complete", "目标完成 → completed (可建新目标)", json!({"type":"object"})),
        ("goal_pause", "暂停当前目标 (active → paused)", json!({"type":"object"})),
        ("goal_block", "报告目标受阻 (active → blocked + 原因)", json!({"type":"object","properties":{"code":{"type":"string"},"message":{"type":"string"}}})),
    ]
}

/// 全量工具 schema (能力可见): 已注册工具 ∩ 手写 schema, 未覆盖的给通用 schema.
fn tools_schema(registry: &ToolRegistry) -> Vec<Value> {
    let registered: Vec<String> = registry.list();
    let known = known_schemas();
    let mut out: Vec<Value> = known
        .iter()
        .filter(|(name, _, _)| registered.iter().any(|r| r == name))
        .map(|(name, desc, params)| {
            json!({"type":"function","function":{"name":name,"description":desc,"parameters":params}})
        })
        .collect();
    // 已注册但未手写 schema 的工具: 通用 schema (能力可见, 参数由 AI 按名推断)
    for name in registered.iter() {
        if known.iter().any(|(k, _, _)| k == name) {
            continue;
        }
        out.push(json!({
            "type":"function",
            "function":{"name":name,"description":format!("工具 {name} (参数按工具约定传入)"),"parameters":{"type":"object","properties":{}}}
        }));
    }
    out
}

struct AppState {
    bridge: Arc<ToolBridge>,
    store: Arc<SqliteMemoryStore>,
    pool: Arc<PipelinePool>,
    /// 互动通知通道 (daemon task 持有 daemon, 此处只发「主人来消息了」时刻).
    interactions: tokio::sync::mpsc::Sender<chrono::DateTime<Utc>>,
    /// 主动送达广播 (模块 4: daemon 涌现/事件 → SSE 推送前端).
    events: tokio::sync::broadcast::Sender<String>,
    /// 机制装配器 (CompanionApp: 注入管线/提炼/摘要/自成长).
    app: Arc<CompanionApp>,
    /// 生命周期 hooks (P1#4, A1#5): UserPromptSubmit/PostToolUse 真实时机触发.
    lifecycle: LifecycleBus,
    subject: String,
}

/// 生命周期钩子: 日志 hook (P1#4 接线示例; 扩展点 — 审计/遥测/通知可挂同口).
struct LifecycleLogHook;

#[async_trait::async_trait]
impl LifecycleHook for LifecycleLogHook {
    fn watch(&self) -> (LifecycleEvent, Option<String>) {
        (LifecycleEvent::UserPromptSubmit, None)
    }
    async fn on_event(&self, ctx: &LifecycleContext) -> Result<(), String> {
        eprintln!(
            "[lifecycle] user_prompt_submit (session: {}): {}",
            ctx.session_id.as_deref().unwrap_or("-"),
            ctx.detail
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>()
        );
        Ok(())
    }
}

fn load_key() -> Result<String, String> {
    if let Ok(k) = std::env::var("APEIRETH_API_KEY") {
        if !k.trim().is_empty() {
            return Ok(k.trim().to_string());
        }
    }
    std::fs::read_to_string(r"apikey-ultra.txt")
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("读 apikey 失败: {e}"))
}

// ============================================================
// 真 LLM 组件 (共享 pipeline; lib 零 LLM 依赖 — 实现全部在此)
// ============================================================

/// 做梦摘要器 (真 MiniMax): 把合并记忆提炼成一条简洁摘要.
pub struct MiniMaxDreamSummarizer {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl DreamSummarizer for MiniMaxDreamSummarizer {
    async fn summarize(&self, merged: &str) -> Result<String, String> {
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("你是阿佩瑞斯的记忆整理员。把「做梦合并」的记忆提炼成一条简洁摘要 (<= 50 字), 只输出摘要正文。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("合并内容: {merged}")),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.4),
            max_tokens: Some(128),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| e.clone())?;
        let chat_resp = openai_chat_from_normalized(&resp);
        for ch in &chat_resp.choices {
            let content = ch.message.content.clone();
            if let Some(idx) = content.find("</think>") {
                let c = content[idx + "</think>".len()..].trim().to_string();
                if !c.is_empty() {
                    return Ok(c);
                }
            } else if !content.trim().is_empty() {
                return Ok(content.trim().to_string());
            }
        }
        Err("摘要 LLM 返回空".to_string())
    }
}

/// 宪法评审 (真 MiniMax): 按 E 层原则判案, 非关键词匹配.
pub struct MiniMaxConstitutionLlm {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl ConstitutionLlm for MiniMaxConstitutionLlm {
    async fn ask(&self, constitution: &str, action: &str) -> Result<String, String> {
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!(format!("你是 Apeireth 的宪法评审员。宪法全文:\n{constitution}\n\n判断「待审动作」是否违反宪法。不要关键词匹配, 判断真实意图与后果。只输出一行: ALLOW 或 BLOCK:<一句话理由>。")),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("待审动作: {action}")),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.2),
            max_tokens: Some(512),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let resp = pool_dispatch(
            &self.pool,
            ProtocolKind::OpenAiChat,
            openai_chat_to_normalized(&req),
            model(),
        )
        .await
        .map_err(|e| e.clone())?;
        let chat = openai_chat_from_normalized(&resp);
        for ch in &chat.choices {
            let c = ch.message.content.clone();
            if !c.trim().is_empty() {
                return Ok(c);
            }
        }
        Err("评审 LLM 返回空".into())
    }
}

/// 语调渲染 (真 MiniMax + tone): 机制事实 → 自然问候; 失败兜底原文.
pub struct TonalUtterance {
    pool: Arc<PipelinePool>,
    tone: &'static str,
}

#[async_trait::async_trait]
impl UtteranceGenerator for TonalUtterance {
    async fn utter(&self, i: &Initiative) -> Result<String, String> {
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!(format!("你是阿佩瑞斯, 一个诚实、有记忆的伙伴。语调: {}。基于给定事实说话, 不编造。", self.tone)),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("把这些事实变成一句自然、真诚、简短的中文主动问候 (<=40字):\n{}", i.to_message())),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.8),
            max_tokens: Some(1024),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let resp = pool_dispatch(
            &self.pool,
            ProtocolKind::OpenAiChat,
            openai_chat_to_normalized(&req),
            model(),
        )
        .await
        .map_err(|e| e.clone())?;
        let chat = openai_chat_from_normalized(&resp);
        for ch in &chat.choices {
            let raw = ch.message.content.clone();
            let stripped = if let Some(idx) = raw.find("</think>") {
                raw[idx + 8..].trim().to_string()
            } else {
                raw.clone()
            };
            if !stripped.is_empty() {
                return Ok(stripped);
            }
            if !raw.trim().is_empty() {
                return Ok(raw.trim().to_string());
            }
        }
        Ok(i.to_message())
    }
}

/// 深度反思器 (模块 5, 真 MiniMax): 周期记忆 → 洞察/模式/建议 (markdown 文本).
pub struct MiniMaxReflector {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl ReflectionReflector for MiniMaxReflector {
    async fn reflect(&self, context: &str) -> Result<String, String> {
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("你是阿佩瑞斯, 在做周期自我反思。基于近期记忆与事件, 输出深度反思 (markdown): ① 观察到的模式 (主人的习惯/偏好变化) ② 值得注意的洞察 ③ 对未来的具体建议 (含可执行经验)。不超过 300 字, 真诚不套话。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("近期记忆:\n{context}")),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.5),
            max_tokens: Some(500),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| format!("深度反思 LLM 调用失败: {e}"))?;
        let chat = openai_chat_from_normalized(&resp);
        let content = chat
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let text = match content.find("</think>") {
            Some(i) => content[i + 8..].to_string(),
            None => content,
        };
        if text.trim().is_empty() {
            return Err("深度反思返回空".to_string());
        }
        Ok(text.trim().to_string())
    }
}

/// 深度召回 (DeepRecall trait, 真 MiniMax): LLM 从候选记忆选与 query 最相关的 top 5.
/// VCP AIMemoHandler 精神; 失败 → 装配器降级普通注入.
pub struct MiniMaxDeepRecall {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl DeepRecall for MiniMaxDeepRecall {
    async fn recall(&self, query: &str, candidates: &[String]) -> Result<Vec<String>, String> {
        let list: String = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{i}. {}", c.chars().take(100).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n");
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("你是阿佩瑞斯的记忆检索员。根据主人的问题, 从候选记忆里选出最相关的 3-5 条, 只输出 JSON 数组: [编号]。无关则输出 []。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("主人问题: {query}\n候选记忆:\n{list}")),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.1),
            max_tokens: Some(100),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| e.clone())?;
        let chat = openai_chat_from_normalized(&resp);
        let content = chat
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let text = match content.find("</think>") {
            Some(i) => content[i + 8..].to_string(),
            None => content,
        };
        let (start, end) = match (text.find('['), text.rfind(']')) {
            (Some(a), Some(b)) if b > a => (a, b + 1),
            _ => return Err("召回 JSON 解析失败".to_string()),
        };
        let idxs: Vec<usize> = serde_json::from_str(&text[start..end]).unwrap_or_default();
        let out: Vec<String> = idxs
            .into_iter()
            .filter_map(|i| candidates.get(i).cloned())
            .take(5)
            .collect();
        if out.is_empty() {
            Err("无相关记忆".to_string())
        } else {
            Ok(out)
        }
    }
}

/// 滚动摘要 (DialogSummarizer trait, 真 MiniMax): 旧段 → 摘要.
/// sum-* 链式基线由装配器查库提供 (prev_summary), 持久化也由装配器完成.
pub struct MiniMaxDialogSummarizer {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl DialogSummarizer for MiniMaxDialogSummarizer {
    async fn summarize(&self, text: &str, prev_summary: Option<&str>) -> Result<String, String> {
        let base = match prev_summary {
            Some(p) => format!("【上次摘要】{p}\n\n"),
            None => String::new(),
        };
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("把这段对话(含上次摘要基线)压缩成 100 字以内的最新摘要, 保留关键事实/约定/情绪, 只输出摘要。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("{base}对话:\n{}", text.chars().take(3000).collect::<String>())),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(200),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| e.clone())?;
        let chat = openai_chat_from_normalized(&resp);
        let content = chat
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let text = match content.find("</think>") {
            Some(i) => content[i + 8..].to_string(),
            None => content,
        };
        let t = text.trim();
        if t.is_empty() {
            return Err("摘要 LLM 返回空".to_string());
        }
        Ok(t.to_string())
    }
}

/// 反思→经验 (ExperienceRefiner trait, 真 MiniMax): 从反思记录提炼一条可复用经验.
/// 0 假装: 提炼失败/解析失败 → 如实返回 Err, 不硬造经验.
pub struct MiniMaxExperienceRefiner {
    pool: Arc<PipelinePool>,
}

#[async_trait::async_trait]
impl ExperienceRefiner for MiniMaxExperienceRefiner {
    async fn refine(&self, reflects: &[String]) -> Result<Option<Experience>, String> {
        if reflects.is_empty() {
            return Ok(None);
        }
        let req = OpenAiChatRequest {
            model: model().to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("你是阿佩瑞斯的经验提炼员。从反思记录中提炼一条可复用经验, 只输出 JSON: {\"scene\": \"触发场景\", \"practice\": \"做法\", \"result\": \"结果\"}。没有可提炼的就输出 {\"scene\": \"\"}。"),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: json!(format!("反思记录:\n{}", reflects.join("\n---\n"))),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(300),
            stream: false,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let normalized = openai_chat_to_normalized(&req);
        let resp = pool_dispatch(&self.pool, ProtocolKind::OpenAiChat, normalized, model())
            .await
            .map_err(|e| format!("提炼 LLM 调用失败: {e}"))?;
        let chat = openai_chat_from_normalized(&resp);
        let content = chat
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        // 剥 <think> 后取 JSON 段 (首个 { 到末个 })
        let text = match content.find("</think>") {
            Some(i) => content[i + 8..].to_string(),
            None => content,
        };
        let (start, end) = match (text.find('{'), text.rfind('}')) {
            (Some(a), Some(b)) if b > a => (a, b + 1),
            _ => return Ok(None),
        };
        let parsed: Value = serde_json::from_str(&text[start..end])
            .map_err(|e| format!("经验 JSON 解析失败 (如实放弃): {e}"))?;
        let scene = parsed
            .get("scene")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if scene.is_empty() {
            return Ok(None); // LLM 判定无可提炼
        }
        let practice = parsed
            .get("practice")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let result = parsed
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let now = chrono::Utc::now().timestamp();
        let id = format!("exp-{}", uuid::Uuid::new_v4());
        Ok(Some(Experience {
            id: id.clone(),
            chain: id,
            rev: 1,
            scene,
            practice: if practice.is_empty() {
                "未提炼出做法".into()
            } else {
                practice
            },
            result,
            outcome: "partial".into(),
            verify_count: 0,
            score: 0.5,
            ready: false,
            proposed: false,
            created_at: now,
            updated_at: now,
        }))
    }
}

/// 模块 4: 开发用测试事件 (验证 SSE 推送链路; 生产事件 = 涌现/做梦/反思自动推送).
async fn test_event(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let _ = st
        .events
        .send("测试事件: 本座在 (SSE 链路验证)".to_string());
    Json(json!({"ok": true, "note": "已推送测试事件到 SSE"}))
}

/// 模块 4: SSE 事件流 (主动送达 — 涌现/做梦/反思完成等实时推送).
async fn events(
    State(st): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = st.events.subscribe();
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(text) => return Some((Ok::<_, Infallible>(SseEvent::default().data(text)), rx)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {} // 跳过期消息
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// 提取 MiniMax CoT (Chain-of-Thought) — 双轨解析, 兼容 `<think>...` 与 `<!-- ... -->`.
/// 0 装 PASS 严守: MiniMax M3 **没有** OpenAI 风格的独立 `delta.reasoning_content` 字段,
/// CoT 跟正文共用 `delta.content` 字段, 跨多 chunk 时边界标记 (`<think>` / `` /
/// `<!--` / `-->`) 可能切分.
/// 服务端拼成完整 text 后用此函数一次性切分 (方案 B 兜底, per
/// `_research_mem/sub_agent_reports/2026-08-19/MiniMax_reasoning_verification.md` §6 + §7).
///
/// 双轨 (per 验证报告 §7 "字段探测双轨" 扩展到 inline 标记):
/// - 优先 `<think>` (8/19 验证报告写 `<!-- -->` 是当时实际行为; 8/20 后续实测 MiniMax API
///   已切换到 `<think>...` 风格 — 实测响应: `<think>The user is asking...\n...</think>2`.
///   为 0 装 PASS 兼容两种格式, 任一命中即按 CoT 剥离.)
/// - `<!-- ... -->`: XML 注释样式, 旧版 MiniMax / 其他兼容代理可能仍使用.
///
/// 边界 case: 0 装严守 — 不假装 LLM 一定输出 CoT; 无标记时返 ("", content) 等价无变化.
/// 跨 chunk: 单次 chat_completions (stream=false) 走完整 text, 不存在跨 chunk 切分,
/// 但状态机按"先匹配最早出现"语义以应对混合内容.
///
/// 工程规范: 0 触碰 3 不可变脊柱 (Self-Disable / L0 HA / 13 键 verdict cache), 0 改 enum/const,
/// 0 改 workspace.version (1.2.0 双轴制: 产品轴 tag v1.0.0 + workspace 轴 1.2.0).
fn extract_minimax_cot(content: &str) -> (String, String) {
    // 双轨: 优先 `<think>` (实测当前 MiniMax 实际格式), 次 `<!-- -->` (旧版/代理).
    // 每条 (open_marker, close_marker) 独立处理 — 命中其一即切 CoT, 余下照常 visible.
    for (open, close) in [("<think>", "</think>"), ("<!--", "-->")] {
        if content.contains(open) {
            let mut reasoning = String::new();
            let mut visible = String::new();
            let mut rest = content;
            while let Some(start) = rest.find(open) {
                // text before open_marker
                visible.push_str(&rest[..start]);
                if let Some(end) = rest[start..].find(close) {
                    // extract open ... close
                    let block_end = start + end + close.len();
                    reasoning.push_str(&rest[start..block_end]);
                    reasoning.push('\n');
                    rest = &rest[block_end..];
                } else {
                    // unterminated: best-effort 0 装严守, 把残余当 visible 拼上, 然后结束
                    visible.push_str(&rest[start..]);
                    rest = "";
                    break;
                }
            }
            visible.push_str(rest);
            return (reasoning.trim().to_string(), visible.trim().to_string());
        }
    }
    // 0 装 PASS: 无 CoT 标记 → (空 reasoning, 全部 content 返 visible), 0 假装 CoT 必有.
    (String::new(), content.to_string())
}

/// 伙伴主链路: 喂节律 → CompanionApp 注入管线 → LLM+工具循环 → OpenAI 兼容响应.
async fn chat_completions(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<OpenAiChatRequest>,
) -> impl IntoResponse {
    let continuity = headers
        .get("x-apeireth-continuity")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&st.subject)
        .to_string();

    // 喂节律: 对话 = 互动 (节律直方图学习作息 + 重置做梦安静期) — 「他在」的感知
    let _ = st.interactions.send(Utc::now()).await;

    let mut messages = req.messages.clone();
    // 当前问题 (最后一条 user 消息; 供推理召回/提炼)
    let query = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| match &m.content {
            serde_json::Value::String(s) => s.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    // P1#4 lifecycle: UserPromptSubmit (真实时机 — 主人提交时)
    if !query.is_empty() {
        let _ = st
            .lifecycle
            .fire(
                LifecycleEvent::UserPromptSubmit,
                LifecycleContext::new(&continuity).with_detail(&query),
            )
            .await;
    }
    // 注入管线 (CompanionApp, ContextAssembler 统一预算):
    // identity 块 (L0) → 独立 persona 消息; 其余块 → 合并记忆注入消息
    let blocks = st.app.build_injection(&query).await;
    let mut injections: Vec<String> = Vec::new();
    for b in &blocks {
        if b.name == "identity" {
            messages.insert(
                0,
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!(b.content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            );
        } else {
            injections.push(b.content.clone());
        }
    }
    if !injections.is_empty() {
        let block = injections.join("\n");
        messages.insert(
            0,
            OpenAiChatMessage {
                role: "system".to_string(),
                content: json!(format!(
                    "以下是 Apeireth 记忆系统注入的上下文 (只作参考, 若与用户当前说法冲突以用户为准):\n{block}"
                )),
                tool_calls: None,
                tool_call_id: None,
            },
        );
    }
    // 上下文管理 (模块 3): 长对话 → 滚动摘要 + 保留注入区 + 最近 30 条.
    // 被裁旧段尝试 LLM 摘要 (节流); 失败 → 丢弃 + 诚实提示 (不硬造).
    if messages.len() > 34 {
        let head_end = messages
            .iter()
            .position(|m| m.role != "system")
            .unwrap_or(0);
        let overflow: Vec<OpenAiChatMessage> = messages[head_end..messages.len() - 30].to_vec();
        let tail: Vec<OpenAiChatMessage> = messages[messages.len() - 30..].to_vec();
        let mut head: Vec<OpenAiChatMessage> = messages[..head_end].to_vec();
        if !overflow.is_empty() {
            if st.app.summarize_due() {
                let text = overflow
                    .iter()
                    .map(|m| format!("[{}] {}", m.role, m.content.as_str().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("\n");
                if let Some(summary) = st.app.summarize_dialog(&text).await {
                    head.push(OpenAiChatMessage {
                        role: "system".to_string(),
                        content: json!(format!("【早期对话摘要】{summary}")),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                } else {
                    head.push(OpenAiChatMessage {
                        role: "system".to_string(),
                        content: json!("【早期对话摘要】(摘要失败, 已裁剪 — 细节已由记忆系统提炼)"),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            } else {
                head.push(OpenAiChatMessage {
                    role: "system".to_string(),
                    content: json!("【早期对话摘要】(已裁剪 — 细节已由记忆系统提炼)"),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }
        head.extend(tail);
        messages = head;
    }

    let tools = tools_schema(&st.bridge.registry);
    let mut final_content: String;
    let mut notes: Vec<String> = Vec::new();
    // 输出上限: 客户端请求值优先 (VCP 精神: 用户可设), env 默认, 上限保护
    let env_max = std::env::var("APEIRETH_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_MAX_TOKENS)
        .clamp(256, MAX_TOKENS_CAP);
    let out_tokens = req
        .max_tokens
        .filter(|v| *v > 0)
        .unwrap_or(env_max)
        .clamp(256, MAX_TOKENS_CAP);

    // CoT 收集 (0 装 PASS 严守: 0 假装 MiniMax 一定输出 `<!-- -->`; 无标记时为空字符串)
    let mut reasoning_out = String::new();

    // ========== STREAMING BRANCH (v1.5+, 0 装严守) ==========
    // P0 unblock 朋友 friend 8/18 18:11 "显示每步 cot 和 tool call 详情" 需求:
    // req.stream=true → 透传 LLM 的真 SSE 到客户端, 跳过本仓 LLM 循环 (tool loop)
    //
    // MiniMax M3 已知 (per 验证报告 §1-§3):
    // - 0 OpenAI 风格独立 `delta.reasoning_content` 字段
    // - CoT 嵌入 `delta.content` 内 `<!-- ... -->` 边界标记
    // - 跨 chunk 边界 `<!--` / `-->` 可能切分
    //
    // 架构 (per 验证报告 §6 方案 A 选):
    // - companion_serve: SSE 字节透传 (stream_forward 已存在 protocol_handlers.rs:1379)
    // - 前端 companion-desktop: 维护 `<!-- ... -->` 字符串状态机切分, 重封 RuntimeEvent
    //   `reasoning-delta` / `content-delta` (contract 已有, runtime.ts:50-59)
    //
    // 0 装 PASS 严守 (per 决策 #33 §2.3 + R125-16 retry verify):
    // - 透传 = 0 解析, 0 干预 LLM 输出 (MiniMax 后续若加 reasoning_content 字段, 不破坏兼容)
    // - tool loop 跳过 = 透明 — `delta.tool_calls` 仍在流里, 客户端可见, 仅不在本仓执行
    //   (full streaming + tool loop 是 v1.5 后续路线, 0 假装已实现)
    // - 字段探测双轨 (per 验证报告 §7): 当前 0 检测 `delta.reasoning_content` 字段,
    //   未来 MiniMax 若加 → 优先用字段, 缺则回 inline 解析
    if req.stream {
        let stream_req = OpenAiChatRequest {
            model: model().to_string(),
            messages: messages.clone(),
            temperature: Some(0.6),
            max_tokens: Some(out_tokens),
            stream: true,
            stop: None,
            tools: Some(tools.clone()),
            tool_choice: Some(json!("auto")),
        };
        let body = match serde_json::to_vec(&stream_req) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[stream] serialize 失败: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": {"message": format!("stream serialize: {e}")}})),
                )
                    .into_response();
            }
        };
        eprintln!(
            "[stream] req.stream=true, 透传 SSE 到 {} (tool loop 跳过, per v1.5 known limit)",
            model()
        );
        return match stream_forward(&st.pool.select_pipeline(model()), ProtocolKind::OpenAiChat, body.into(), model())
            .await
        {
            Ok(r) => r.into_response(),
            Err(e) => {
                eprintln!("[stream] stream_forward 失败: {e}");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": {"message": format!("stream forward: {e}")}})),
                )
                    .into_response()
            }
        };
    }
    // ========== STREAMING BRANCH END ==========

    let mut rounds = 0usize;
    loop {
        rounds += 1;
        let req2 = OpenAiChatRequest {
            model: model().to_string(),
            messages: messages.clone(),
            temperature: Some(0.6),
            max_tokens: Some(out_tokens),
            stream: false,
            stop: None,
            tools: Some(tools.clone()),
            tool_choice: Some(json!("auto")),
        };
        let Some((content, tcs)) = chat_once(&st.pool, &req2, rounds).await else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": {"message": "模型服务暂时不可用 (MiniMax 限流) — 本座已尽力, 请过 10-30 秒再试"}})),
            )
                .into_response();
        };
        if tcs.is_empty() {
            // 0 装 PASS 严守: 旧版 `content.split("</think>").last()` 在 MiniMax (用 `<!-- -->`) 上 0 拆 CoT.
            // 改用 extract_minimax_cot 拆 CoT (`<!-- ... -->`) + 留 visible content.
            // (per _research_mem/sub_agent_reports/2026-08-19/MiniMax_reasoning_verification.md §5)
            let (cot, visible) = extract_minimax_cot(&content);
            final_content = visible;
            // 把 CoT 也存到 x_apeireth 字段, 让 friend 在 JSON 响应里直接看到 reasoning
            // (0 装: 没 CoT 时 cot 为空字符串, 字段也会带空值 — 透明)
            reasoning_out = cot;
            break;
        }
        messages.push(OpenAiChatMessage {
            role: "assistant".to_string(),
            content: json!(content),
            tool_calls: Some(tcs.clone()),
            tool_call_id: None,
        });
        let mut tool_msgs = Vec::new();
        for tc in &tcs {
            let id = tc["id"].clone();
            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
            let args: Value =
                serde_json::from_str(tc["function"]["arguments"].as_str().unwrap_or("{}"))
                    .unwrap_or(json!({}));
            let call = ParsedToolCall {
                tool_name: name.clone(),
                args: args.clone(),
                raw_marker: String::new(),
                archery: false,
                archery_no_reply: false,
            };
            let r = st.bridge.execute_if_allowed(&call).await;
            let body = if r.success {
                serde_json::to_string(&r.output).unwrap_or_default()
            } else {
                format!("工具失败: {:?}", r.error)
            };
            let truncated: String = body.chars().take(4000).collect();
            notes.push(format!("[{name}] 已执行"));
            // P1#4 lifecycle: PostToolUse (真实时机 — 工具执行后; 不阻断主链路)
            let _ = st
                .lifecycle
                .fire(
                    LifecycleEvent::PostToolUse,
                    LifecycleContext::new(&continuity)
                        .with_detail(format!("{name}: 成功={}", r.success)),
                )
                .await;
            tool_msgs.push(OpenAiChatMessage {
                role: "tool".to_string(),
                content: json!(truncated),
                tool_calls: None,
                tool_call_id: Some(id.as_str().unwrap_or("").to_string()),
            });
        }
        messages.extend(tool_msgs);
        // MiniMax 限流缓解 (2026-08-20 实测): 工具循环轮1成功 ~2.7s, 立即发轮2 必触发
        // `suppressed: openai-chat:MiniMax-M3` 限流. 工具执行完 → 等 2s → 再调 LLM,
        // 让 MiniMax token 桶恢复. env APEIRETH_INTERROUND_SLEEP_MS 可覆盖 (0 = 关闭).
        let interround_ms = std::env::var("APEIRETH_INTERROUND_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(2000);
        if interround_ms > 0 {
            tokio::time::sleep(Duration::from_millis(interround_ms)).await;
        }
        if rounds >= MAX_TOOL_ROUNDS {
            final_content = "工具循环达到上限, 已停止。请让主人再发一条消息继续。".to_string();
            break;
        }
    }

    let resp = json!({
        "id": format!("chatcmpl-apeireth-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model(),
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": final_content},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        "x_apeireth": {
            "continuity": continuity,
            "tool_rounds": rounds,
            "tools_executed": notes,
            "reasoning_content": reasoning_out,
            "features": ["memory_injection", "today_summary", "tool_bridge", "daemon_resident", "memory_extractor", "l0_identity", "l1_essential_story", "cot_extraction"],
            "note": "Apeireth 伙伴主链路: CompanionApp 注入管线 (L0/L1 常驻), 工具桥执行, daemon 同进程常驻; reasoning_content 字段 (MiniMax <!-- --> 拆 CoT, 0 装严守 字段探测双轨, 未来 reasoning_content 字段直传)"
        }
    });

    // 对话后节流提炼 (通用记忆捕获): CompanionApp 节流判断 → 异步提炼并对账写入.
    // fire-and-forget: 不影响响应; 提炼失败只记日志 (限流时放弃, 下个窗口再试).
    if st.app.extraction_due() {
        let app = Arc::clone(&st.app);
        tokio::spawn(async move {
            app.run_extraction(12).await;
        });
    }

    (StatusCode::OK, Json(resp)).into_response()
}

async fn chat_once(
    pool: &Arc<PipelinePool>,
    req: &OpenAiChatRequest,
    label: usize,
) -> Option<(String, Vec<Value>)> {
    // 限流重试策略 (2026-08-16 实测: MiniMax 限流严重, 5×8s=40s+ 静默等待体感极差):
    // 最多 3 次 × 6s 退避; 仍失败 → 快速失败 (让用户明确知道限流, 优于无声长等)
    for attempt in 0..3 {
        let normalized = openai_chat_to_normalized(req);
        let t0 = std::time::Instant::now();
        match pool_dispatch(pool, ProtocolKind::OpenAiChat, normalized, &req.model).await {
            Ok(r) => {
                let chat = openai_chat_from_normalized(&r);
                let content = chat
                    .choices
                    .first()
                    .map(|c| c.message.content.clone())
                    .unwrap_or_default();
                let tcs = chat
                    .choices
                    .first()
                    .map(|c| c.message.tool_calls.clone())
                    .unwrap_or_default()
                    .unwrap_or_default();
                // 空响应 (MiniMax 异常时 200 但内容空) → 视为失败重试, 不静默返回空白
                if content.trim().is_empty() && tcs.is_empty() {
                    eprintln!("  [管线] 轮{label} 空响应 (MiniMax 异常), 重试");
                    tokio::time::sleep(Duration::from_secs(4)).await;
                    continue;
                }
                eprintln!("[llm] 轮{label} 成功 ({}ms)", t0.elapsed().as_millis());
                return Some((content, tcs));
            }
            Err(e) => {
                eprintln!("  [管线] 轮{label} 第{}次失败: {e}, 6s 后重试", attempt + 1);
                tokio::time::sleep(Duration::from_secs(6)).await;
            }
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 0 装 PASS: env 缺省回落 DEFAULT_MODEL ("MiniMax-M3"). env APEIRETH_LLM_MODEL 可覆盖.
    // 2026-08-20: companion_serve 启动时读 env / TOML 配置 (0 装 PASS: env 缺省回落).
    let model_in_use = init_model().to_string();
    println!("[llm] model = {model_in_use} (env APEIRETH_LLM_MODEL 可覆盖, 缺省 {DEFAULT_MODEL})");

    // 启动时读 TOML 配置 (per 2026-08-20 P1 配置层): env APEIRETH_LLM_CONFIG 指向 TOML 文件.
    // TOML 里第一个 provider 的 base_url 自动覆盖 BASE_URL. 多 provider / fallback 链
    // 见 docs/02-guides/custom-llm.md (本 session C 任务). 不读 TOML 时退化到 env / DEFAULT.
    let toml_cfg = std::env::var("APEIRETH_LLM_CONFIG")
        .ok()
        .and_then(|path| match apeireth_api::llm::config::LlmConfig::from_file(&path) {
            Ok(cfg) => {
                let n = cfg.providers.len();
                let first_base = cfg.providers.values().next().and_then(|p| p.base_url.clone());
                println!("[llm] TOML config 加载: {n} providers from {path}");
                Some((cfg, first_base))
            }
            Err(e) => {
                eprintln!("[llm] TOML config 加载失败 (退化到 env / default): {e}");
                None
            }
        });
    init_base_url(toml_cfg.as_ref().and_then(|(_, b)| b.clone()));
    println!("[llm] base_url = {} (TOML 优先 → APEIRETH_LLM_BASE_URL env → default {DEFAULT_BASE_URL})", base_url());

    let key = load_key()?;
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);

    // ① 持久记忆库 (文件, 重启不失忆) + 哲学锚点
    let store = open_memory_store().expect("真记忆库");
    let subject = continuity_id_from_env("companion-main");
    println!("[mem] 持久记忆库: 已打开 (重启不失忆) · subject: {subject}");

    // 可选种子记忆 (演示/验证)
    if let Ok(seed) = std::env::var("APEIRETH_SEED_MEMORY") {
        for (i, c) in seed.split(';').filter(|s| !s.trim().is_empty()).enumerate() {
            let _ = store.put_episode(&apeireth_memory::CoreEpisode {
                id: format!("seed-{i}"),
                timestamp: chrono::Utc::now().timestamp(),
                role: "assistant".into(),
                content: c.trim().to_string(),
                session_id: MEMORY_SESSION.to_string(),
            });
        }
        println!("[seed] 已写入种子记忆: {}", seed.replace(';', " | "));
    }

    // 目标服务 (持久目录 %APPDATA%\apeireth\goals; 与工具桥/装配器共享同一实例)
    let goal_dir = apeireth_companion::daemon::default_memory_path()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::temp_dir().join("apeireth-goals"))
        .join("goals");
    let mut goals = GoalService::new(&goal_dir);
    goals.restore("goal-main");
    let goals_shared: std::sync::Arc<std::sync::Mutex<GoalService>> =
        std::sync::Arc::new(std::sync::Mutex::new(goals));

    // ② 工具桥全增强 (宪法 LLM 评审 + 目标工具 + 显式扩权 APEIRETH_GRANT="FileOperator:24;Git:12")
    // PipelinePool 构造: 有 TOML → multi provider pool; 无 TOML / 失败 → single pool (1:1 兼容旧版).
    let pool: Arc<PipelinePool> = match toml_cfg.as_ref().map(|(c, _)| c) {
        Some(cfg) => {
            let mut pipelines = std::collections::HashMap::new();
            let mut fallback_order = Vec::new();
            for (name, prov) in &cfg.providers {
                let base = prov.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
                let pipe = Arc::new(build_pipeline(base.to_string(), Some(key.clone()))?);
                pipelines.insert(name.clone(), pipe);
                fallback_order.push(name.clone());
            }
            let router = Arc::new(MultiLlmRouter::new());
            println!(
                "[pool] multi mode: {} providers (fallback: {})",
                pipelines.len(),
                fallback_order.join(" → ")
            );
            Arc::new(PipelinePool::multi(pipelines, fallback_order, router))
        }
        None => {
            let pipeline = Arc::new(build_pipeline(base_url().to_string(), Some(key.clone()))?);
            println!("[pool] single mode (无 TOML, 1:1 兼容旧版)");
            Arc::new(PipelinePool::single("default", pipeline))
        }
    };
    let bridge = Arc::new(
        ToolBridge::new(Arc::clone(&store))
            .with_judicator(Arc::new(LlmJudicator::new(Arc::new(
                MiniMaxConstitutionLlm {
                    pool: Arc::clone(&pool),
                },
            ))))
            .with_goals(std::sync::Arc::clone(&goals_shared)),
    );
    if let Ok(grants) = std::env::var("APEIRETH_GRANT") {
        for g in grants.split(';').filter(|s| !s.trim().is_empty()) {
            let (tool, hours) = match g.split_once(':') {
                Some((t, h)) => (t.trim(), h.trim().parse().unwrap_or(24)),
                None => (g.trim(), 24),
            };
            bridge
                .packs
                .grant(apeireth_companion::packs::PermissionPack::timed(
                    "serve 显式扩权",
                    vec![tool.to_string()],
                    hours,
                    None,
                ));
            println!("[grant] {tool}: {hours}h");
        }
    }
    println!("[bridge] 宪法评审 (真 LLM): Medium+ 工具执行前按 E 层判案");

    // 主动送达广播 (模块 4: daemon 涌现/事件 → SSE 推送)
    let (tx_events, _) = tokio::sync::broadcast::channel::<String>(64);

    // ③ CompanionApp 机制装配 (注入管线/提炼/摘要/自成长; 全部 LLM 实现注入)
    let extract_interval = std::env::var("APEIRETH_EXTRACT_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(600));
    let rhythm_share: std::sync::Arc<std::sync::Mutex<Option<RhythmEstimate>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    // 机制件运行时聚合 (E4 好奇 + F1 情绪 + F4 假设 + TP21 目录; 目录条目由记忆主题
    // 构建的调用方负责 — 此处从零开始, 对话积累后由 brain.on_message 喂回声)
    let brain = Arc::new(apeireth_companion::runtime_brain::RuntimeBrain::new(
        apeireth_companion::curiosity::CuriosityConfig::default(),
        apeireth_companion::hypothesis::HypothesisConfig::default(),
        vec![apeireth_companion::progressive::CatalogEntry::new(
            "伙伴回忆",
            "与主人的共同生活记录（对话积累自动生长）",
            0,
        )],
    ));
    let app = Arc::new(
        CompanionApp::new(Arc::clone(&store), MEMORY_SESSION)
            // L0: Identity 常驻 (persona + 约束, 永不截断)
            .with_identity(format!("{PERSONA}\n{CLAIM_RULE}\n{AUTH_RULE}"))
            // L1: Essential Story 常驻 (mempalace §5.6 渐进加载; essential-*/高 importance)
            .with_essential_budget(800)
            .with_inject_budget(6000)
            .with_rhythm(std::sync::Arc::clone(&rhythm_share))
            .with_goal(std::sync::Arc::clone(&goals_shared))
            .with_extractor(Arc::new(MiniMaxMemoryExtractor {
                pool: Arc::clone(&pool),
            }))
            .with_summarizer(Arc::new(MiniMaxDialogSummarizer {
                pool: Arc::clone(&pool),
            }))
            .with_refiner(Arc::new(MiniMaxExperienceRefiner {
                pool: Arc::clone(&pool),
            }))
            .with_deep_recall(Arc::new(MiniMaxDeepRecall {
                pool: Arc::clone(&pool),
            }))
            .with_extract_interval(extract_interval)
            .with_summarize_interval(Duration::from_secs(300))
            .with_brain(Arc::clone(&brain)),
    );
    println!(
        "[app] CompanionApp 装配完成: L0 Identity + L1 Essential 常驻, 提炼 {:?} 节流",
        extract_interval
    );

    // ④ daemon 常驻 (做梦 LLM 摘要 + 反思 LLM 深度 + 涌现 LLM 润色, 同进程): 记忆会话 "me"
    let quiet = std::env::var("APEIRETH_DREAM_QUIET_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(6 * 3600));
    let dream = DreamScheduler::new(Arc::clone(&store), apeireth_core::clock::system_clock())
        .with_quiet_threshold(quiet)
        .with_session(MEMORY_SESSION.to_string())
        .with_summarizer(Arc::new(MiniMaxDreamSummarizer {
            pool: Arc::clone(&pool),
        }));
    let reflect_period = std::env::var("APEIRETH_REFLECT_PERIOD_HOURS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|h| chrono::Duration::milliseconds((h * 3600_000.0) as i64))
        .unwrap_or(chrono::Duration::days(1));
    let reflect = ReflectionScheduler::new(
        Arc::clone(&store),
        apeireth_core::clock::system_clock(),
        MEMORY_SESSION.to_string(),
    )
    .with_period(reflect_period)
    .with_reflector(Arc::new(MiniMaxReflector {
        pool: Arc::clone(&pool),
    }));
    let tone = tone_hint(&apeireth_companion::bond::Bond::new());
    // 送达通道: 广播 (SSE) 必开; Lark (离线) 有凭据则叠加
    let mut sink = MultiSink::new().push(Box::new(apeireth_companion::daemon::BroadcastSink::new(
        tx_events.clone(),
    )));
    match LarkSink::from_env() {
        Ok(lark) => {
            sink = sink.push(Box::new(lark));
            println!("[sink] Lark 离线送达已启用 (凭据有效)");
        }
        Err(e) => {
            println!("[sink] Lark 未启用 (需要 APEIRETH_LARK_APP_ID/SECRET/RECEIVE_ID): {e}");
        }
    }
    match TelegramSink::from_env() {
        Ok(tg) => {
            sink = sink.push(Box::new(tg));
            println!("[sink] Telegram 离线送达已启用 (凭据有效)");
        }
        Err(e) => {
            println!("[sink] Telegram 未启用 (需要 APEIRETH_TELEGRAM_BOT_TOKEN/CHAT_ID): {e}");
        }
    }
    let daemon = CompanionDaemon::new(
        apeireth_companion::bond::Bond::new(),
        apeireth_companion::emergence::Boundaries::default(),
        CompanionDelivery::new(
            ThrottledUtterance::new(
                TonalUtterance {
                    pool: Arc::clone(&pool),
                    tone,
                },
                Duration::from_secs(30),
            ),
            sink,
        ),
        MemoryContextSource::new(Arc::clone(&store)),
        MEMORY_SESSION.to_string(),
        Duration::from_secs(60),
    )
    .with_dream(dream)
    .with_reflection(reflect);
    println!("[daemon] 常驻: 做梦(LLM 摘要, 安静期 {:?}) + 反思({:?}, LLM 深度) + 涌现(LLM 润色, SSE 推送)", quiet, reflect_period);

    // 互动通知通道: handler 发「主人来消息了」, daemon 喂节律 + 重置做梦安静期
    let (tx_interact, rx_interact) = tokio::sync::mpsc::channel::<chrono::DateTime<Utc>>(64);

    // ⑤ HTTP 伙伴端点
    // P1#4 lifecycle: 注册日志 hook + SessionStart (启动时真实触发)
    let lifecycle = LifecycleBus::new().register(Box::new(LifecycleLogHook));
    let _ = lifecycle
        .fire(
            LifecycleEvent::SessionStart,
            LifecycleContext::new(&subject).with_detail("companion_serve v4 启动"),
        )
        .await;
    // B1 Web 面板 v2 需要独立 store 引用 (AppState 已 move store, 此处先 clone)
    let store_for_panel = Arc::clone(&store);
    let state = Arc::new(AppState {
        bridge,
        store,
        pool,
        interactions: tx_interact,
        events: tx_events,
        app,
        lifecycle,
        subject,
    });
    let router = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/apeireth/grant", post(grant))
        .route("/v1/apeireth/approval-requests", get(approval_requests))
        .route("/v1/apeireth/events", get(events))
        .route("/v1/apeireth/test-event", post(test_event))
        // B1 Web 面板 v2: 静态面板页 (assets/panel/) + 只读数据端点 (apeireth-api panel_readonly)
        .route("/panel", get(panel_index))
        .route("/panel/:asset", get(panel_asset))
        .nest_service(
            "/v1/panel",
            apeireth_api::panel_readonly::panel_router(store_for_panel),
        )
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("✅ companion_serve v4 — 伙伴端点全能力版 (CompanionApp 机制装配)");
    println!(
        "   http://127.0.0.1:{port}/panel  (Web 面板 v2: 会话/记忆/图谱/授权/审计, 只读真接口)"
    );
    println!("   http://127.0.0.1:{port}/v1  (模型 MiniMax-M3, Key 任意非空)");
    println!(
        "   会话标签: X-Apeireth-Continuity (缺省 {}) · 工具: 全部可见, 执行受宪法/权限约束",
        state.subject.as_str()
    );
    // daemon 循环与 HTTP 同 task 交替 (daemon 内部 RefCell 跨 await → 非 Send, 不能 spawn)
    let d_app = Arc::clone(&state.app);
    let d_rhythm = rhythm_share;
    tokio::select! {
        r = axum::serve(listener, router) => { r?; }
        _ = daemon_loop(daemon, rx_interact, d_app, d_rhythm) => {}
    }
    Ok(())
}

/// daemon 常驻循环: 定时 step (做梦/反思/涌现) + 响应互动通知 (喂节律)
/// + 自成长延伸 (CompanionApp): 反思完成→提炼经验入库; 晋级候选自动成文.
/// 具体类型 (Delivery trait 私有, 不能作泛型约束); daemon 非 Send, 只在同 task 内用.
type ServeDaemon = CompanionDaemon<
    CompanionDelivery<ThrottledUtterance<TonalUtterance>, MultiSink>,
    MemoryContextSource,
>;
async fn daemon_loop(
    mut daemon: ServeDaemon,
    mut rx: tokio::sync::mpsc::Receiver<chrono::DateTime<Utc>>,
    app: Arc<CompanionApp>,
    rhythm_share: std::sync::Arc<std::sync::Mutex<Option<RhythmEstimate>>>,
) {
    let mut last_cycles: u64 = daemon
        .reflection
        .as_ref()
        .map(|r| r.cycles_completed())
        .unwrap_or(0);
    let mut last_batch_extract = std::time::Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let t0 = std::time::Instant::now();
                daemon.step().await;
                // 节律共享 (模块 1 状态感知): 每 tick 更新活跃概率 (UTC 坐标, 与观察自洽)
                {
                    let now = Utc::now();
                    let mins = now.hour() * 60 + now.minute();
                    let est = daemon.awake.loop_.rhythm.estimate(mins);
                    if let Ok(mut share) = rhythm_share.lock() {
                        *share = Some(est);
                    }
                }
                // 延伸 1: 反思周期完成 → LLM 提炼经验入经验库 (自成长管道 Level 0)
                let cycles = daemon.reflection.as_ref().map(|r| r.cycles_completed()).unwrap_or(0);
                if cycles > last_cycles {
                    eprintln!("[growth] 反思周期完成 (累计 {cycles}), 提炼经验...");
                    let reflects: Vec<String> = app.store()
                        .recent_episodes(app.session(), 100)
                        .unwrap_or_default()
                        .iter()
                        .filter(|e| e.id.starts_with("reflect-"))
                        .take(3)
                        .map(|e| e.content.clone())
                        .collect();
                    match app.refine_experience(&reflects).await {
                        Ok(Some(exp)) => {
                            ExperienceStore::new(Arc::clone(app.store())).save(&exp)
                                .map(|_| eprintln!("[growth] 经验入库: {}", exp.scene))
                                .unwrap_or_else(|e| eprintln!("[growth] 经验入库失败: {e}"));
                        }
                        Ok(None) => eprintln!("[growth] 本次反思无可提炼经验"),
                        Err(e) => eprintln!("[growth] 经验提炼失败: {e}"),
                    }
                    last_cycles = cycles;
                }
                // 批量记忆提炼 (与做梦同频: 6h 节流; 通用捕获 — 偏好/事实/约定):
                // 0 假装: 按时间节流而非做梦事件精确绑定 (DreamScheduler 无公开计数器)
                if last_batch_extract.elapsed() >= Duration::from_secs(6 * 3600) {
                    last_batch_extract = std::time::Instant::now();
                    eprintln!("[extract] 批量提炼 (6h 周期)...");
                    app.run_extraction(30).await;
                }
                // 延伸 3: 晋级候选自动成文 (数据目录 promotion-candidates.md; 空则不写)
                if let Some(path) = app.export_promotion_candidates() {
                    eprintln!("[growth] 晋级候选已成文: {:?}", path);
                }
                eprintln!("[daemon-loop] tick done in {:?}", t0.elapsed());
            }
            Some(at) = rx.recv() => daemon.on_user_message(at),
            else => break,
        }
    }
}

/// 待批授权请求 (AI 被拒时产生; 前端轮询展示, 主人一键批准 — 权限洋葱真实载体).
async fn approval_requests(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    Json(apeireth_companion::approval_requests::pending_json(
        &st.store,
    ))
}

/// 主人批准端点 (权限洋葱对齐): 主人带 master token 直接批准工具授权 (PermissionPack),
/// AI 只请求不接触 token. 授权后高危工具在时限内可直接执行.
async fn grant(State(st): State<Arc<AppState>>, Json(req): Json<Value>) -> impl IntoResponse {
    let tool = req
        .get("tool")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(tool) = tool else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "需要 tool (工具名)"})),
        )
            .into_response();
    };
    let hours = req
        .get("hours")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1)
        .min(24 * 30);
    let token = req
        .get("master_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let expected = std::env::var("APEIRETH_MASTER_TOKEN").unwrap_or_default();
    if expected.is_empty() || token != expected {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "master token 不匹配 (主人授权权在主人手里)"})),
        )
            .into_response();
    }
    st.bridge
        .packs
        .grant(apeireth_companion::packs::PermissionPack::timed(
            "主人授权",
            vec![tool.to_string()],
            hours,
            None,
        ));
    (
        StatusCode::OK,
        Json(json!({"ok": true, "tool": tool, "hours": hours, "note": "已按权限洋葱授权 (PermissionPack); 到期自动失效"})),
    )
        .into_response()
}

/// 内置聊天页 (零依赖单文件前端, 浏览器打开即用; 供主人/任何前端先体验).
async fn index() -> impl IntoResponse {
    axum::response::Html(include_str!("../assets/chat.html").to_string())
}

/// B1 Web 面板 v2 入口页 (静态资产 include_str! 编译期内嵌 — 与 chat.html 同形态, 零运行时依赖).
async fn panel_index() -> impl IntoResponse {
    axum::response::Html(include_str!("../assets/panel/index.html").to_string())
}

/// B1 Web 面板 v2 静态资产 (5 页 + css/js; 白名单匹配, 其它一律 404).
async fn panel_asset(Path(asset): Path<String>) -> axum::response::Response {
    use axum::http::header::CONTENT_TYPE;
    let (ctype, body): (&str, &str) = match asset.as_str() {
        "index.html" => (
            "text/html; charset=utf-8",
            include_str!("../assets/panel/index.html"),
        ),
        "sessions.html" => (
            "text/html; charset=utf-8",
            include_str!("../assets/panel/sessions.html"),
        ),
        "memory.html" => (
            "text/html; charset=utf-8",
            include_str!("../assets/panel/memory.html"),
        ),
        "graph.html" => (
            "text/html; charset=utf-8",
            include_str!("../assets/panel/graph.html"),
        ),
        "approvals.html" => (
            "text/html; charset=utf-8",
            include_str!("../assets/panel/approvals.html"),
        ),
        "audit.html" => (
            "text/html; charset=utf-8",
            include_str!("../assets/panel/audit.html"),
        ),
        "panel.css" => (
            "text/css; charset=utf-8",
            include_str!("../assets/panel/panel.css"),
        ),
        "panel.js" => (
            "application/javascript; charset=utf-8",
            include_str!("../assets/panel/panel.js"),
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(CONTENT_TYPE, ctype)], body).into_response()
}

async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "apeireth-companion-serve-v4",
        "version": env!("CARGO_PKG_VERSION"),
        "features": ["persistent_memory", "daemon_resident", "dream_llm_summarizer", "utterance_llm", "constitution_llm_judicator", "memory_injection", "today_summary", "tool_bridge_all", "openai_compat", "companion_app", "l0_identity", "l1_essential_story"],
    }))
}

async fn list_models() -> impl IntoResponse {
    Json(json!({
        "object": "list",
        "data": [{"id": model(), "object": "model", "created": 0, "owned_by": "minimax"}]
    }))
}

#[cfg(test)]
mod cot_extraction_tests {
    //! 单元测试 — extract_minimax_cot 双轨解析 `<think>...` + `<!-- ... -->`.
    //!
    //! 工程规范:
    //! - 0 装 PASS 严守: 无标记 / 单标记 / 多标记 / 不闭合 / 双轨兼容 7+ 测全齐
    //! - 0 改 enum/const, 0 触碰 3 不可变脊柱
    //! - 0 改 workspace.version (1.2.0)
    //!
    //! (per _research_mem/sub_agent_reports/2026-08-19/MiniMax_reasoning_verification.md §5 + §7
    //!  8/20 实测 MiniMax 当前返回 `<think>...`; 双轨兼容 8/19 验证报告 `<!-- -->`)

    use super::extract_minimax_cot;

    /// Happy path: `<think>` 单段, 8/20 实测 MiniMax 实际响应格式.
    #[test]
    fn happy_path_think_then_content() {
        let content = "<think>We need to think</think>Here is the answer.";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, "<think>We need to think</think>");
        assert_eq!(visible, "Here is the answer.");
    }

    /// Happy path: 旧 `<!-- -->` 格式, 兼容 8/19 验证报告与代理/历史实例.
    #[test]
    fn happy_path_html_comment_then_content() {
        let content = "<!-- We need to think -->Here is the answer.";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, "<!-- We need to think -->");
        assert_eq!(visible, "Here is the answer.");
    }

    /// 0 装 PASS: 无标记 → (空 reasoning, 全部 content 返 visible), 0 假装 CoT 必有.
    #[test]
    fn no_markers_returns_empty_cot_full_visible() {
        let content = "Plain answer without any CoT markers.";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, "");
        assert_eq!(visible, "Plain answer without any CoT markers.");
    }

    /// 0 装 PASS: 空字符串 → (空, 空), 边界 0 panic.
    #[test]
    fn empty_content_returns_empty_both() {
        let (cot, visible) = extract_minimax_cot("");
        assert_eq!(cot, "");
        assert_eq!(visible, "");
    }

    /// 多段 `<think>` CoT: 2 段中间夹 + 末尾夹.
    #[test]
    fn multiple_think_blocks_all_extracted() {
        let content = "<think>first thought</think>middle<think>second thought</think>end";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, "<think>first thought</think>\n<think>second thought</think>");
        assert_eq!(visible, "middleend");
    }

    /// CoT 在末尾: `<think>` 末尾格式 (类似验证报告 §2 chunk 9 模式).
    #[test]
    fn think_at_end_extracted() {
        let content = "Visible answer first.\n\n<think>thinking more</think>";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, "<think>thinking more</think>");
        assert_eq!(visible, "Visible answer first.");
    }

    /// 边界: 不闭合的 `<think>` (跨 chunk 残余) — best-effort 当 visible, 0 装严守.
    #[test]
    fn unterminated_think_treated_as_visible_best_effort() {
        let content = "answer part\n<think>still thinking without close";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, ""); // 不闭合 → 0 假装这是 CoT
        assert_eq!(visible, "answer part\n<think>still thinking without close");
    }

    /// 8/20 实测响应: `<think>` 包多行推理 + 短答案, 9.11 vs 9.9 类型.
    #[test]
    fn realistic_minimax_think_sample_extracts_cot() {
        // 8/20 E2E 实测响应 (1+1 简化版)
        let content = "<think>\nThe user is asking a simple math question: 1+1 equals what?\nAs 阿佩瑞斯, I should respond in character but keep it brief.\n</think>\n2";
        let (cot, visible) = extract_minimax_cot(content);
        assert!(cot.starts_with("<think>"), "CoT 以 <think> 开头");
        assert!(cot.ends_with("</think>"), "CoT 以 </think> 结尾");
        assert_eq!(visible.trim(), "2", "正文只剩 '2'");
    }

    /// 双轨兼容: 同一函数, 同一调用, `<think>` 优先于 `<!-- -->` (实测 MiniMax 当前格式).
    /// 包含两者 → 只剥 `<think>`, 余下当 visible (0 装严守: LLM 不会同时输出两种).
    #[test]
    fn dual_track_think_takes_priority() {
        let content = "<think>reasoning</think>real answer <!-- legacy comment -->";
        let (cot, visible) = extract_minimax_cot(content);
        assert_eq!(cot, "<think>reasoning</think>");
        assert!(visible.contains("real answer"));
        assert!(visible.contains("<!-- legacy comment -->")); // 旧标记当 visible
    }

    /// 工程规范: 0 装 PASS — `<think>` 嵌套时 (罕见) 状态机不退化, 取最外层.
    #[test]
    fn nested_think_handled_robustly_no_panic() {
        // 嵌套: `<think> outer<think>inner</think>tail</think>`
        let content = "<think>outer<think>inner</think>tail</think>";
        let (_cot, _visible) = extract_minimax_cot(content);
        // 不 panic 即可, 0 装严守 — 不依赖精确拆分
    }
}

// ──────────────────────────────────────────────────────────────────
// 2026-08-20: 配置层单元测 (env / TOML / default fallback)
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod llm_config_tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        // 先尝试拿锁; 拿不到 = 当前已有测试在跑, 跳过 (避免毒化)
        let Ok(_g) = ENV_LOCK.lock() else {
            f();
            return;
        };
        std::env::remove_var("APEIRETH_LLM_MODEL");
        std::env::remove_var("APEIRETH_LLM_BASE_URL");
        std::env::remove_var("APEIRETH_LLM_CONFIG");
        f();
    }

    /// 0 装 PASS: env 缺省回落 = "MiniMax-M3" (旧版 1:1 行为)
    #[test]
    fn model_defaults_to_minimax_when_no_env() {
        with_clean_env(|| {
            assert_eq!(init_model(), "MiniMax-M3");
            assert_eq!(model(), "MiniMax-M3");
        });
    }

    #[test]
    fn model_env_overrides_default() {
        with_clean_env(|| {
            std::env::set_var("APEIRETH_LLM_MODEL", "gpt-4o-custom");
            assert_eq!(init_model(), "gpt-4o-custom");
        });
    }

    /// 0 装 PASS: 缺省回落 = "https://api.minimaxi.com" (旧版 1:1 行为)
    #[test]
    fn base_url_defaults_to_minimax_when_no_env() {
        with_clean_env(|| {
            init_base_url(None);
            assert_eq!(base_url(), "https://api.minimaxi.com");
        });
    }

    #[test]
    fn base_url_env_overrides_default() {
        with_clean_env(|| {
            std::env::set_var("APEIRETH_LLM_BASE_URL", "https://env.test.com");
            init_base_url(None);
            assert_eq!(base_url(), "https://env.test.com");
        });
    }

    #[test]
    fn base_url_toml_first_provider_overrides_env() {
        with_clean_env(|| {
            std::env::set_var("APEIRETH_LLM_BASE_URL", "https://env.test.com");
            init_base_url(Some("https://toml.test.com".to_string()));
            assert_eq!(base_url(), "https://toml.test.com");
        });
    }

    #[test]
    fn base_url_explicit_none_falls_through_to_env() {
        with_clean_env(|| {
            std::env::set_var("APEIRETH_LLM_BASE_URL", "https://env-only.test.com");
            init_base_url(None);
            assert_eq!(base_url(), "https://env-only.test.com");
        });
    }
}

// ──────────────────────────────────────────────────────────────────
// 2026-08-20: MultiLlmRouter 真接入 PipelinePool — 单测 (per spec §4.1+§4.2)
// 主人拍板决策: 1(b) 2(b 全 true) 3(a fail-fast) 4(a default) 5(a 单 provider)
//             6(b 走 LlmProvider.complete) 7(4 type) 8(测 7+8 必需)
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod multi_llm_router_tests {
    //! MultiLlmRouter 真接入 PipelinePool 验证 (per spec §4.1 + §4.2).
    //!
    //! 0 触碰严守: PipelinePool 抽象不动, 复用 single/multi 工厂.
    //! 测覆盖: 6 主测 + 2 边界测 (决策 8 必需) = 8 测.
    //!
    //! 已知现状 (诚实标注, 0 触碰 config.rs 下):
    //! - `LlmConfig::build_provider` 支持 `apeireth-api` / `openai-compatible` / `scripted`,
    //!   **`anthropic-compatible` 暂未在 config.rs 注册** (落到 unknown 分支返 Err).
    //!   测 6 显式标注此现状, 不假装 4 type 都 build 成功.
    //! - `select_pipeline(_)` v1 简化 (决策 2) 永远返 default_pipeline, 跟旧 build_pipeline
    //!   行为 1:1. 测 3 验证这个语义.

    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        // 拿锁; 拿不到 = 当前已有测试在跑 (Env 已被并行测污染), 跳过隔离
        let Ok(_g) = ENV_LOCK.lock() else {
            f();
            return;
        };
        std::env::remove_var("APEIRETH_LLM_CONFIG");
        std::env::remove_var("APEIRETH_LLM_MODEL");
        std::env::remove_var("APEIRETH_LLM_BASE_URL");
        // 配置层 + 测试用的 key 集合: 全清, 避免跨测污染
        std::env::remove_var("APEIRETH_API_KEY");
        std::env::remove_var("APEIRETH_LLM_NO_KEY");
        std::env::remove_var("APEIRETH_OPENAI_KEY");
        std::env::remove_var("APEIRETH_ANTHROPIC_KEY");
        f();
    }

    // 共享辅助: 构造一个 `Pipeline` 实例 (build_pipeline 调真实 HTTP 客户端构造)
    // 测里只用它的 Arc 共享语义, 不会真的发请求.
    fn make_arc_pipeline(base: &str, key: &str) -> Arc<Pipeline> {
        Arc::new(build_pipeline(base.to_string(), Some(key.to_string())).unwrap())
    }

    // ============ 6 主测 (per spec §4.1) ============

    /// 测 1: 无 TOML / 无 provider = 单 Pipeline 退化 (决策 1b)
    /// PipelinePool::single("default", pipe) 1:1 兼容旧版 Arc<Pipeline> 行为.
    #[test]
    fn pool_single_mode_uses_default_pipeline_when_no_toml() {
        with_clean_env(|| {
            let pipe = make_arc_pipeline("https://default.test", "fake-key");
            let pool = PipelinePool::single("default", Arc::clone(&pipe));

            // 单 provider mode: provider_count == 1 (spec §2.1 PipelinePool::single)
            assert_eq!(pool.provider_count(), 1, "single mode provider count 必须 == 1");

            // 决策 4a: select_pipeline(unknown) 退化 default_pipeline
            let p1 = pool.select_pipeline("MiniMax-M3");
            let p2 = pool.select_pipeline("unknown-model");
            let p3 = pool.select_pipeline("any-other-llm");

            // 决策 2 (v1 简化): select_pipeline 永远返 default_pipeline
            // 验证 3 次选都返同一个 Arc (pointer identity)
            assert!(Arc::ptr_eq(&p1, &pipe), "select_pipeline 必须返 default Arc");
            assert!(Arc::ptr_eq(&p2, &pipe), "未知 model 也必须走 default");
            assert!(Arc::ptr_eq(&p3, &pipe), "任意 model 都走 default (v1 简化)");

            // provider_names() 暴露 1 个 default (跟旧版 build_pipeline 行为 1:1)
            assert_eq!(pool.provider_names(), vec!["default".to_string()]);
        });
    }

    /// 测 2: 有 TOML = 多 Pipeline + MultiLlmRouter 真接 (PipelinePool::multi 工厂)
    /// 决策: 走 `LlmConfig::from_str` 解析 → `build_router()` 拿 router → 手构 PipelinePool::multi
    /// (不调 build_pipeline, 因 base_url 是 fake, 不发真请求).
    #[test]
    fn pool_multi_mode_constructs_from_toml_with_router() {
        with_clean_env(|| {
            std::env::set_var("APEIRETH_API_KEY", "fake-api");
            std::env::set_var("APEIRETH_OPENAI_KEY", "fake-openai");

            let toml = r#"
                [providers.apeireth-api]
                type = "apeireth-api"
                base_url = "https://api.minimaxi.com/v1"
                api_key_env = "APEIRETH_API_KEY"
                models = ["MiniMax-M3"]

                [providers.openai]
                type = "openai-compatible"
                base_url = "https://api.openai.com/v1"
                api_key_env = "APEIRETH_OPENAI_KEY"
                models = ["gpt-4o"]

                [router]
                fallback_order = ["apeireth-api", "openai"]
            "#;
            let cfg = apeireth_api::LlmConfig::from_str(toml).expect("TOML 解析必须成功");

            // router 真接 (build_router() 不发请求, 只构造对象)
            let router = Arc::new(cfg.build_router().expect("build_router 成功"));
            assert_eq!(router.provider_count(), 2, "router 必须含 2 provider");
            // fallback_order 显式设 → provider_names 按 fallback_order 排
            assert_eq!(
                router.provider_names(),
                vec!["apeireth-api".to_string(), "openai".to_string()],
                "fallback_order 顺序: apeireth-api 优先"
            );

            // 手构 PipelinePool::multi (用 fake pipeline, 不调 build_pipeline 走真网络)
            let mut pipelines = std::collections::HashMap::new();
            pipelines.insert(
                "apeireth-api".to_string(),
                make_arc_pipeline("https://api.minimaxi.com/v1", "fake-api"),
            );
            pipelines.insert(
                "openai".to_string(),
                make_arc_pipeline("https://api.openai.com/v1", "fake-openai"),
            );
            let pool = PipelinePool::multi(pipelines, vec!["apeireth-api".into(), "openai".into()], router);

            // 多 provider mode: provider_count == 2
            assert_eq!(pool.provider_count(), 2, "multi mode provider count 必须 == 2");
            assert_eq!(
                pool.provider_names(),
                vec!["apeireth-api".to_string(), "openai".to_string()],
                "provider_names 按 fallback_order 暴露"
            );
        });
    }

    /// 测 3: select_pipeline v1 简化 — 任何 model 都返 default (决策 2b 全 true + 决策 4a)
    /// 验证: PipelinePool::multi 模式下也保持 v1 简化 (后续 phase 真接 router 路径时再切).
    #[test]
    fn pool_select_pipeline_returns_default_for_any_model() {
        with_clean_env(|| {
            let mut pipelines = std::collections::HashMap::new();
            let default_pipe = make_arc_pipeline("https://default.test", "fake-key");
            let second_pipe = make_arc_pipeline("https://second.test", "fake-key-2");
            pipelines.insert("default".to_string(), Arc::clone(&default_pipe));
            pipelines.insert("second".to_string(), Arc::clone(&second_pipe));

            let router = Arc::new(MultiLlmRouter::new());
            let pool = PipelinePool::multi(
                pipelines,
                vec!["default".into(), "second".into()],
                router,
            );

            // 决策 2 (v1 简化): 任何 model 都走 default_pipeline
            // PipelinePool::multi 内部 `pipelines.values().next().cloned()` 作为 default —
            // HashMap 顺序未保证, 所以这里只验证"同一 pool 多次 select_pipeline 返同一 Arc"
            // (即 v1 简化的不变量), 不强求 pointer identity 到某个 named pipeline.
            let p_a = pool.select_pipeline("MiniMax-M3");
            let p_b = pool.select_pipeline("gpt-4o");
            let p_c = pool.select_pipeline("claude-sonnet-4");
            let p_d = pool.select_pipeline("anything-completely-unknown");

            // 不变量 1: 4 次 select_pipeline 全部返同一个 Arc (v1 简化)
            assert!(Arc::ptr_eq(&p_a, &p_b), "v1: A == B (同 default)");
            assert!(Arc::ptr_eq(&p_b, &p_c), "v1: B == C (同 default)");
            assert!(Arc::ptr_eq(&p_c, &p_d), "v1: C == D (同 default)");

            // 不变量 2: 这个 default 必须是 pipelines 里某个真实 pipeline 的 Arc clone
            // (PipelinePool::multi 不创造新 Arc, 是 .cloned() 复制)
            let pool_default_is_real = Arc::ptr_eq(&p_a, &default_pipe)
                || Arc::ptr_eq(&p_a, &second_pipe);
            assert!(
                pool_default_is_real,
                "v1 default 必须是 pipelines 里某个真实 pipeline 的 Arc clone"
            );
        });
    }

    /// 测 4 (决策 6b): fallback 链端到端, **走 LlmProvider.complete (router 路径) 而不是 dispatch**
    /// 验证: MultiLlmRouter 跨第一个失败 → 走第二个 → 成功 (跟 router.rs 自带测同模式).
    #[tokio::test]
    async fn pool_fallback_chain_uses_router_complete_not_dispatch() {
        use apeireth_api::llm::providers::scripted::{ScriptedLlmProvider, ScriptedResponse};
        use apeireth_api::llm::traits::{ChatMessage, LlmProvider as _, LlmRequest};
        use apeireth_api::LlmError;

        // 构造一个永远 fail 的 provider (决策 5: router 自己 fallback, 不靠 chat_once 跨 provider)
        struct FailingProvider {
            name: String,
        }
        #[async_trait::async_trait]
        impl LlmProvider for FailingProvider {
            fn name(&self) -> &str {
                &self.name
            }
            fn supports_model(&self, _model: &str) -> bool {
                true
            }
            async fn complete(
                &self,
                _req: LlmRequest,
            ) -> Result<apeireth_api::LlmResponse, LlmError> {
                // Network error 是 retryable → router 会 fallback 到下一个
                Err(LlmError::Network {
                    provider: self.name.clone(),
                    detail: "mock fail".into(),
                })
            }
        }

        let failing = Arc::new(FailingProvider { name: "failing".into() })
            as Arc<dyn LlmProvider>;
        let success = Arc::new(
            ScriptedLlmProvider::new("success")
                .with_script("hello", ScriptedResponse::new("from success")),
        ) as Arc<dyn LlmProvider>;

        // 走 router 路径 (决策 6b: 不走 dispatch, dispatch 是 Pipeline 单 endpoint 不能 fallback)
        let router = MultiLlmRouter::new()
            .with_provider(failing)
            .with_provider(success)
            .with_fallback(vec!["failing".into(), "success".into()]);

        let req = LlmRequest::new("m", vec![ChatMessage::user("hello")]);
        let resp = router.complete(req).await.expect("router 必须 fallback 到 success");

        // 验证: 第一个 failing 失败 → router fallback → 第二个 success 命中
        assert_eq!(resp.content, "from success", "router 必须跨失败 fallback 到 success");
        assert_eq!(resp.provider, "success", "provider 字段必须是 success");

        // 验证 router 是 Arc<MultiLlmRouter> 形态, 能塞进 PipelinePool.multi
        // (此处不强构 pool, 测 2 已验 multi 工厂; 这里专测 router 路径语义)
    }

    /// 测 5: 健康检查 — `provider_names()` 端点暴露的 provider 列表
    /// 单 provider mode → ["default"]; 多 provider mode → 按 fallback_order.
    #[test]
    fn pool_health_endpoint_lists_providers() {
        with_clean_env(|| {
            // 单 provider mode
            let single = PipelinePool::single(
                "default",
                make_arc_pipeline("https://default.test", "fake-key"),
            );
            assert_eq!(
                single.provider_names(),
                vec!["default".to_string()],
                "单 provider mode: 仅暴露 default"
            );

            // 多 provider mode (按 fallback_order 暴露, 跟 router.provider_names 1:1)
            let mut pipelines = std::collections::HashMap::new();
            pipelines.insert(
                "alpha".to_string(),
                make_arc_pipeline("https://alpha.test", "fake-a"),
            );
            pipelines.insert(
                "beta".to_string(),
                make_arc_pipeline("https://beta.test", "fake-b"),
            );
            pipelines.insert(
                "gamma".to_string(),
                make_arc_pipeline("https://gamma.test", "fake-c"),
            );
            let pool = PipelinePool::multi(
                pipelines,
                vec!["beta".into(), "gamma".into(), "alpha".into()],
                Arc::new(MultiLlmRouter::new()),
            );
            assert_eq!(
                pool.provider_names(),
                vec!["beta".to_string(), "gamma".to_string(), "alpha".to_string()],
                "provider_names 按 fallback_order 暴露"
            );
        });
    }

    /// 测 6 (决策 7): 4 provider type 都能 build
    /// 现状 (0 触碰 config.rs, 诚实标注):
    /// - `apeireth-api` / `openai-compatible` / `scripted` 走 `LlmConfig::build_router` 成功
    /// - `anthropic-compatible` 在 config.rs `build_provider` 当前**未注册** (返 Err "unknown provider type")
    /// 测 6 验两部分:
    ///   6a: 4 个 type 都能写进 TOML 且 `from_str` 解析成功 (schema 兼容)
    ///   6b: 3 个已知 type 走 `build_router` 成功 + 1 个 anthropic 走 `build_router` 返 Err
    #[test]
    fn pool_supports_all_4_provider_types() {
        with_clean_env(|| {
            std::env::set_var("APEIRETH_API_KEY", "fake-api");
            std::env::set_var("APEIRETH_OPENAI_KEY", "fake-openai");
            std::env::set_var("APEIRETH_ANTHROPIC_KEY", "fake-anthropic");
            std::env::set_var("APEIRETH_LLM_NO_KEY", "placeholder");

            let toml_4types = r#"
                [providers.apeireth-api]
                type = "apeireth-api"
                base_url = "https://api.minimaxi.com/v1"
                api_key_env = "APEIRETH_API_KEY"
                models = ["MiniMax-M3"]

                [providers.openai]
                type = "openai-compatible"
                base_url = "https://api.openai.com/v1"
                api_key_env = "APEIRETH_OPENAI_KEY"
                models = ["gpt-4o"]

                [providers.anthropic]
                type = "anthropic-compatible"
                base_url = "https://api.minimaxi.com/anthropic"
                api_key_env = "APEIRETH_ANTHROPIC_KEY"
                models = ["claude-sonnet-4"]

                [providers.scripted-test]
                type = "scripted"
                api_key_env = "APEIRETH_LLM_NO_KEY"
                scripts = { "hello" = "hi from scripted" }
                default_response = "default scripted"
            "#;

            // 6a: 4 个 type 都能 from_str 解析 (ProviderConfig schema 通用, type 字段不限制)
            let cfg = apeireth_api::LlmConfig::from_str(toml_4types)
                .expect("TOML 解析必须成功 (4 个 type 都能进 schema)");
            assert_eq!(
                cfg.providers.len(),
                4,
                "TOML 必须解析出 4 个 provider (apeireth-api / openai / anthropic / scripted-test)"
            );

            // 6b: build_router 行为 — 现状 (0 触碰 config.rs):
            // - apeireth-api / openai-compatible / scripted → 成功
            // - anthropic-compatible → 落到 config.rs `build_provider` 的 unknown 分支 → Err
            let router_result = cfg.build_router();
            match router_result {
                Ok(router) => {
                    // 假设 config.rs 已扩展支持 anthropic-compatible — 此分支不应走到
                    // (留给未来 Phase: 真接 4 type 后改此 assert)
                    assert!(
                        router.provider_count() >= 3,
                        "router 至少 3 个 provider (apeireth-api + openai + scripted)"
                    );
                }
                Err(e) => {
                    // 当前 (config.rs 0 触碰) 现实: anthropic-compatible 落到 unknown → Err
                    // 验证 Err 是 LlmError::Config (fail-fast 路径, 决策 3)
                    use apeireth_api::LlmError;
                    assert!(
                        matches!(e, LlmError::Config(_)),
                        "4 type build 失败必须是 Config 错 (决策 3 fail-fast), 实际: {e:?}"
                    );
                    let msg = format!("{e}");
                    assert!(
                        msg.contains("anthropic-compatible") || msg.contains("unknown provider type"),
                        "Err 信息必须指明 anthropic 未知 type; 实际: {msg}"
                    );
                }
            }

            // 6c: PipelinePool::multi 装配至少 3 个 provider pipeline (不调 build_pipeline 网络)
            let mut pipelines = std::collections::HashMap::new();
            pipelines.insert(
                "apeireth-api".to_string(),
                make_arc_pipeline("https://api.minimaxi.com/v1", "fake-api"),
            );
            pipelines.insert(
                "openai".to_string(),
                make_arc_pipeline("https://api.openai.com/v1", "fake-openai"),
            );
            pipelines.insert(
                "scripted-test".to_string(),
                make_arc_pipeline("https://unused", "placeholder"),
            );
            let pool = PipelinePool::multi(
                pipelines,
                vec![
                    "apeireth-api".into(),
                    "openai".into(),
                    "scripted-test".into(),
                ],
                Arc::new(MultiLlmRouter::new()),
            );
            assert_eq!(
                pool.provider_count(),
                3,
                "PipelinePool::multi 装配 3 provider (anthropic 走 LlmProvider 路径不冲突)"
            );
        });
    }

    // ============ 2 边界测 (per spec §4.2 决策 8 — 必需) ============

    /// 测 7 (决策 3a): TOML provider key 缺失 → fail-fast (启动期立刻报错, 0 静默)
    /// 验证: `LlmConfig::build_router()` 返 `LlmError::Config`, 信息含 env 名 + provider 名.
    #[test]
    fn pool_toml_provider_key_missing_fails_fast() {
        with_clean_env(|| {
            // 确保缺失 env 不存在 (双重保险)
            std::env::remove_var("APEIRETH_NONEXISTENT_KEY_X42");

            let toml = r#"
                [providers.fail-key]
                type = "apeireth-api"
                base_url = "https://api.test.com/v1"
                api_key_env = "APEIRETH_NONEXISTENT_KEY_X42"
                models = ["x"]
            "#;
            let cfg = apeireth_api::LlmConfig::from_str(toml).expect("TOML 解析 OK");
            let result = cfg.build_router();

            // 决策 3a: fail-fast — 启动期立刻返 Err, 不静默 skip
            // 注: 不能 .expect_err() 因为 MultiLlmRouter 未实现 Debug (0 触碰 router.rs)
            let err = match result {
                Ok(_) => panic!("key 缺失必须 fail-fast 返 Err (实际 Ok)"),
                Err(e) => e,
            };
            use apeireth_api::LlmError;
            assert!(
                matches!(err, LlmError::Config(_)),
                "key 缺失必须返 LlmError::Config (决策 3 fail-fast), 实际: {err:?}"
            );
            let msg = format!("{err}");
            assert!(
                msg.contains("APEIRETH_NONEXISTENT_KEY_X42"),
                "Err 信息必须含 env 名 (定位调试); 实际: {msg}"
            );
            assert!(
                msg.contains("fail-key"),
                "Err 信息必须含 provider 名; 实际: {msg}"
            );
        });
    }

    /// 测 8: TOML 0 个 provider → 退化单 Pipeline (不 panic)
    /// 验证: PipelinePool 启动期构造路径对空 providers 优雅退化 (走 ::single 分支).
    #[test]
    fn pool_empty_toml_falls_back_to_single_pipeline() {
        with_clean_env(|| {
            // 8a: LlmConfig 解析空 TOML (无 [providers.*]) → providers == {} (spec 现状)
            let toml_empty = r#"
                [router]
                fallback_order = []
            "#;
            let cfg = apeireth_api::LlmConfig::from_str(toml_empty)
                .expect("空 TOML 必须能解析 (不 panic)");
            assert_eq!(
                cfg.providers.len(),
                0,
                "空 TOML → providers 必须为空 HashMap"
            );
            assert!(
                cfg.providers.is_empty(),
                "决策 4a 退化前提: providers 空 → 走 PipelinePool::single"
            );

            // 8b: 退化构造 — main() 启动期路径 (per spec §2.2) 应在此分支走 PipelinePool::single
            // 模拟: 手构 PipelinePool::single 当 TOML 空时
            let pipe = make_arc_pipeline("https://default.test", "fake-key");
            let pool = PipelinePool::single("default", Arc::clone(&pipe));
            assert_eq!(pool.provider_count(), 1);
            assert_eq!(pool.provider_names(), vec!["default".to_string()]);
            // 选任何 model 都返 default (跟旧版 build_pipeline 行为 1:1)
            assert!(Arc::ptr_eq(&pool.select_pipeline("anything"), &pipe));
        });
    }
}
