//! `apeireth-companion::presence` — 「内心状态频道」事件定义 (前端 SSE 用).
//!
//! ## 定位
//!
//! companion_serve 的 `GET /v1/apeireth/events` 是一条 `broadcast::Sender<String>`
//! 管道 (2026-08-16 模块 4 已铺), 此前只有 "[他说] ..." 自由文本与测试事件流入。
//! 本模块定义**结构化的内心状态事件**: 单行 JSON, 含 `type` / `at` (RFC3339) / 负载字段,
//! 与 legacy 文本行共用同一条 SSE 流 (前端按 `{` 前缀或 `type` 字段区分)。
//!
//! ## 事件类型与真实来源 (0 装 PASS: 每个字段都有真实出处)
//!
//! | type            | 真实来源 |
//! |-----------------|---------|
//! | `emotion`       | consciousness `EmotionEngine` 真引擎: `snapshot()` (PAD 三维 + 主导情绪 + 漂移强度) + `response_style()`; `tone` 字段来自 `AwakeCompanion::tone()` 三层器官语调合成 |
//! | `initiative`    | `AwakeCompanion::tick` 决策留痕 (`last_decision`): 开口 = `Action::label()`; 拦下 = [`InitiativeGate`] 13 种真实门控原因 (emergence 8 门禁逐分支留痕 + organs 5 门控) |
//! | `dream`         | 做梦整合写回真库后的 `mem-dream-*` episode 增量 (条数 + 最新一条内容前 40 字摘要前缀) |
//! | `memory_recall` | `RecallMemoryTool` 真实输出 `{"query","found","top"}`: 只取 `found` 条目数与 query 关键词; 命中原文 (`top`) **不进事件**, `redacted: true` 为设计占位 |
//!
//! ## 诚实标注 (断点在哪, 未接的不假装)
//!
//! - chat_completions handler 同步路径**拿不到 PAD**: daemon (含情绪引擎) 内部 RefCell
//!   跨 await 非 Send, 锁在 daemon_loop task 里与 HTTP 交替运行。emotion 事件只能由
//!   daemon_loop 推 (每 tick 心跳 + 主人消息到达后), handler 内 0 假装。
//! - `build_injection` 记忆注入路径的召回条目数锁在 `assemble.rs::inject_memory` 内部
//!   (局部变量不外露) → 该路径的 memory_recall **未接**; 当前只接工具桥路径。
//! - presence 事件只进 SSE 广播, 不进 Lark/Telegram 离线 sink (sink 只收渲染文本)。
//!
//! ## 稳定性约定
//!
//! [`InitiativeGate`] 的 serde 标签 (`emotion_low` / `council_veto` / `quiet_hours` 等)
//! 即 SSE JSON 线上的 `gate` 值, 前端按标签消费 — **改标签 = 改前端契约**, 需同步前端。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use apeireth_consciousness::emotion::{EmotionEngine, Pad, ResponseStyle};

// ============================================================
// 门控原因枚举 (initiative 事件 held 的细分原因)
// ============================================================

