//! `apeireth-companion::emergence` — 涌现循环 (机制, 不是行为).
//!
//! 只焊原语, 不写死行为 (per 主人 2026-08-15 拍板):
//!
//! - **节律学习**: 直方图估计「此刻你活跃的概率」—— 支持多峰作息 (早/晚)、周末偏移,
//!   不硬编码任何时刻, 不假设单峰.
//! - **驱动**: 关系压力随「沉默时长 × 关系温暖度」增长 (Borbély 睡眠压力式内稳态).
//! - **门禁 (宪法)**: 不打扰开关 + 安静窗口 + 主动频率上限 + 最小关系深度.
//! - **决策**: 只回答「现在该不该找你 / 为什么 / 我有多确定」, 不产生具体话语.
//! - **反馈**: 回了 → 关系加深; 没回 → 关系变淡 (负性偏误: 惩罚 > 奖励).
//!
//! **科学诚实**:
//! - 所有可调常数集中在 `LoopConfig`, 每个字段标注依据与「待拟合」状态.
//! - 当前参数是「合理先验」, 不是「数据结论」; 拟合只能等真实交互数据 (见
//!   docs/stage1/product-loop-rationale.md 的待验证清单).
//!
//! **关键**: `Initiative` 不含任何固定文案。真正的话由 LLM 在送达时, 面对这份
//! Initiative 自然生成。所以「早安」不在代码里 —— 它是循环转起来之后长出来的。

use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use crate::actions::{select_action, Action};
use crate::presence::InitiativeGate;
use crate::Bond;

// ============================================================
// 关系状态 (trait 缝)
// ============================================================

/// 关系状态提供者。生产实现由 `crate::Bond` 桥接.
pub trait RelationshipState {
    /// 0..1 的关系深度 (门槛用)
    fn depth(&self) -> f64;
    /// 0..1 的关系温暖度 (驱动用): 默认 = 深度; Bond 混入信任与共鸣.
    fn warmth(&self) -> f64 {
        self.depth()
    }
    /// 反馈后微调深度 (delta 可正可负, 内部 clamp 到 0..1)
    fn adjust(&mut self, delta: f64);
}

/// 机制层本地近似 (诚实: 不是真 `Bond`, 是「最丑能转」的最小实现)
#[derive(Debug, Clone)]
pub struct LocalRelationship {
    depth: f64,
}

impl LocalRelationship {
    pub fn new(depth: f64) -> Self {
        Self {
            depth: depth.clamp(0.0, 1.0),
        }
    }
}

impl RelationshipState for LocalRelationship {
    fn depth(&self) -> f64 {
        self.depth
    }
    fn adjust(&mut self, delta: f64) {
        self.depth = (self.depth + delta).clamp(0.0, 1.0);
    }
}

/// 桥接真实的 `Bond` (零新依赖): 深度读自 `Bond::depth`,
/// 温暖度 = 深度 × (0.5 + 0.5 × (信任+共鸣)/2) —— 信任/共鸣低时, 关系虽深也克制.
impl RelationshipState for Bond {
    fn depth(&self) -> f64 {
        self.depth().value()
    }
    fn warmth(&self) -> f64 {
        let d = self.depth().value();
        let c = self.character();
        let trust_resonance = (c.trust + c.resonance) / 2.0;
        d * (0.5 + 0.5 * trust_resonance)
    }
    fn adjust(&mut self, delta: f64) {
        let stage = self.stage();
        self.evolve(stage, delta); // evolve 内部 clamp 到 0..1
    }
}

// ============================================================
// 参数集中地 (所有「拍脑袋」常数, 附依据 + 待拟合标注)
// ============================================================

