//! `apeireth-companion::intent_brier` — W6 意图理解准确率 Brier 自我诊断.
//!
//! ## 哲学 (per 主人 2026-08-18: 价值内化从玄学变有数字)
//!
//! 主人认可的不是预测机校准, 而是"对主人意图的理解准确率":
//! 每轮对话后, 模型对"主人真实意图"的预测概率 vs 事后真实意图命中
//! → Brier score (与 `oracle.rs::Forecast::resolve` 同一公式: `(p-1)² if hit else p²`).
//!
//! ## 与 oracle 的差异 (诚实登记)
//!
//! - oracle 是「外部世界事件是否会发生」(股价/部署成功/...),
//!   命中信号是 客观世界 outcome (true/false).
//! - W6 是「我是否猜对主人意图」, 命中信号是 主人反馈 (agree/correct/silent).
//! - 公式同源, 领域不同 → 同 brier_score 函数表达, 不同记录/反馈模型.
//!
//! ## 滚动窗口
//!
//! 默认 30/100/300 轮三档 (主人惯例, 短期/中期/长期校准信号).
//! 每个窗口独立计算 mean_brier + sample_count, 趋势 = 短期 vs 长期斜率.
//!
//! ## 诊断输出
//!
//! 按 `domain` (话题领域) 分组 → mean_brier → 识别低校准领域 (mean > threshold).
//! 默认阈值 0.25 (业内 Brier benchmark 中位).
//!
//! ## 衔接 (不破坏 oracle API)
//!
//! - 复用 `crate::oracle::CalibratedResolver::status()` 接口形状:
//!   `(mean_brier, resolved_count, hint)`. W6 自包含, 不依赖 SqliteMemoryStore (内存 ledger).
//! - 可选: 把 `IntentLedger` 的 records 转成 `Vec<Forecast>` 喂 `ForecastRegistry`
//!   (callers 自决定; 默认不挂, 0 LLM 路径).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

// ============================================================
// 核心数据结构
// ============================================================

/// 模型对主人意图的预测.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentPrediction {
    /// 预测话题 (e.g. "exam_prep", "companion", "invest").
    pub topic: String,
    /// 模型自信度 (0..1). f64 与 Brier 公式对齐 (避免 f32→f64 精度退化).
    pub confidence: f64,
}

impl IntentPrediction {
    pub fn new(topic: impl Into<String>, confidence: f64) -> Self {
        Self { topic: topic.into(), confidence: confidence.clamp(0.0, 1.0) }
    }
}

/// 反馈信号 (主人纠正/同意/沉默).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackOutcome {
    /// 主人同意 → 命中 (hit=true).
    Agree,
    /// 主人纠正 → 未命中 (hit=false).
    Correct,
    /// 沉默 → 保守按命中计 (hit=true; 可配置翻转).
    Silent,
}

impl FeedbackOutcome {
    /// 是否算"命中" (Brier 计算用).
    pub fn is_hit(self) -> bool {
        matches!(self, FeedbackOutcome::Agree | FeedbackOutcome::Silent)
    }
}

/// 一条意图对账记录.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentRecord {
    pub prediction: IntentPrediction,
    /// 主人真实意图 (None = 尚未反馈).
    pub true_topic: Option<String>,
    /// 反馈结果 (None = 尚未反馈).
    pub outcome: Option<FeedbackOutcome>,
    /// 毫秒时间戳.
    pub timestamp_ms: i64,
    /// 话题领域 (诊断聚合用; e.g. "study", "invest", "companion").
    pub domain: Option<String>,
}

impl IntentRecord {
    pub fn new(prediction: IntentPrediction, timestamp_ms: i64) -> Self {
        Self {
            prediction,
            true_topic: None,
            outcome: None,
            timestamp_ms,
            domain: None,
        }
    }
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// 反馈: 同意/纠正/沉默 (true_topic + outcome).
    pub fn feedback(
        mut self,
        outcome: FeedbackOutcome,
        true_topic: Option<String>,
    ) -> Self {
        self.outcome = Some(outcome);
        self.true_topic = true_topic;
        self
    }

    /// 命中与否 (仅当有 outcome 时).
    pub fn hit(&self) -> Option<bool> {
        self.outcome.map(|o| o.is_hit())
    }

    /// Brier 单条得分 (无 outcome → None).
    pub fn brier(&self) -> Option<f64> {
        self.outcome.map(|o| brier_score(self.prediction.confidence, o.is_hit()))
    }
}