/// 开口被拦下的真实门控原因 — 与代码里的门控分支一一对应 (不臆造):
///
/// - emergence.rs `EmergenceLoop::tick` 机制层 8 门禁:
///   `UserQuiet` / `QuietHours` / `DailyLimit` / `LlmBudget` / `DepthLow`
///   / `RhythmUnknown` / `RhythmVeto` / `DriveLow`
/// - organs.rs `AwakeCompanion::tick` 器官层 5 门控:
///   `SovereigntyFrozen` / `EmotionLow` / `CouncilVeto` / `PolicyInactive` / `GateBlock`
///
/// serde 标签 (snake_case) 即线上 JSON 值, 见 `as_str()` 单测锁定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitiativeGate {
    /// 主权熔断 (SovereigntyGate.is_frozen) — 最高优先, 一切停止.
    SovereigntyFrozen,
    /// 用户显式「不打扰」开关 (Boundaries.user_quiet, emergence 门禁 0).
    UserQuiet,
    /// 安静时段窗口 (Boundaries.quiet_start/end_minutes, emergence 门禁 1).
    QuietHours,
    /// 每日主动上限 (max_initiatives_per_day, emergence 门禁 2).
    DailyLimit,
    /// LLM 成本节流 (min_llm_interval, 距上次主动太近, emergence 门禁 2.5).
    LlmBudget,
    /// 关系深度不足 (depth < min_depth, emergence 门禁 3).
    DepthLow,
    /// 作息未学到 (rhythm.days == 0, 不猜测作息, emergence 门禁 4).
    RhythmUnknown,
    /// 作息否决 (此刻活跃概率 < rhythm_veto_probability, emergence 门禁 5).
    RhythmVeto,
    /// 驱动不足保持安静 (drive < drive_threshold 且冷启动探针未到期, emergence 驱动判定).
    DriveLow,
    /// 情绪低于开口门槛 (PAD 愉悦度 mood < mood_floor, organs 情绪调制).
    EmotionLow,
    /// 智囊团审议否决 (Council verdict.is_rejected, organs 审议门控).
    CouncilVeto,
    /// 主动策略非活跃态 (EvolutionStateMachine 未处于 Active, organs 演化门控).
    PolicyInactive,
    /// 洋葱门拦下 (哲学守门 BlockByPrinciple 或权限/HA 拦截, organs SecurityGate).
    GateBlock,
}

impl InitiativeGate {
    /// 稳定 serde 标签 (= JSON 线上的 `gate` 值; 单测锁定, 改即破前端契约).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SovereigntyFrozen => "sovereignty_frozen",
            Self::UserQuiet => "user_quiet",
            Self::QuietHours => "quiet_hours",
            Self::DailyLimit => "daily_limit",
            Self::LlmBudget => "llm_budget",
            Self::DepthLow => "depth_low",
            Self::RhythmUnknown => "rhythm_unknown",
            Self::RhythmVeto => "rhythm_veto",
            Self::DriveLow => "drive_low",
            Self::EmotionLow => "emotion_low",
            Self::CouncilVeto => "council_veto",
            Self::PolicyInactive => "policy_inactive",
            Self::GateBlock => "gate_block",
        }
    }

    /// 中文说明 (前端展示/日志用).
    pub const fn label(&self) -> &'static str {
        match self {
            Self::SovereigntyFrozen => "主权熔断, 一切停止",
            Self::UserQuiet => "主人开了不打扰",
            Self::QuietHours => "安静时段",
            Self::DailyLimit => "今日主动次数已达上限",
            Self::LlmBudget => "LLM 成本节流 (距上次开口太近)",
            Self::DepthLow => "关系深度还不够",
            Self::RhythmUnknown => "还没学到主人的作息",
            Self::RhythmVeto => "这个时段主人通常不活跃",
            Self::DriveLow => "还没到想开口的程度",
            Self::EmotionLow => "情绪有点低, 先不出声",
            Self::CouncilVeto => "智囊团审议否决了这次开口",
            Self::PolicyInactive => "主动策略不在生效状态",
            Self::GateBlock => "被基地的宪法/权限门拦下",
        }
    }
}

// ============================================================
// 开口决策留痕 (organs.rs AwakeCompanion::tick 每次覆写)
// ============================================================

/// `AwakeCompanion` 最近一次 tick 的开口决策 (观测口, 不参与决策).
///
/// organs.rs 在 tick 的每个出口分支真实记录: 开口 = `Spoke` (含机制选出的动作标签),
/// 拦下 = `Held` (含门控原因). daemon_loop 据此推 `initiative` 事件。
#[derive(Debug, Clone, PartialEq)]
pub enum GateDecision {
    /// 开口了. `action` = `Action::label()` (机制动作空间真实选出, 非文案).
    Spoke {
        /// 动作标签 (如 "问候"/"分享发现" — 见 actions.rs Action::label).
        action: String,
    },
    /// 被门控拦下 (含真实原因).
    Held(InitiativeGate),
}

// ============================================================
// 内心状态事件 (SSE data 行的单行 JSON)
// ============================================================