/// 机制可调参数. 每个字段: 依据 + 当前值 = 合理先验, 非数据结论 (待拟合).
#[derive(Debug, Clone)]
pub struct LoopConfig {
    /// 驱动 = warmth × depth_weight + 沉默压力 × silence_weight (默认 0.5/0.5, 待拟合)
    pub depth_weight: f64,
    pub silence_weight: f64,
    /// 沉默多久压力饱和 (小时): 72h → 压力 1.0 (类比 Borbély 睡眠压力, 待拟合)
    pub silence_saturation_hours: f64,
    /// 活跃时段加成 (到点更愿意开口, 默认 +0.25; 经合成用户校准: 0.2 对日常双峰用户过于保守)
    pub rhythm_boost: f64,
    /// 驱动阈值: drive >= 阈值才开口 (默认 0.45; 经合成用户校准: 0.5 对日常双峰用户过于保守)
    pub drive_threshold: f64,
    /// 冷启动探针 (RL 探索): 在活跃时段且距上次主动 >= 此小时数, 即使 drive 未达阈值也试一次
    /// (破「太安静→无反馈→温暖度不涨」死锁; 默认 24h, 待拟合)
    pub probe_hours: f64,
    /// 情绪愉悦度下限: mood < 此值不出声 (默认 0.3, 待拟合)
    pub mood_floor: f64,
    /// 回应 → 关系增量 (默认 +0.05, 待拟合)
    pub respond_delta: f64,
    /// 忽略 → 关系减量 (默认 -0.10; 负性偏误: 惩罚 > 奖励, Baumeister「坏比好强」)
    pub ignored_delta: f64,
    /// 节律直方图桶宽 (分钟, 默认 30)
    pub rhythm_bucket_minutes: u32,
    /// 活跃概率阈值: 该时段活跃概率 >= 此值视为「活跃时段」 (默认 0.5, 待拟合)
    pub rhythm_active_probability: f64,
    /// 节奏否决阈值: 学到的作息说「此刻活跃概率 < 此值」→ 沉默压力再大也不打扰
    /// (默认 0.2; 学来的, 不是写死的 — 夜猫子会被学成夜间活跃, 否决自然失效)
    pub rhythm_veto_probability: f64,
    /// 置信度饱和天数: days/14 → 置信度 (默认 14, 待拟合)
    pub rhythm_confidence_days: f64,
    /// 两次主动之间的最短间隔 (LLM 成本预算): 生产渲染走真 LLM, 连续开口会触发
    /// MiniMax 限流 (suppressed, 2026-08-15 生产力实测) — 机制层保证两次主动
    /// >= 此间隔, 给 LLM 留恢复时间 (默认 60s, 待拟合)
    pub min_llm_interval: Duration,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            depth_weight: 0.5,
            silence_weight: 0.5,
            silence_saturation_hours: 72.0,
            rhythm_boost: 0.25,
            drive_threshold: 0.45,
            probe_hours: 24.0,
            mood_floor: 0.3,
            respond_delta: 0.05,
            ignored_delta: -0.10,
            rhythm_bucket_minutes: 30,
            rhythm_active_probability: 0.5,
            rhythm_veto_probability: 0.2,
            rhythm_confidence_days: 14.0,
            min_llm_interval: Duration::from_secs(60),
        }
    }
}

// ============================================================
// 节律 (作息估计) — 直方图, 诚实, 不硬编码时刻
// ============================================================

#[derive(Debug, Clone, Copy)]
pub struct RhythmEstimate {
    /// 此刻 (传入的 minutes_now) 你活跃的概率: 该时间桶命中数 / 观察天数
    pub active_probability: f64,
    /// 观察天数 (不同日期数)
    pub days: usize,
    /// 0..1 置信度, 观察天数越多越高
    pub confidence: f64,
}

impl RhythmEstimate {
    /// 诚实可解释: 明确说出「是猜的 + 概率 + 置信度 + 依据」, 不假装精确。
    pub fn explain(&self) -> String {
        if self.days == 0 {
            return "我还没观察到你的作息, 所以现在不会主动打扰你".to_string();
        }
        format!(
            "根据 {} 天观察, 我猜这个时段你活跃的概率约 {:.0}% (置信度 {:.0}%)",
            self.days,
            self.active_probability * 100.0,
            self.confidence * 100.0
        )
    }
}

