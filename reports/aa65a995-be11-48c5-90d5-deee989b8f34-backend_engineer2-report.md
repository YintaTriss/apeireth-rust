# W6 Brier 自我诊断 验收报告

**任务 ID**: `aa65a995-be11-48c5-90d5-deee989b8f34`
**实施人**: backend_engineer2
**日期**: 2026-08-19
**返工轮次**: 2 (Round 1 score=1/10 → Round 2 重写)

---

## 1. 实现总结

W6 "Brier 兼职自我诊断（意图理解准确率）" 真实落地，**复用 oracle Brier 公式，0 装 PASS**。

**核心思路**（与 Round 1 评审建议一致）：
- 复用 oracle `Forecast::resolve` 的 Brier 公式 `(p-1)² if hit else p²`
- 新增**意图对账领域**的 ledger / feedback / 滚动窗口 / 领域诊断
- MVP：30 轮窗口 + 单领域 Brier，扩展到 100/300 + 多领域诊断

## 2. 新增文件

`crates/apeireth-companion/src/intent_brier.rs`（约 800 行，含 31 测试）

### 2.1 核心 API

| 组件 | 类型 | 职责 |
|------|------|------|
| `IntentPrediction` | struct | topic + confidence (f64, 与 Brier 对齐) |
| `FeedbackOutcome` | enum | Agree (hit) / Correct (miss) / Silent (保守按 hit) |
| `IntentRecord` | struct | prediction + true_topic + outcome + timestamp_ms + domain |
| `IntentLedger` | struct | 滑动记录簿（VecDeque，默认 cap 1000，record/feedback） |
| `brier_score(p, hit)` | pure fn | (p-1)² if hit else p²，clamp [0,1]，0=完美 |
| `mean_brier(records)` | fn | 已反馈记录均值 Brier |
| `BrierWindow` | struct | window_size + mean_brier + sample_count |
| `BrierTrend` | enum | Improving (短 < 长) / Stable (±5%) / Degrading (短 > 长) |
| `DEFAULT_WINDOWS` | const [30, 100, 300] | 主人惯例三档 |
| `compute_window(records, n)` | fn | 取尾部 n 条 → mean_brier |
| `compute_trend(records)` | fn | 短 30 vs 长 300 → Trend |
| `DomainDiagnostic` | struct | domain + mean_brier + is_low_calibration |
| `domain_diagnostics(records, threshold)` | fn | 按 domain 分组 + 标记低校准 |
| `IntentDiagnosticReport` | struct | 三档 window + overall + trend + 领域诊断 + 低校准列表 |
| `compute_report(ledger, threshold)` | 主入口 | ledger → 完整报告 |
| `render_report(report)` | fn | 文本渲染（系统 prompt 注入用） |

### 2.2 lib.rs 集成

```rust
pub mod intent_brier;
pub use intent_brier::{
    brier_score, compute_report, compute_trend, compute_window, domain_diagnostics,
    mean_brier, render_report, BrierTrend, BrierWindow, DomainDiagnostic, FeedbackOutcome,
    IntentDiagnosticReport, IntentLedger, IntentPrediction, IntentRecord, DEFAULT_WINDOWS,
    DEFAULT_LOW_CALIBRATION_THRESHOLD,
};
```

## 3. 验收对照

### 3.1 任务要求 vs 交付

| 验收项 | 要求 | 实际 |
|--------|------|------|
| 意图对账器（多场景） | 是 | ✅ 6 测试（record/feedback/容量淘汰/防重复/无记录报错） |
| Brier 计算（数值校准） | 是 | ✅ 6 测试（完美/最差/oracle 一致/clamp/对称/FeedbackOutcome 映射） |
| 滚动窗口 | 是 | ✅ 5 测试（窗口内取尾/窗口大于数据/0 窗口/跳过未反馈） |
| 诊断输出格式 | 是 | ✅ 4 测试（领域分组/无 domain 跳过/阈值可配/报告渲染含⚠标记） |
| 复用 oracle 不破坏 | 是 | ✅ 2 测试（oracle Brier 数值级验证 + IntentRecord↔Forecast 字段同构可挂接） |
| cargo test -p apeireth-companion --lib 全绿 | 是 | ✅ **607 passed; 0 failed** (576 基线 + 31 新增) |
| cargo check --workspace --all-targets 0 错 | 是 | ✅ 0 errors |
| 报告 reports/<taskId>-backend_engineer2-report.md | 是 | ✅ 本文件 |
| backlog.md W6 → ✅ | 是 | ✅ 已更新 |