/// 内心状态事件 —  serde 内部标签 `type`, 负载字段平铺, 单行 JSON。
///
/// 线上形态示例 (emotion):
/// `{"type":"emotion","at":"2026-08-21T08:30:00Z","pad":{"p":0.0,"a":0.0,"d":0.0},...}`
///
/// 注: 不派生 PartialEq — consciousness `Pad` 未派生 PartialEq (他 crate 公共类型,
/// 不改); 测试逐字段断言.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PresenceEvent {
    /// PAD 情绪快照 (真实来源: consciousness EmotionEngine).
    Emotion(EmotionPayload),
    /// 开口决策 (真实来源: AwakeCompanion 决策留痕).
    Initiative(InitiativePayload),
    /// 做梦整合完成 (真实来源: 真库 mem-dream-* 增量).
    Dream(DreamPayload),
    /// 记忆被唤起 (真实来源: recall_memory 工具输出; 原文脱敏).
    MemoryRecall(MemoryRecallPayload),
}

/// emotion 事件负载.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionPayload {
    /// 事件时刻 (RFC3339, 推送端 Utc::now()).
    pub at: DateTime<Utc>,
    /// PAD 三维 (愉悦/唤醒/支配, 各 [-1, 1]) — `EmotionEngine::current_pad()`.
    pub pad: Pad,
    /// 当前主导情绪 (`BaseEmotion::as_str()`: joy/sadness/anger/fear/surprise/disgust).
    pub dominant: String,
    /// 漂移强度 (PAD 距 baseline 的欧氏距离, `EmotionEngine::snapshot().intensity`).
    pub intensity: f32,
    /// 情感响应风格标签 (`EmotionEngine::response_style()` → 稳定小写标签).
    pub response_style: String,
    /// 三层器官语调 (关系基线 × 情绪语气 × 审议强度, `AwakeCompanion::tone()`).
    /// 仅 daemon_loop 来源携带; 无语调来源时不出现该字段.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
}

/// initiative 事件负载.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitiativePayload {
    /// 事件时刻 (RFC3339).
    pub at: DateTime<Utc>,
    /// 结果: "spoke" (开口了) / "held" (被拦下).
    pub outcome: InitiativeOutcome,
    /// 拦下原因 (held 时必现; 13 种真实门控, 见 [`InitiativeGate`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<InitiativeGate>,
    /// 门控原因中文说明 (held 时必现, 与 gate 对应).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_label: Option<String>,
    /// 开口的动作标签 (spoke 时必现; `Action::label()`).
    /// 诚实: 完整话术已由 BroadcastSink 的 "[他说]" 文本行送达, 此处不重复.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// initiative 结果标签.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitiativeOutcome {
    /// 开口了.
    Spoke,
    /// 被门控拦下.
    Held,
}

/// dream 事件负载.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DreamPayload {
    /// 事件时刻 (RFC3339).
    pub at: DateTime<Utc>,
    /// 本次做梦整合写回真库的条数 (mem-dream-* 增量计数).
    pub merged_count: usize,
    /// 最新一条整合内容的摘要前缀 (前 40 字, 含 【做梦整合】/【做梦摘要】 原始前缀).
    pub summary_prefix: String,
}

/// memory_recall 事件负载 (设计上脱敏: 只带条目数与关键词, 不带命中原文).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecallPayload {
    /// 事件时刻 (RFC3339).
    pub at: DateTime<Utc>,
    /// 命中条目数 (recall_memory 工具输出的 `found`, 真实).
    pub found: usize,
    /// 检索关键词 (工具入参 `query` 按工具同规则切词, 截断防爆量).
    pub keywords: Vec<String>,
    /// 脱敏占位: 恒 true — 命中原文 (`top` 字段) 设计上不进事件。
    /// 前端若需原文, 应走授权的记忆面板接口, 而非 SSE 广播。
    pub redacted: bool,
}