// ============================================================
// IntentLedger — 滚动记录簿
// ============================================================

/// 滑动记录簿 (按插入顺序; 可设最大容量, 默认 1000).
#[derive(Debug, Clone)]
pub struct IntentLedger {
    records: VecDeque<IntentRecord>,
    max_capacity: usize,
}

impl Default for IntentLedger {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl IntentLedger {
    pub fn new(max_capacity: usize) -> Self {
        Self { records: VecDeque::with_capacity(max_capacity), max_capacity }
    }

    /// 记录一次预测.
    pub fn record(&mut self, r: IntentRecord) {
        if self.records.len() >= self.max_capacity {
            self.records.pop_front();
        }
        self.records.push_back(r);
    }

    /// 按 id (timestamp_ms 字符串化作简化 id) 找记录并追加反馈.
    /// Ponytail: 用 timestamp_ms 当 id (足够, 单进程内毫秒唯一).
    pub fn feedback(
        &mut self,
        timestamp_ms: i64,
        outcome: FeedbackOutcome,
        true_topic: Option<String>,
    ) -> Result<(), String> {
        let pos = self
            .records
            .iter()
            .position(|r| r.timestamp_ms == timestamp_ms)
            .ok_or_else(|| format!("无此记录: {timestamp_ms}"))?;
        let r = &mut self.records[pos];
        if r.outcome.is_some() {
            return Err("已反馈, 不重复".into());
        }
        r.outcome = Some(outcome);
        r.true_topic = true_topic;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// 全部记录 (按插入顺序).
    pub fn records(&self) -> Vec<IntentRecord> {
        self.records.iter().cloned().collect()
    }
    /// 仅已反馈记录.
    pub fn resolved_records(&self) -> Vec<IntentRecord> {
        self.records.iter().filter(|r| r.outcome.is_some()).cloned().collect()
    }
}

// ============================================================
// Brier 纯函数 (与 oracle.rs 同公式)
// ============================================================

/// Brier 单条得分: `(p-1)² if hit else p²`. ∈ [0, 1]; 0 = 完美, 1 = 最差.
pub fn brier_score(predicted_confidence: f64, hit: bool) -> f64 {
    let p = predicted_confidence.clamp(0.0, 1.0);
    if hit { (p - 1.0).powi(2) } else { p.powi(2) }
}

/// Brier 均值 (无样本 → 0.0).
pub fn mean_brier(records: &[IntentRecord]) -> f64 {
    let resolved: Vec<f64> = records.iter().filter_map(|r| r.brier()).collect();
    if resolved.is_empty() {
        0.0
    } else {
        resolved.iter().sum::<f64>() / resolved.len() as f64
    }
}

// ============================================================
// 滚动窗口 + 趋势
// ============================================================

/// 单个滚动窗口的统计.
#[derive(Debug, Clone, PartialEq)]
pub struct BrierWindow {
    pub window_size: usize,
    pub mean_brier: f64,
    pub sample_count: usize,
}

impl BrierWindow {
    pub fn empty(window_size: usize) -> Self {
        Self { window_size, mean_brier: 0.0, sample_count: 0 }
    }
}

/// 校准趋势 (短期窗口 vs 长期窗口).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrierTrend {
    /// 短期 Brier < 长期 → 改善中.
    Improving,
    /// 短期 ≈ 长期 (差异 < 5%).
    Stable,
    /// 短期 Brier > 长期 → 退化.
    Degrading,
}

/// 默认窗口档位 [30, 100, 300] (主人惯例).
pub const DEFAULT_WINDOWS: &[usize] = &[30, 100, 300];

/// 趋势判定阈值 (差异占比; < 5% → Stable). Ponytail: 常量, 业务可覆盖.
pub const TREND_DELTA_RATIO: f64 = 0.05;

/// 低校准领域阈值 (mean_brier 高于此 → 低校准). 业内中位 0.25.
pub const DEFAULT_LOW_CALIBRATION_THRESHOLD: f64 = 0.25;

pub fn compute_window(records: &[IntentRecord], window_size: usize) -> BrierWindow {
    if window_size == 0 {
        return BrierWindow::empty(0);
    }
    let n = records.len();
    let start = n.saturating_sub(window_size);
    let slice = &records[start..];
    let resolved: Vec<&IntentRecord> = slice.iter().filter(|r| r.outcome.is_some()).collect();
    let sample_count = resolved.len();
    if sample_count == 0 {
        BrierWindow { window_size, mean_brier: 0.0, sample_count: 0 }
    } else {
        let sum: f64 = resolved.iter().filter_map(|r| r.brier()).sum();
        BrierWindow { window_size, mean_brier: sum / sample_count as f64, sample_count }
    }
}

/// 趋势 = 短期窗口均值 vs 长期窗口均值 (含样本不足时的退化分支).
pub fn compute_trend(records: &[IntentRecord]) -> BrierTrend {
    let short = compute_window(records, 30);
    let long = compute_window(records, 300);
    match (short.sample_count, long.sample_count) {
        (0, _) | (_, 0) => BrierTrend::Stable, // 数据不足 → 不判定
        (_, _) => {
            let delta = (long.mean_brier - short.mean_brier) / long.mean_brier.max(1e-9);
            if delta > TREND_DELTA_RATIO {
                BrierTrend::Improving
            } else if delta < -TREND_DELTA_RATIO {
                BrierTrend::Degrading
            } else {
                BrierTrend::Stable
            }
        }
    }
}

// ============================================================
// 领域诊断 (识别低校准话题领域)
// ============================================================

/// 单个话题领域的诊断.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainDiagnostic {
    pub domain: String,
    pub mean_brier: f64,
    pub sample_count: usize,
    pub is_low_calibration: bool,
}