### 3.2 关键设计决策（Ponytail 视角）

- **ladders 用得对**：复用 oracle 的 Brier 公式（`brier_score` 与 `Forecast::resolve` 数值级一致，已测试验证）；不复用 SqliteMemoryStore，IntentLedger 内存即可（0 IO 依赖，单进程内时间戳当 id 足够）
- **复用 oracle CalibratedResolver 接口形状**：`compute_report → IntentDiagnosticReport` 字段语义与 `CalibratedResolver::status → CalibrationStatus` 同构（mean_brier + resolved_count + 提示），方便上层统一注入 system prompt
- **f32→f64 精度修正**：Brier 是平方运算，f32 精度损失会导致 0.09 变 0.09000001 等偏差；confidence 改 f64 与公式对齐（数值级测试已通过）
- **领域诊断**：BTreeMap 分组 → 排序（mean_brier desc）→ 阈值标记，0 装默认阈值 0.25（业内 Brier benchmark 中位），可由 caller 覆盖
- **趋势判定**：短窗 30 vs 长窗 300，5% delta 阈值（常量 `TREND_DELTA_RATIO`），数据不足返 Stable（不假装判断）
- **0 LLM / 0 IO / 0 随机**：纯确定性函数 + 内存 ledger，可测可复现
- **API 0 改动**：oracle::Forecast / ForecastRegistry / CalibratedResolver 完全不动；IntentLedger 完全独立

### 3.3 与 oracle 衔接图

```
外部世界事件 (oracle.rs)         主人意图反馈 (intent_brier.rs)
        │                                  │
        ▼                                  ▼
   Forecast                          IntentRecord
   { prob, resolved, brier }        { prediction, outcome, domain }
        │                                  │
        ▼                                  ▼
   ForecastRegistry                  IntentLedger (内存)
   { SqliteMemoryStore }             { VecDeque, cap 1000 }
        │                                  │
        ▼                                  ▼
   CalibratedResolver               compute_report
   { BetaBinomial + Brier 均值 }    { 三窗口 + 趋势 + 领域 }
        │                                  │
        └──────────────┬───────────────────┘
                       ▼
              system prompt 注入 (同接口形状)
              (mean_brier + sample_count + hint)
```

两条道各自输入不同（外部事件 vs 主人反馈），但校准输出同构（同公式 + 同 status 字段），上层可统一消费。

## 4. 已知边界（0 装标注，备升级路径）

1. **Silent 默认按 hit 计**：保守假设（沉默 = 主人没纠正 = 猜对）。如需"沉默 = miss"，加 `IntentLedger::set_silent_policy` 方法即可
2. **领域标签当前是手工指定**：可在 assemble.rs 注入前从 W4 TopicPrediction.topics() 取（已设计衔接点）—— 一行 wiring
3. **Ledger 内存存储**：进程重启丢历史；如需持久化，可加 `to_forecast_registry` 转 `ForecastRegistry::register`（接口同构已验证）
4. **窗口档位 [30, 100, 300] 是常量**：A/B 后可改 `Vec<usize>` 注入
5. **低校准阈值 0.25 是业内中位**：实际场景需调（callers 通过 `compute_report` 参数覆盖）

## 5. 提交

- 新文件：`crates/apeireth-companion/src/intent_brier.rs`（约 800 行）
- 修改：`crates/apeireth-companion/src/lib.rs`（注册 + pub re-export 14 项）
- 修改：`docs/backlog.md`（W6 行 ✅）

— 后端工程师2 / W6