/// ResponseStyle → 稳定小写标签 (emotion 事件的 `response_style` 字段).
/// consciousness 的 ResponseStyle 无 as_str, 此处确定性映射 (7 档全覆盖).
pub const fn response_style_tag(style: ResponseStyle) -> &'static str {
    match style {
        ResponseStyle::Warm => "warm",
        ResponseStyle::Friendly => "friendly",
        ResponseStyle::Gentle => "gentle",
        ResponseStyle::Cautious => "cautious",
        ResponseStyle::Diplomatic => "diplomatic",
        ResponseStyle::Curious => "curious",
        ResponseStyle::Professional => "professional",
    }
}

/// 检索关键词切词 — 与 `RecallMemoryTool` (tool_bridge.rs) 同规则:
/// 按空白与中文/英文标点 (，、。,.?？) 切分; 最多 8 个词, 每词最多 16 字 (防爆量).
pub fn keywords_from_query(query: &str) -> Vec<String> {
    query
        .split(|c: char| {
            c.is_whitespace() || matches!(c, '，' | ',' | '、' | '。' | '.' | '?' | '？')
        })
        .filter(|t| !t.is_empty())
        .take(8)
        .map(|t| t.chars().take(16).collect())
        .collect()
}

impl PresenceEvent {
    /// emotion 事件: 从真引擎取 PAD 快照 + 主导情绪 + 漂移强度 + 响应风格.
    /// `tone` = 三层器官语调 (AwakeCompanion::tone()), 无来源传 None.
    pub fn emotion(engine: &EmotionEngine, tone: Option<String>) -> Self {
        let snap = engine.snapshot();
        Self::Emotion(EmotionPayload {
            at: Utc::now(),
            pad: snap.pad,
            dominant: snap.dominant.as_str().to_string(),
            intensity: snap.intensity,
            response_style: response_style_tag(engine.response_style()).to_string(),
            tone,
        })
    }

    /// initiative 事件 (开口): `action` = Action::label().
    pub fn initiative_spoke(action: impl Into<String>) -> Self {
        Self::Initiative(InitiativePayload {
            at: Utc::now(),
            outcome: InitiativeOutcome::Spoke,
            gate: None,
            gate_label: None,
            action: Some(action.into()),
        })
    }

    /// initiative 事件 (被门控拦下): 含真实门控原因 + 中文说明.
    pub fn initiative_held(gate: InitiativeGate) -> Self {
        Self::Initiative(InitiativePayload {
            at: Utc::now(),
            outcome: InitiativeOutcome::Held,
            gate: Some(gate),
            gate_label: Some(gate.label().to_string()),
            action: None,
        })
    }

    /// 从决策留痕构造 initiative 事件 (daemon_loop 用).
    pub fn initiative(decision: &GateDecision) -> Self {
        match decision {
            GateDecision::Spoke { action } => Self::initiative_spoke(action.clone()),
            GateDecision::Held(gate) => Self::initiative_held(*gate),
        }
    }

    /// dream 事件: 做梦整合完成 (条数 + 摘要前缀).
    pub fn dream(merged_count: usize, summary_prefix: impl Into<String>) -> Self {
        Self::Dream(DreamPayload {
            at: Utc::now(),
            merged_count,
            summary_prefix: summary_prefix.into(),
        })
    }

    /// memory_recall 事件: 记忆被唤起 (条目数 + 关键词; redacted 恒 true).
    pub fn memory_recall(found: usize, query: &str) -> Self {
        Self::MemoryRecall(MemoryRecallPayload {
            at: Utc::now(),
            found,
            keywords: keywords_from_query(query),
            redacted: true,
        })
    }

    /// 序列化为单行 JSON (SSE data 行; serde_json::to_string 保证无换行).
    pub fn to_json_line(&self) -> String {
        // 结构全为 plain data, 序列化不会失败; 万一失败退化为显式错误 JSON (不静默丢).
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!("{{\"type\":\"presence_error\",\"error\":\"{e}\"}}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apeireth_consciousness::emotion::EmotionEvent;

    /// 固定时刻 (测试可重复): 2025-06-15T12:13:20Z.
    fn fixed_at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_750_000_000, 0).expect("合法时间戳")
    }