/// 直方图节律估计器: 每个时间桶记录「多少天里这个时段你有交互」.
/// 支持多峰作息 (早/晚) 与周末偏移; 不假设单峰, 不硬编码时刻.
#[derive(Debug)]
pub struct RhythmEstimator {
    /// (day_key "YYYY-MM-DD", minutes_of_day)
    observations: VecDeque<(String, u32)>,
    /// 保留最近 N 个自然日 (默认 28), 按天淘汰
    capacity_days: usize,
    bucket_minutes: u32,
}

impl RhythmEstimator {
    /// capacity_days: 保留最近 N 个自然日的观察 (默认 28); bucket_minutes: 直方图桶宽.
    /// 按「天」淘汰, 不按「条数」——每天观察次数从几次到上百次都要正确.
    pub fn new(capacity_days: usize, bucket_minutes: u32) -> Self {
        Self {
            observations: VecDeque::new(),
            capacity_days: capacity_days.max(1),
            bucket_minutes: bucket_minutes.max(5),
        }
    }

    pub fn observe(&mut self, at: DateTime<Utc>) {
        let day = at.format("%Y-%m-%d").to_string();
        let mins = at.hour() * 60 + at.minute();
        self.observations.push_back((day, mins));
        // 按天淘汰: 不同天数超过 capacity_days 时, 弹出最老那天的全部观察
        let days: HashSet<&str> = self.observations.iter().map(|(d, _)| d.as_str()).collect();
        if days.len() > self.capacity_days {
            let oldest = self.observations.front().unwrap().0.clone();
            while self
                .observations
                .front()
                .map(|(d, _)| *d == oldest)
                .unwrap_or(false)
            {
                self.observations.pop_front();
            }
        }
    }

    /// 估计「此刻 (minutes_now) 你活跃的概率」.
    pub fn estimate(&self, minutes_now: u32) -> RhythmEstimate {
        let days: HashSet<&str> = self.observations.iter().map(|(d, _)| d.as_str()).collect();
        let n_days = days.len();
        if n_days == 0 {
            return RhythmEstimate {
                active_probability: 0.0,
                days: 0,
                confidence: 0.0,
            };
        }
        let bucket = minutes_now / self.bucket_minutes;
        let hits = self
            .observations
            .iter()
            .filter(|(_, m)| m / self.bucket_minutes == bucket)
            .count();
        let active_probability = (hits as f64 / n_days as f64).clamp(0.0, 1.0);
        let confidence = (n_days as f64 / 14.0).clamp(0.0, 1.0);
        RhythmEstimate {
            active_probability,
            days: n_days,
            confidence,
        }
    }
}

// ============================================================
// 门禁 (宪法)
// ============================================================

#[derive(Debug, Clone)]
pub struct Boundaries {
    pub quiet_start_minutes: Option<u32>,
    pub quiet_end_minutes: Option<u32>,
    /// 用户显式「不打扰」开关 (真门禁, 由用户控制).
    pub user_quiet: bool,
    pub max_initiatives_per_day: u32,
    pub min_depth: f64,
}

impl Default for Boundaries {
    fn default() -> Self {
        Self {
            quiet_start_minutes: None,
            quiet_end_minutes: None,
            user_quiet: false,
            max_initiatives_per_day: 2,
            min_depth: 0.3,
        }
    }
}

impl Boundaries {
    fn in_quiet_window(&self, minutes: u32) -> bool {
        match (self.quiet_start_minutes, self.quiet_end_minutes) {
            (Some(s), Some(e)) if s <= e => minutes >= s && minutes < e,
            (Some(s), Some(e)) => minutes >= s || minutes < e, // 跨午夜
            _ => false,
        }
    }
}

// ============================================================
// 决策结果
// ============================================================