/// 按 domain 分组计算 mean_brier + 标记低校准领域.
pub fn domain_diagnostics(
    records: &[IntentRecord],
    threshold: f64,
) -> Vec<DomainDiagnostic> {
    let mut groups: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for r in records.iter().filter(|r| r.outcome.is_some()) {
        if let Some(d) = &r.domain {
            if let Some(b) = r.brier() {
                groups.entry(d.clone()).or_default().push(b);
            }
        }
    }
    let mut out: Vec<DomainDiagnostic> = groups
        .into_iter()
        .map(|(domain, scores)| {
            let sample_count = scores.len();
            let mean = scores.iter().sum::<f64>() / sample_count as f64;
            DomainDiagnostic {
                domain,
                mean_brier: mean,
                sample_count,
                is_low_calibration: mean > threshold,
            }
        })
        .collect();
    out.sort_by(|a, b| b.mean_brier.partial_cmp(&a.mean_brier).unwrap_or(std::cmp::Ordering::Equal));
    out
}

// ============================================================
// 总报告 + 渲染
// ============================================================

/// 完整诊断报告.
#[derive(Debug, Clone)]
pub struct IntentDiagnosticReport {
    /// 三档窗口 [30, 100, 300].
    pub windows: Vec<BrierWindow>,
    /// 全部已反馈记录的 mean_brier.
    pub overall_mean_brier: f64,
    /// 短期 vs 长期趋势.
    pub trend: BrierTrend,
    /// 各领域诊断 (按 mean_brier desc).
    pub domain_diagnostics: Vec<DomainDiagnostic>,
    /// 低校准领域列表 (mean > threshold).
    pub low_calibration_domains: Vec<String>,
    /// 已反馈样本数.
    pub sample_count: usize,
}

impl IntentDiagnosticReport {
    /// 空报告 (0 已反馈).
    pub fn empty() -> Self {
        Self {
            windows: DEFAULT_WINDOWS.iter().map(|&w| BrierWindow::empty(w)).collect(),
            overall_mean_brier: 0.0,
            trend: BrierTrend::Stable,
            domain_diagnostics: Vec::new(),
            low_calibration_domains: Vec::new(),
            sample_count: 0,
        }
    }
}

/// 主入口: 给定 ledger → 完整诊断报告.
pub fn compute_report(
    ledger: &IntentLedger,
    low_calibration_threshold: f64,
) -> IntentDiagnosticReport {
    let records = ledger.resolved_records();
    if records.is_empty() {
        return IntentDiagnosticReport::empty();
    }
    let windows: Vec<BrierWindow> =
        DEFAULT_WINDOWS.iter().map(|&w| compute_window(&records, w)).collect();
    let overall = mean_brier(&records);
    let trend = compute_trend(&records);
    let domain_diag = domain_diagnostics(&records, low_calibration_threshold);
    let low_calibration_domains: Vec<String> = domain_diag
        .iter()
        .filter(|d| d.is_low_calibration)
        .map(|d| d.domain.clone())
        .collect();
    IntentDiagnosticReport {
        windows,
        overall_mean_brier: overall,
        trend,
        domain_diagnostics: domain_diag,
        low_calibration_domains,
        sample_count: records.len(),
    }
}