    // ---- ① 事件序列化格式: 平铺 type + 负载字段, 单行 JSON ----
    #[test]
    fn presence_emotion_event_serializes_flat_single_line() {
        let mut engine = EmotionEngine::new();
        engine.apply(EmotionEvent::UserPraise).expect("真引擎收事件");
        let ev = PresenceEvent::emotion(&engine, Some("礼貌克制, 谨慎而友好".to_string()));
        let line = ev.to_json_line();
        println!("presence emotion 示例: {line}"); // --nocapture 可见真实 JSON
        assert!(!line.contains('\n'), "SSE data 必须单行: {line}");
        let v: serde_json::Value = serde_json::from_str(&line).expect("合法 JSON");
        assert_eq!(v["type"], "emotion", "内部标签平铺: {line}");
        assert!(v["pad"]["p"].is_number(), "PAD 三维真实存在: {line}");
        assert!(v["pad"]["a"].is_number());
        assert!(v["pad"]["d"].is_number());
        // UserPraise → Joy (真引擎输出, 非硬编码断言具体情绪名, 只断言字段在且非空)
        assert!(v["dominant"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(v["intensity"].is_number());
        // 真引擎语义: UserPraise → Joy, intensity = |delta| = √0.21 ≈ 0.46 < 0.5 → Friendly 档
        assert_eq!(v["response_style"], "friendly");
        assert_eq!(v["tone"], "礼貌克制, 谨慎而友好");
        assert!(v["at"].is_string(), "at 字段存在: {line}");
    }

    // ---- ② 时间戳: RFC3339 且可解析回同一时刻 ----
    #[test]
    fn presence_timestamp_is_rfc3339_and_roundtrips() {
        let ev = PresenceEvent::Dream(DreamPayload {
            at: fixed_at(),
            merged_count: 2,
            summary_prefix: "【做梦整合】线代 ◆ 高数".to_string(),
        });
        let v: serde_json::Value = serde_json::from_str(&ev.to_json_line()).unwrap();
        let at_str = v["at"].as_str().expect("at 是字符串");
        let parsed = DateTime::parse_from_rfc3339(at_str)
            .unwrap_or_else(|e| panic!("at 必须是 RFC3339: {at_str} ({e})"));
        assert_eq!(
            parsed.timestamp(),
            1_750_000_000,
            "RFC3339 解析回同一时刻 (秒级)"
        );
    }

    // ---- ③ 脱敏字段: redacted 恒 true 且在 JSON 里; 原文字段不进事件 ----
    #[test]
    fn presence_memory_recall_is_redacted_with_count_and_keywords() {
        let ev = PresenceEvent::memory_recall(3, "考试 数学，线代");
        println!("presence memory_recall 示例: {}", ev.to_json_line());
        let v: serde_json::Value = serde_json::from_str(&ev.to_json_line()).unwrap();
        assert_eq!(v["type"], "memory_recall");
        assert_eq!(v["found"], 3, "条目数真实透传");
        assert_eq!(v["redacted"], true, "脱敏占位必须存在且为 true");
        assert_eq!(
            v["keywords"],
            serde_json::json!(["考试", "数学", "线代"]),
            "关键词按工具同规则切词"
        );
        assert!(v.get("top").is_none(), "命中原文不进事件 (设计脱敏)");
        assert!(v.get("content").is_none());
    }

    // ---- ④ 枚举标签稳定: 13 种门控原因 serde 标签 == as_str == 前端契约 ----
    #[test]
    fn presence_gate_tags_are_stable() {
        let cases: [(InitiativeGate, &str); 13] = [
            (InitiativeGate::SovereigntyFrozen, "sovereignty_frozen"),
            (InitiativeGate::UserQuiet, "user_quiet"),
            (InitiativeGate::QuietHours, "quiet_hours"),
            (InitiativeGate::DailyLimit, "daily_limit"),
            (InitiativeGate::LlmBudget, "llm_budget"),
            (InitiativeGate::DepthLow, "depth_low"),
            (InitiativeGate::RhythmUnknown, "rhythm_unknown"),
            (InitiativeGate::RhythmVeto, "rhythm_veto"),
            (InitiativeGate::DriveLow, "drive_low"),
            (InitiativeGate::EmotionLow, "emotion_low"),
            (InitiativeGate::CouncilVeto, "council_veto"),
            (InitiativeGate::PolicyInactive, "policy_inactive"),
            (InitiativeGate::GateBlock, "gate_block"),
        ];
        for (gate, tag) in cases {
            assert_eq!(gate.as_str(), tag, "as_str 漂移: {gate:?}");
            let v = serde_json::to_value(gate).expect("序列化");
            assert_eq!(v, serde_json::Value::String(tag.to_string()), "serde 标签漂移");
            assert!(!gate.label().is_empty(), "中文说明非空: {gate:?}");
        }
    }

    // ---- ⑤ initiative 事件: spoke/held 负载形态 (gate/action 互斥出现) ----
    #[test]
    fn presence_initiative_spoke_and_held_shapes() {
        let held = PresenceEvent::initiative_held(InitiativeGate::CouncilVeto);
        println!("presence initiative(held) 示例: {}", held.to_json_line());
        let v: serde_json::Value = serde_json::from_str(&held.to_json_line()).unwrap();
        assert_eq!(v["type"], "initiative");
        assert_eq!(v["outcome"], "held");
        assert_eq!(v["gate"], "council_veto");
        assert_eq!(v["gate_label"], "智囊团审议否决了这次开口");
        assert!(v.get("action").is_none(), "held 不带 action");

        let spoke = PresenceEvent::initiative_spoke("问候");
        let v: serde_json::Value = serde_json::from_str(&spoke.to_json_line()).unwrap();
        assert_eq!(v["type"], "initiative");
        assert_eq!(v["outcome"], "spoke");
        assert_eq!(v["action"], "问候");
        assert!(v.get("gate").is_none(), "spoke 不带 gate");

        // 决策留痕 → 事件 (daemon_loop 真实走法)
        let d = GateDecision::Spoke {
            action: "分享发现".to_string(),
        };
        let ev = PresenceEvent::initiative(&d);
        let v: serde_json::Value = serde_json::from_str(&ev.to_json_line()).unwrap();
        assert_eq!(v["outcome"], "spoke");
        assert_eq!(v["action"], "分享发现");
    }

    // ---- ⑥ dream 事件形态: 条数 + 摘要前缀 ----
    #[test]
    fn presence_dream_event_shape_and_roundtrip() {
        let ev_at = fixed_at();
        let ev = PresenceEvent::Dream(DreamPayload {
            at: ev_at,
            merged_count: 2,
            summary_prefix: "【做梦摘要】主人在准备考试".to_string(),
        });
        let line = ev.to_json_line();
        println!("presence dream 示例: {line}");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["type"], "dream");
        assert_eq!(v["merged_count"], 2);
        assert_eq!(
            v["summary_prefix"], "【做梦摘要】主人在准备考试",
            "摘要前缀真实透传"
        );
        // 反序列化回环 (前端/测试消费同构; PresenceEvent 无 PartialEq — Pad 未派生,
        // 逐字段断言)
        let back: PresenceEvent = serde_json::from_str(&line).expect("可反序列化");
        let PresenceEvent::Dream(p) = back else {
            panic!("应反序列化回 dream 变体: {line}")
        };
        assert_eq!(p.merged_count, 2);
        assert_eq!(p.summary_prefix, "【做梦摘要】主人在准备考试");
        assert_eq!(p.at, ev_at, "at 回环一致");
    }

    // ---- ⑦ 关键词切词边界: 空 query / 超长截断 ----
    #[test]
    fn presence_keywords_split_bounds() {
        assert!(keywords_from_query("").is_empty(), "空 query 无关键词");
        assert!(keywords_from_query(" ，。 ").is_empty(), "纯标点无关键词");
        let long = "这是一个非常非常非常长的关键词超过十六个字的部分应该被截断掉";
        let kws = keywords_from_query(long);
        assert_eq!(kws.len(), 1);
        assert_eq!(kws[0].chars().count(), 16, "每词截断 16 字");
        let many = (0..12).map(|i| format!("词{i}")).collect::<Vec<_>>().join(" ");
        assert_eq!(keywords_from_query(&many).len(), 8, "最多 8 个关键词");
    }
}