#[derive(Debug, Clone)]
pub struct Initiative {
    pub reason: InitiativeReason,
    /// 这次主动选择的动作 (从动作空间里选, 不只「问候」)
    pub action: Action,
    pub rhythm: RhythmEstimate,
    pub depth: f64,
    /// 从记忆里捞的、关于用户的东西 (非固定文案, 由上层记忆检索注入)
    pub context_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InitiativeReason {
    /// 到了用户通常活跃的时段
    RhythmMatched { minutes_now: u32 },
    /// 沉默太久 (关系压力)
    LongSilence { since_hours: f64 },
}

impl Initiative {
    /// 把 Initiative 渲染成一条诚实可读的消息正文.
    /// 不含任何固定问候文案——只陈述「为什么现在 + 我猜的作息 + 我记得什么」.
    pub fn to_message(&self) -> String {
        let why = match &self.reason {
            InitiativeReason::RhythmMatched { minutes_now } => {
                let (h, m) = (minutes_now / 60, minutes_now % 60);
                format!("现在 {}:{:02}, 到了你通常活跃的时段", h, m)
            }
            InitiativeReason::LongSilence { since_hours } => {
                format!("已经 {:.1} 小时没联系了", since_hours)
            }
        };
        let mut msg = format!(
            "[动作: {}] {}. {}",
            self.action.label(),
            why,
            self.rhythm.explain()
        );
        if let Some(h) = &self.context_hint {
            msg.push_str(&format!(" 我记得: {}", h));
        }
        msg
    }
}

// ============================================================
// 反馈
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feedback {
    Responded,
    Ignored,
}

/// 自评: 基于反馈给自己打的分 + 关系深度的变化量
#[derive(Debug, Clone, Copy)]
pub struct SelfScore {
    pub value: f64,
    pub depth_delta: f64,
}

// ============================================================
// 送达通道 (trait 缝)
// ============================================================

#[async_trait]
pub trait Delivery: Send + Sync {
    async fn deliver(&self, initiative: &Initiative) -> Result<(), String>;
}

/// 默认 no-op (诚实: 真送达是 lark/通知/语音 的接入点, 待接)
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopDelivery;

#[async_trait]
impl Delivery for NoopDelivery {
    async fn deliver(&self, _initiative: &Initiative) -> Result<(), String> {
        Ok(())
    }
}

/// 控制台送达 (真实现: 把 Initiative 渲染成诚实可读的消息打印出来).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConsoleDelivery;

#[async_trait]
impl Delivery for ConsoleDelivery {
    async fn deliver(&self, i: &Initiative) -> Result<(), String> {
        println!("[主动] {}", i.to_message());
        println!("       关系深度 {:.2}", i.depth);
        Ok(())
    }
}

// ============================================================
// 涌现循环
// ============================================================

pub struct EmergenceLoop<R: RelationshipState> {
    pub relationship: R,
    pub rhythm: RhythmEstimator,
    pub boundaries: Boundaries,
    pub config: LoopConfig,
    last_contact: Option<DateTime<Utc>>,
    last_initiative: Option<DateTime<Utc>>,
    initiatives_today: u32,
    day_key: String,
    /// 本地轨迹 (诚实: 待桥接到 Timeline/memory)
    pub history: VecDeque<HistoryEntry>,
    /// 最近一次 tick「保持安静」的门控原因留痕 (presence 内心状态频道观测口;
    /// 决定开口时清零). 纯记录, 不参与决策 — 2026-08-21 增量添加.
    last_hold: Option<InitiativeGate>,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub at: DateTime<Utc>,
    pub feedback: Feedback,
    pub score: f64,
}

impl<R: RelationshipState> EmergenceLoop<R> {
    pub fn new(relationship: R, boundaries: Boundaries) -> Self {
        let config = LoopConfig::default();
        Self {
            relationship,
            rhythm: RhythmEstimator::new(28, config.rhythm_bucket_minutes),
            boundaries,
            config,
            last_contact: None,
            last_initiative: None,
            initiatives_today: 0,
            day_key: String::new(),
            history: VecDeque::with_capacity(64),
            last_hold: None,
        }
    }