/// 渲染诊断报告为可读文本 (供上层注入 system prompt 或日志).
pub fn render_report(report: &IntentDiagnosticReport) -> String {
    let mut s = String::from("[意图理解校准诊断]\n");
    s.push_str(&format!(
        "· 总样本 {} 条, 整体 Brier = {:.3} (0=完美, 1=全错)\n",
        report.sample_count, report.overall_mean_brier
    ));
    let trend_str = match report.trend {
        BrierTrend::Improving => "改善中 ↑",
        BrierTrend::Stable => "稳定 →",
        BrierTrend::Degrading => "退化 ↓",
    };
    s.push_str(&format!("· 趋势: {trend_str}\n"));
    for w in &report.windows {
        s.push_str(&format!(
            "· 窗口 {}: Brier = {:.3} (样本 {})\n",
            w.window_size, w.mean_brier, w.sample_count
        ));
    }
    if !report.domain_diagnostics.is_empty() {
        s.push_str("· 按领域:\n");
        for d in &report.domain_diagnostics {
            let flag = if d.is_low_calibration { " ⚠低校准" } else { "" };
            s.push_str(&format!(
                "  - {}: Brier = {:.3} (样本 {}){}\n",
                d.domain, d.mean_brier, d.sample_count, flag
            ));
        }
    }
    if !report.low_calibration_domains.is_empty() {
        s.push_str(&format!(
            "· 低校准领域需关注: {}\n",
            report.low_calibration_domains.join(", ")
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(topic: &str, conf: f64, ts: i64) -> IntentRecord {
        IntentRecord::new(IntentPrediction::new(topic, conf), ts)
    }

    fn rec_domain(topic: &str, conf: f64, ts: i64, domain: &str) -> IntentRecord {
        IntentRecord::new(IntentPrediction::new(topic, conf), ts).with_domain(domain)
    }

    // --- brier_score 纯函数 ---

    #[test]
    fn brier_perfect_prediction_zero() {
        // 命中 + 高自信 → 0
        assert!((brier_score(0.99, true) - 0.0001).abs() < 1e-4);
        // 未命中 + 低自信 → 0
        assert!((brier_score(0.01, false) - 0.0001).abs() < 1e-4);
    }

    #[test]
    fn brier_worst_prediction_one() {
        // 命中 + 0 自负 → 1
        assert!((brier_score(0.0, true) - 1.0).abs() < 1e-9);
        // 未命中 + 满自负 → 1
        assert!((brier_score(1.0, false) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn brier_oracle_consistency() {
        // 与 oracle.rs::Forecast::resolve 完全同公式: (0.7, true) → 0.09
        assert!((brier_score(0.7, true) - 0.09).abs() < 1e-9);
        assert!((brier_score(0.7, false) - 0.49).abs() < 1e-9);
    }

    #[test]
    fn brier_clamps_out_of_range() {
        // p 超出 [0,1] 应被 clamp (不 panic)
        let h = brier_score(1.5, true);
        let l = brier_score(-0.5, false);
        assert_eq!(h, 0.0); // p=1.0, hit=true → 0
        assert_eq!(l, 0.0); // p=0.0, hit=false → 0
    }

    #[test]
    fn brier_symmetry_at_half() {
        // p=0.5 时, hit 与 miss Brier 相等 = 0.25 (无信号最优)
        assert!((brier_score(0.5, true) - 0.25).abs() < 1e-9);
        assert!((brier_score(0.5, false) - 0.25).abs() < 1e-9);
    }

    // --- FeedbackOutcome ---

    #[test]
    fn feedback_hit_mapping() {
        assert!(FeedbackOutcome::Agree.is_hit());
        assert!(!FeedbackOutcome::Correct.is_hit());
        assert!(FeedbackOutcome::Silent.is_hit()); // 保守默认
    }

    // --- IntentRecord ---

    #[test]
    fn record_no_outcome_brier_none() {
        let r = rec("exam_prep", 0.8, 1);
        assert!(r.brier().is_none());
        assert!(r.hit().is_none());
    }

    #[test]
    fn record_with_agree_brier_correct() {
        let r = rec("exam_prep", 0.8, 1).feedback(FeedbackOutcome::Agree, None);
        // hit=true, p=0.8 → (0.8-1)² = 0.04
        assert!((r.brier().unwrap() - 0.04).abs() < 1e-9);
    }

    #[test]
    fn record_with_correct_brier_high() {
        let r = rec("exam_prep", 0.8, 1).feedback(FeedbackOutcome::Correct, Some("invest".into()));
        // hit=false, p=0.8 → 0.8² = 0.64
        assert!((r.brier().unwrap() - 0.64).abs() < 1e-9);
        assert_eq!(r.true_topic.as_deref(), Some("invest"));
    }

    // --- IntentLedger ---

    #[test]
    fn ledger_record_and_resolved_filter() {
        let mut l = IntentLedger::new(100);
        l.record(rec("a", 0.9, 1));
        l.record(rec("b", 0.7, 2).feedback(FeedbackOutcome::Agree, None));
        l.record(rec("c", 0.6, 3));
        assert_eq!(l.len(), 3);
        assert_eq!(l.resolved_records().len(), 1);
    }

    #[test]
    fn ledger_feedback_updates_record() {
        let mut l = IntentLedger::new(100);
        l.record(rec("a", 0.9, 100));
        l.feedback(100, FeedbackOutcome::Correct, Some("true_topic".into())).unwrap();
        let r = &l.records()[0];
        assert_eq!(r.outcome, Some(FeedbackOutcome::Correct));
        assert_eq!(r.true_topic.as_deref(), Some("true_topic"));
    }

    #[test]
    fn ledger_feedback_idempotent_rejected() {
        let mut l = IntentLedger::new(100);
        l.record(rec("a", 0.9, 100));
        l.feedback(100, FeedbackOutcome::Agree, None).unwrap();
        let r = l.feedback(100, FeedbackOutcome::Correct, None);
        assert!(r.is_err(), "重复 feedback 应拒绝: {r:?}");
    }

    #[test]
    fn ledger_feedback_missing_record_errors() {
        let mut l = IntentLedger::new(100);
        let r = l.feedback(999, FeedbackOutcome::Agree, None);
        assert!(r.is_err());
    }

    #[test]
    fn ledger_capacity_evicts_oldest() {
        let mut l = IntentLedger::new(3);
        l.record(rec("a", 0.9, 1));
        l.record(rec("b", 0.9, 2));
        l.record(rec("c", 0.9, 3));
        l.record(rec("d", 0.9, 4));
        assert_eq!(l.len(), 3, "应弹出最旧: {l:?}");
        assert_eq!(l.records()[0].prediction.topic, "b");
    }

    // --- 滚动窗口 ---

    #[test]
    fn window_smaller_than_data_takes_tail() {
        let mut l = IntentLedger::new(100);
        for i in 0..50 {
            l.record(rec("a", 0.9, i).feedback(FeedbackOutcome::Agree, None)); // hit, p=0.9 → 0.01
        }
        let w = compute_window(&l.records(), 30);
        assert_eq!(w.sample_count, 30);
        assert_eq!(w.window_size, 30);
        assert!((w.mean_brier - 0.01).abs() < 1e-9);
    }

    #[test]
    fn window_larger_than_data_uses_all() {
        let mut l = IntentLedger::new(100);
        for i in 0..5 {
            l.record(rec("a", 0.5, i).feedback(FeedbackOutcome::Agree, None)); // 0.25
        }
        let w = compute_window(&l.records(), 30);
        assert_eq!(w.sample_count, 5);
        assert!((w.mean_brier - 0.25).abs() < 1e-9);
    }

    #[test]
    fn window_zero_size_returns_empty() {
        let w = compute_window(&[], 0);
        assert_eq!(w.sample_count, 0);
    }

    #[test]
    fn window_skips_unresolved_records() {
        let mut l = IntentLedger::new(100);
        l.record(rec("a", 0.9, 1)); // 未反馈
        l.record(rec("b", 0.7, 2).feedback(FeedbackOutcome::Agree, None)); // 0.09
        let w = compute_window(&l.records(), 30);
        assert_eq!(w.sample_count, 1, "未反馈应跳过");
        assert!((w.mean_brier - 0.09).abs() < 1e-9);
    }

    // --- 趋势 ---

    #[test]
    fn trend_improving_when_short_better() {
        let mut l = IntentLedger::new(400);
        // 前 270 条: Brier 高 (0.5 自负 + miss → 0.25)
        for i in 0..270 {
            l.record(rec("a", 0.5, i).feedback(FeedbackOutcome::Correct, None));
        }
        // 后 30 条: Brier 低 (0.99 自负 + hit → ~0.0001)
        for i in 270..300 {
            l.record(rec("a", 0.99, i).feedback(FeedbackOutcome::Agree, None));
        }
        assert_eq!(compute_trend(&l.records()), BrierTrend::Improving);
    }

    #[test]
    fn trend_degrading_when_short_worse() {
        let mut l = IntentLedger::new(400);
        for i in 0..270 {
            l.record(rec("a", 0.99, i).feedback(FeedbackOutcome::Agree, None)); // 0.0001
        }
        for i in 270..300 {
            l.record(rec("a", 0.5, i).feedback(FeedbackOutcome::Correct, None)); // 0.25
        }
        assert_eq!(compute_trend(&l.records()), BrierTrend::Degrading);
    }

    #[test]
    fn trend_stable_within_threshold() {
        let mut l = IntentLedger::new(400);
        for i in 0..300 {
            l.record(rec("a", 0.99, i).feedback(FeedbackOutcome::Agree, None));
        }
        assert_eq!(compute_trend(&l.records()), BrierTrend::Stable);
    }

    #[test]
    fn trend_stable_when_insufficient_data() {
        let mut l = IntentLedger::new(100);
        l.record(rec("a", 0.9, 1).feedback(FeedbackOutcome::Agree, None));
        // 样本太少 → Stable
        assert_eq!(compute_trend(&l.records()), BrierTrend::Stable);
    }

    // --- 领域诊断 ---

    #[test]
    fn domain_diagnostics_groups_and_flags() {
        let mut l = IntentLedger::new(100);
        // study: 高自负 + hit → Brier 低 (好)
        for i in 0..10 {
            l.record(rec_domain("study", 0.9, i, "study").feedback(FeedbackOutcome::Agree, None));
        }
        // invest: 高自负 + miss → Brier 高 (差, 低校准)
        for i in 10..20 {
            l.record(rec_domain("invest", 0.9, i, "invest").feedback(FeedbackOutcome::Correct, None));
        }
        let diags = domain_diagnostics(&l.records(), DEFAULT_LOW_CALIBRATION_THRESHOLD);
        assert_eq!(diags.len(), 2);
        // 按 mean_brier desc → invest 在前
        assert_eq!(diags[0].domain, "invest");
        assert!(diags[0].is_low_calibration);
        assert!(!diags[1].is_low_calibration);
    }

    #[test]
    fn domain_diagnostics_skips_records_without_domain() {
        let mut l = IntentLedger::new(100);
        l.record(rec("a", 0.9, 1).feedback(FeedbackOutcome::Agree, None)); // 无 domain
        l.record(rec_domain("b", 0.9, 2, "study").feedback(FeedbackOutcome::Agree, None));
        let diags = domain_diagnostics(&l.records(), DEFAULT_LOW_CALIBRATION_THRESHOLD);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].domain, "study");
    }

    #[test]
    fn domain_diagnostics_threshold_configurable() {
        let mut l = IntentLedger::new(100);
        for i in 0..5 {
            l.record(rec_domain("a", 0.7, i, "test").feedback(FeedbackOutcome::Correct, None)); // 0.49
        }
        // 默认阈值 0.25 → 低校准
        assert!(domain_diagnostics(&l.records(), 0.25)[0].is_low_calibration);
        // 阈值 0.5 → 不低校准
        assert!(!domain_diagnostics(&l.records(), 0.5)[0].is_low_calibration);
    }

    // --- 报告 ---

    #[test]
    fn report_empty_ledger() {
        let l = IntentLedger::new(100);
        let r = compute_report(&l, DEFAULT_LOW_CALIBRATION_THRESHOLD);
        assert_eq!(r.sample_count, 0);
        assert_eq!(r.windows.len(), 3);
        assert_eq!(r.trend, BrierTrend::Stable);
        assert!(r.domain_diagnostics.is_empty());
    }

    #[test]
    fn report_full_pipeline() {
        let mut l = IntentLedger::new(500);
        // 早期 150 条 study Agree (Brier 0.01, 好)
        for i in 0..150 {
            l.record(
                rec_domain("study", 0.9, i, "study").feedback(FeedbackOutcome::Agree, None),
            );
        }
        // 中期 100 条 invest Correct (Brier 0.81, 差, 混入低校准)
        for i in 150..250 {
            l.record(
                rec_domain("invest", 0.9, i, "invest")
                    .feedback(FeedbackOutcome::Correct, None),
            );
        }
        // 近期 50 条 study Agree (Brier 0.01, 又好回来 → 趋势改善)
        for i in 250..300 {
            l.record(
                rec_domain("study", 0.9, i, "study").feedback(FeedbackOutcome::Agree, None),
            );
        }
        let r = compute_report(&l, DEFAULT_LOW_CALIBRATION_THRESHOLD);
        assert_eq!(r.sample_count, 300);
        assert_eq!(r.windows.len(), 3);
        // 短窗 30 全是 study good → 短窗 Brier ≈ 0.01
        // 长窗 300 含 100 条 invest bad → 长窗 Brier ≈ 0.27
        // short(0.01) < long(0.27) → 改善中 ↑
        assert_eq!(r.trend, BrierTrend::Improving, "近期校准优于长期: {r:?}");
        // 低校准领域识别: invest (Brier ≈ 0.81)
        assert!(
            r.low_calibration_domains.contains(&"invest".to_string()),
            "invest 应识别为低校准: {:?}",
            r.low_calibration_domains
        );
        assert!(!r.low_calibration_domains.contains(&"study".to_string()));
        // study 应是好校准
        let study = r
            .domain_diagnostics
            .iter()
            .find(|d| d.domain == "study")
            .unwrap();
        assert!(!study.is_low_calibration);
    }

    #[test]
    fn report_renders_with_all_sections() {
        let mut l = IntentLedger::new(500);
        for i in 0..100 {
            l.record(
                rec_domain("study", 0.9, i, "study").feedback(FeedbackOutcome::Agree, None),
            );
        }
        for i in 100..110 {
            l.record(
                rec_domain("invest", 0.9, i, "invest")
                    .feedback(FeedbackOutcome::Correct, None),
            );
        }
        let r = compute_report(&l, DEFAULT_LOW_CALIBRATION_THRESHOLD);
        let text = render_report(&r);
        assert!(text.contains("[意图理解校准诊断]"));
        assert!(text.contains("总样本 110 条"));
        assert!(text.contains("整体 Brier"));
        assert!(text.contains("窗口 30"));
        assert!(text.contains("窗口 100"));
        assert!(text.contains("窗口 300"));
        assert!(text.contains("study"));
        assert!(text.contains("invest"));
        assert!(text.contains("⚠低校准"), "低校准标记应渲染");
    }

    #[test]
    fn report_renders_empty_ledger_gracefully() {
        let l = IntentLedger::new(100);
        let r = compute_report(&l, DEFAULT_LOW_CALIBRATION_THRESHOLD);
        let text = render_report(&r);
        assert!(text.contains("[意图理解校准诊断]"));
        assert!(text.contains("总样本 0"));
    }

    // --- 复用 oracle 不破坏 ---

    #[test]
    fn oracle_forecast_brier_unchanged() {
        // W6 公式与 oracle 完全一致 → 数值级验证 oracle Brier 不受 W6 影响
        let p = 0.7;
        let oracle_hit = (p as f64 - 1.0_f64).powi(2);
        let oracle_miss = (p as f64).powi(2);
        assert!((brier_score(p, true) - oracle_hit).abs() < 1e-9);
        assert!((brier_score(p, false) - oracle_miss).abs() < 1e-9);
    }

    #[test]
    fn intent_records_convertible_to_forecast_shape() {
        // 验证 IntentRecord 的关键字段与 Forecast 同构 (允许外部挂接)
        let r = rec("a", 0.8, 1).feedback(FeedbackOutcome::Agree, None);
        let f_shape = (
            r.prediction.topic.as_str(),
            r.prediction.confidence as f64,
            r.outcome.unwrap().is_hit(),
        );
        assert_eq!(f_shape.0, "a");
        assert!((f_shape.1 - 0.8).abs() < 1e-9);
        assert!(f_shape.2);
        // 转换路径若需要: r.brier() 可直接喂 ForecastRegistry (Ponytail: 接口同构即可, 不真接)
        assert!(r.brier().is_some());
    }
}