    /// 换参数 (实验调参入口). 必须在开始观察前调用 (会重建节律估计器).
    pub fn with_config(mut self, config: LoopConfig) -> Self {
        let bucket = config.rhythm_bucket_minutes;
        self.config = config;
        self.rhythm = RhythmEstimator::new(28, bucket);
        self
    }

    /// 当前关系深度 (0..1).
    pub fn depth(&self) -> f64 {
        self.relationship.depth()
    }

    /// 最近一次 tick 被机制层门控拦下的原因 (开口成功 = None).
    /// presence 内心状态频道观测口 (2026-08-21): 只读, 0 副作用.
    pub fn last_hold(&self) -> Option<InitiativeGate> {
        self.last_hold
    }

    /// 观察一次用户交互 (无论主动/被动), 喂给节律学习 + 刷新最后接触时间.
    pub fn observe_interaction(&mut self, at: DateTime<Utc>) {
        self.rhythm.observe(at);
        self.last_contact = Some(at);
    }

    /// 每次心跳调用一次. 返回 `Some(Initiative)` = 决定主动找你; `None` = 保持安静.
    pub fn tick(&mut self, now: DateTime<Utc>, context_hint: Option<String>) -> Option<Initiative> {
        let minutes_now = now.hour() * 60 + now.minute();

        // 按天重置计数
        let key = now.format("%Y-%j").to_string();
        if key != self.day_key {
            self.day_key = key;
            self.initiatives_today = 0;
        }

        // 门禁 0: 用户显式「不打扰」
        if self.boundaries.user_quiet {
            self.last_hold = Some(InitiativeGate::UserQuiet); // presence 留痕
            return None;
        }
        // 门禁 1: 安静窗口
        if self.boundaries.in_quiet_window(minutes_now) {
            self.last_hold = Some(InitiativeGate::QuietHours); // presence 留痕
            return None;
        }
        // 门禁 2: 频率上限
        if self.initiatives_today >= self.boundaries.max_initiatives_per_day {
            self.last_hold = Some(InitiativeGate::DailyLimit); // presence 留痕
            return None;
        }
        // 门禁 2.5 (LLM 成本预算): 距上次主动不足 min_llm_interval → 保持安静.
        // 生产渲染走真 LLM, 连续开口会触发 MiniMax 限流 (suppressed, 2026-08-15 实测).
        if let Some(last) = self.last_initiative {
            let since = now.signed_duration_since(last);
            let min = chrono::Duration::from_std(self.config.min_llm_interval)
                .unwrap_or(chrono::Duration::zero());
            if since < min {
                self.last_hold = Some(InitiativeGate::LlmBudget); // presence 留痕
                return None;
            }
        }
        // 门禁 3: 关系深度不够
        let depth = self.relationship.depth();
        if depth < self.boundaries.min_depth {
            self.last_hold = Some(InitiativeGate::DepthLow); // presence 留痕
            return None;
        }

        let rhythm = self.rhythm.estimate(minutes_now);
        // 门禁 4: 没有观察天数时, 不猜测作息 (诚实: 不打扰)
        if rhythm.days == 0 {
            self.last_hold = Some(InitiativeGate::RhythmUnknown); // presence 留痕
            return None;
        }
        // 门禁 5 (节奏否决): 学到的作息说「此刻几乎不可能活跃」→ 沉默压力再大也不打扰
        // (这是「推算作息」能力的第一个实例: 深夜/午觉被学成安静, 不是写死的)
        if rhythm.active_probability < self.config.rhythm_veto_probability {
            self.last_hold = Some(InitiativeGate::RhythmVeto); // presence 留痕
            return None;
        }

        // 驱动 = 温暖度 × 权重 + 沉默压力 × 权重 (机制里那粒种子)
        let silence_hours = self
            .last_contact
            .map(|lc| (now - lc).num_minutes() as f64 / 60.0)
            .unwrap_or(f64::INFINITY);
        let silence_pressure =
            (silence_hours / self.config.silence_saturation_hours).clamp(0.0, 1.0);

        let warmth = self.relationship.warmth();
        let in_rhythm = rhythm.active_probability >= self.config.rhythm_active_probability;
        let mut drive =
            warmth * self.config.depth_weight + silence_pressure * self.config.silence_weight;
        if in_rhythm {
            drive += self.config.rhythm_boost;
        }

        let hours_since_initiative = self
            .last_initiative
            .map(|t| (now - t).num_minutes() as f64 / 60.0);

        let reason = if drive >= self.config.drive_threshold {
            if in_rhythm {
                InitiativeReason::RhythmMatched { minutes_now }
            } else {
                InitiativeReason::LongSilence {
                    since_hours: silence_hours,
                }
            }
        } else {
            // 冷启动探针 (RL 探索): 活跃时段且距上次主动 >= probe_hours → 试一次
            let probe = in_rhythm
                && hours_since_initiative
                    .map(|h| h >= self.config.probe_hours)
                    .unwrap_or(true);
            if !probe {
                self.last_hold = Some(InitiativeGate::DriveLow); // presence 留痕
                return None;
            }
            InitiativeReason::LongSilence {
                since_hours: hours_since_initiative.unwrap_or(silence_hours),
            }
        };

        self.initiatives_today += 1;
        self.last_initiative = Some(now);
        self.last_hold = None; // presence 留痕: 决定开口, 清除拦下原因
        let action = select_action(context_hint.as_deref());
        Some(Initiative {
            reason,
            action,
            rhythm,
            depth,
            context_hint,
        })
    }

    /// 用户对上次主动的反馈 → 更新关系深度 + 自评 + 记录轨迹.
    pub fn apply_feedback(&mut self, feedback: Feedback, at: DateTime<Utc>) -> SelfScore {
        let (depth_delta, value) = match feedback {
            Feedback::Responded => (self.config.respond_delta, 0.9),
            Feedback::Ignored => (self.config.ignored_delta, 0.2),
        };
        self.relationship.adjust(depth_delta);
        self.history.push_back(HistoryEntry {
            at,
            feedback,
            score: value,
        });
        if self.history.len() > 64 {
            self.history.pop_front();
        }
        SelfScore { value, depth_delta }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, h, m, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn rhythm_histogram_learns_probability() {
        let mut e = RhythmEstimator::new(28, 30);
        for day in 9..=15u32 {
            e.observe(at(day, 8, 35 + (day % 3)));
        }
        // 8:40 所在桶 (8:30-9:00) 命中 7 天 → 概率 1.0, 置信度 7/14
        let est = e.estimate(8 * 60 + 40);
        assert_eq!(est.days, 7);
        assert!((est.active_probability - 1.0).abs() < 1e-6);
        assert!((est.confidence - 0.5).abs() < 1e-6);
        // 深夜桶 (2:00) 命中 0 → 概率 0
        let night = e.estimate(2 * 60);
        assert_eq!(night.active_probability, 0.0);
        let s = est.explain();
        assert!(s.contains("猜") && s.contains("置信度") && s.contains("概率"));
    }

    #[test]
    fn zero_observations_means_no_initiative() {
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.6), Boundaries::default());
        assert!(l.tick(at(15, 8, 40), None).is_none());
    }

    #[test]
    fn quiet_window_blocks_initiative() {
        let b = Boundaries {
            quiet_start_minutes: Some(0),
            quiet_end_minutes: Some(6 * 60),
            ..Default::default()
        };
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.6), b);
        l.observe_interaction(at(15, 8, 40));
        assert!(l.tick(at(15, 2, 0), None).is_none());
    }

    #[test]
    fn shallow_bond_does_not_initiate() {
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.1), Boundaries::default());
        l.observe_interaction(at(15, 8, 40));
        assert!(l.tick(at(15, 8, 40), None).is_none());
    }

    #[test]
    fn deep_bond_in_rhythm_window_initiates() {
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.8), Boundaries::default());
        for d in 9..=15 {
            l.observe_interaction(at(d, 8, 40));
        }
        let hint = Some("昨天修的 council bug".to_string());
        let init = l.tick(at(16, 8, 40), hint);
        assert!(init.is_some());
        let init = init.unwrap();
        assert!(matches!(
            init.reason,
            InitiativeReason::RhythmMatched { .. }
        ));
        assert!(init.context_hint.is_some());
        // 决策层不产生任何固定问候文案
        assert!(!init.context_hint.as_ref().unwrap().contains("早上好"));
    }

    #[test]
    fn feedback_shapes_relationship() {
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.6), Boundaries::default());
        for d in 9..=15 {
            l.observe_interaction(at(d, 8, 40));
        }
        let _ = l.tick(at(16, 8, 40), None);
        let d0 = l.relationship.depth();
        let s1 = l.apply_feedback(Feedback::Responded, at(16, 8, 45));
        let d1 = l.relationship.depth();
        assert!(d1 > d0);
        assert!(s1.value > 0.8);
        let s2 = l.apply_feedback(Feedback::Ignored, at(16, 8, 50));
        let d2 = l.relationship.depth();
        assert!(d2 < d1);
        assert!(s2.value < 0.5);
    }

    #[test]
    fn frequency_limit_holds() {
        let b = Boundaries {
            max_initiatives_per_day: 1,
            ..Default::default()
        };
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.8), b);
        for d in 9..=15 {
            l.observe_interaction(at(d, 8, 40));
        }
        assert!(l.tick(at(16, 8, 40), None).is_some());
        assert!(l.tick(at(16, 8, 41), None).is_none());
    }

    #[test]
    fn min_llm_interval_blocks_back_to_back_initiatives() {
        let mut config = LoopConfig::default();
        config.min_llm_interval = Duration::from_secs(3600); // 1h: 演示 1 分钟内的第二次主动必被拦
        let mut l = EmergenceLoop::new(LocalRelationship::new(0.8), Boundaries::default())
            .with_config(config);
        // 学会两个活跃时段: 早 8:40 + 下午 16:10 (同一 16:00-16:30 桶)
        for d in 9..=15 {
            l.observe_interaction(at(d, 8, 40));
            l.observe_interaction(at(d, 16, 10));
        }
        assert!(l.tick(at(16, 8, 40), None).is_some());
        // 1 分钟后仍在 min_llm_interval 内 → 保持安静 (LLM 成本预算门禁)
        assert!(l.tick(at(16, 8, 41), None).is_none());
        // 超过间隔且仍在活跃时段 (16:00-16:30 桶) → 恢复主动
        assert!(l.tick(at(16, 16, 10), None).is_some());
    }

    #[test]
    fn bond_warmth_uses_trust_and_resonance() {
        let mut bond = crate::Bond::new();
        bond.evolve(crate::BondStage::Trusted, 0.5);
        // 初始 character 全 0 → warmth = 0.5 × 0.5 = 0.25
        assert!((RelationshipState::warmth(&bond) - 0.25).abs() < 1e-6);
        // 注入情绪 (信任/共鸣升) → warmth 升
        bond.apply_emotion(1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(RelationshipState::warmth(&bond) > 0.25);
    }

    #[test]
    fn bond_bridge_uses_real_bond() {
        let mut bond = crate::Bond::new();
        bond.evolve(crate::BondStage::Trusted, 0.5);
        let mut l = EmergenceLoop::new(bond, Boundaries::default());
        assert!((l.depth() - 0.5).abs() < 1e-6);
        l.apply_feedback(Feedback::Responded, at(15, 9, 0));
        assert!(l.depth() > 0.5);
    }
}
