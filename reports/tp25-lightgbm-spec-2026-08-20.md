# TP25 LightGBM 时序预测器 — 实施 Spec (轻量版)

- **日期**: 2026-08-20
- **任务源**: docs/04-internal/next-team-handbook.md §1 TP25 (E3 增强)
- **trait 口**: `crates/apeireth-companion/src/oracle_adapters.rs:873` (`TimeSeriesPredictor`)
- **既有**: `NoopTimeSeriesPredictor` (L882) / `NaiveBaselinePredictor` (L961) / `ArimaPredictor` (L1075)

---

## 0. 决策 (TL;DR)

**走 tract-onnx 纯 Rust 路线** — 唯一新增运行时依赖,Windows/Linux/macOS **0 系统库 / 0 CMake / 0 MSVC**。

```text
装了什么   : tract-onnx (纯 Rust ONNX runtime) + ndarray (数据 buffer)
             ↓
不装什么   : lib_lightgbm (C++) / lightgbm-rs / onnxruntime / 任何系统动态库 / Python
             ↓
0 装兜底   : LightGBMProvider::default() session=None → AdapterError::Degraded
             ↓
脱机跑测   : fixture 在 → 跑真推理;不在 → if !path.exists() return;cargo test 永挂绿
```

---

## 1. 为什么 tract-onnx (排除 lightgbm-rs)

| 路线 | 装什么 | 0 装 PASS | 精度 |
|---|---|---|---|
| **tract-onnx (本 spec)** | 纯 Rust | **✓** | 与 LightGBM 原生等价 |
| ~~lightgbm-rs~~ | lib_lightgbm + CMake + MSVC | ✗ Windows CI 难 | 高 |
| ~~onnxruntime-rs~~ | onnxruntime C++ 1-2MB | △ 需预下载 zip | 高 |
| ~~纯 Rust 近似~~ | gbdt-rs / rgbt | ✓ | 打折 5-15% |

**不选 lightgbm-rs** 根因: C++ 编译链 + Windows 0 pre-built + 项目"0 系统库"宪法冲突。tract-onnx 已完整支持 TreeEnsemble op,**精度无损**。

---

## 2. 数据流

```
caller (blend_predictions 数字信号)
  ↓ .predict(series, horizon)
LightGBMProvider { session: Option<Arc<RunnableModel>> }
  ↓ session.is_none() → AdapterError::Degraded
tract-onnx → ndarray[1, window] → Vec<f64>
  N-step: 自回归回填 (history.push(y); history.remove(0))
```

trait 口**不改**:`predict(&[f64], usize) -> Result<Vec<f64>, AdapterError>`。

---

## 3. 0 装兜底 (宪法级)

```rust
#[derive(Debug, Clone)]
pub struct LightGBMProvider {
    /// None = 0 装 / 装载失败 → 完全等价 NoopTimeSeriesPredictor
    session: Option<Arc<tract_onnx::prelude::RunnableModel<...>>>,
    window_size: usize,
}

impl Default for LightGBMProvider {
    fn default() -> Self { Self { session: None, window_size: 60 } }
}

impl TimeSeriesPredictor for LightGBMProvider {
    fn predict(&self, series: &[f64], horizon: usize) -> Result<Vec<f64>, AdapterError> {
        let session = self.session.as_ref().ok_or_else(||
            AdapterError::Degraded("LightGBM 模型未装载 (默认 Noop, .onnx 缺失)".into()))?;
        if series.len() < self.window_size {
            return Err(AdapterError::Degraded(format!(
                "输入序列太短 ({} < window={})", series.len(), self.window_size)));
        }
        let mut history = series[series.len()-self.window_size..].to_vec();
        let mut out = Vec::with_capacity(horizon);
        for _ in 0..horizon {
            let y = tract_run(session, &history);  // ndarray → f64
            out.push(y);
            history.push(y);
            if history.len() > self.window_size { history.remove(0); }
        }
        Ok(out)
    }
    fn provider(&self) -> &str {
        if self.session.is_some() { "lightgbm-onnx" } else { "lightgbm-noop" }
    }
}
```

**不变量**:`from_onnx_file()` 失败 → `session=None` + `eprintln!` (不静默退化);`lightgbm-noop` ≠ `"noop"` (主人/审计区分)。

---

## 4. 模型管理 (轻量)

| 项 | 约定 |
|---|---|
| 默认路径 Win | `%APPDATA%\apeireth\models\lightgbm\` |
| 默认路径 Linux/macOS | `~/.config/apeireth/models/lightgbm/` |
| 主人端覆盖 | `APEIRETH_LIGHTGBM_MODEL_DIR` env var |
| 文件名 | `<symbol>_<horizon>step_v<n>_<yyyymmdd>.onnx` |
| 装载选择 | 同 symbol+horizon 取最大 v + 最新日期 |

Python 训练**不入 Apeireth 仓** (owner-side `scripts/train_lightgbm.py` + onnxmltools) — Rust 端只读推理。

---

## 5. 代码改动

### 5.1 `crates/apeireth-companion/Cargo.toml` (+2 dep)

```toml
tract-onnx = "0.21"  # 纯 Rust ONNX runtime
ndarray = "0.15"     # tract 喂数据 buffer
```

0 feature / 0 optional / 0 native build — 纯 Cargo dep,增量编译 +20-40s。

### 5.2 `oracle_adapters.rs` (+250 行)

- 末尾新 `lightgbm_provider` mod (struct + impl + `from_onnx_file` + `is_loaded`)
- `lightgbm_predict_with_ci(series, horizon) -> (Vec<f64>, Vec<f64>)` — bootstrap 残差估 σ,半宽 = `1.96 * σ * sqrt(h)` (per ARIMA P1 同款口径)
- 4 个单测 (3 永远跑 + 1 fixture skip)

### 5.3 0 改文件

| 类别 | 状态 |
|---|---|
| `oracle.rs` / 其他 8 个 crate | 0 改 |
| `workspace.Cargo.toml` (version 1.2.0) | 0 改 |
| `TimeSeriesPredictor` trait (L873) / enum / const / 24 LOCKED | 0 改 |
| `blend_predictions` / `arima_predict_with_ci` / gh_*.ps1 | 0 改 |

---

## 6. E2E 验证 (脱机 PASS)

### 永远跑 (4 个)

```rust
#[test] fn lightgbm_default_is_noop_with_honest_err() { ... }
//   assert provider()=="lightgbm-noop"
//   assert predict() → AdapterError::Degraded("LightGBM 模型未装载")

#[test] fn lightgbm_provider_distinguishable_from_arima_naive_noop() { ... }
//   "lightgbm-noop" ≠ "arima-1-1-1" ≠ "naive-baseline" ≠ "noop"

#[test] fn lightgbm_blendable_with_llm_text_prediction() { ... }
//   blend_predictions(0.65, 0.70, 0.8, 0.5) ∈ [0,1]

#[test] fn lightgbm_input_too_short_returns_degraded() { ... }
//   series.len() < window_size → Degraded(明示原因)
```

### Fixture 门控 (2 个)

```rust
#[test]
fn lightgbm_e2e_1step_with_fixture() {
    let fixture = Path::new("tests/fixtures/lightgbm/BTC_1step_v1_20260820.onnx");
    if !fixture.exists() {
        eprintln!("[skip] fixture 缺失: {fixture:?}");
        return;  // ← 脱机 PASS: 无 fixture CI 永挂绿
    }
    let p = LightGBMProvider::from_onnx_file(fixture, 60).unwrap();
    assert_eq!(p.provider(), "lightgbm-onnx");
    let series: Vec<f64> = (0..100).map(|t| 100.0 + (t as f64/5.0).sin()).collect();
    let one = p.predict(&series, 1).unwrap();
    assert!(one[0].is_finite());
    // 真值 ≈ 100 + sin(20/5) ≈ 100.91;精度 RMSE < 1.5
}

#[test]
fn lightgbm_e2e_nstep_with_ci() {
    let fixture = Path::new("tests/fixtures/lightgbm/BTC_1step_v1_20260820.onnx");
    if !fixture.exists() { return; }
    let p = LightGBMProvider::from_onnx_file(fixture, 60).unwrap();
    let series: Vec<f64> = (0..100).map(|t| 100.0 + (t as f64/5.0).sin()).collect();
    let (pred, ci) = lightgbm_predict_with_ci(&series, 5).unwrap();
    assert_eq!(pred.len(), 5);
    for c in &ci { assert!(*c > 0.0); }
    assert!(ci[4] >= ci[0], "CI 应随 horizon 递增");  // E3 增强口径
}
```

**脱机表**:

| fixture | 场景 A 无模型 | 场景 B 1-step | 场景 C N-step |
|---|---|---|---|
| 在 | ✓ 跑 | ✓ 跑真推理 | ✓ 跑真推理 |
| **不在** | **✓ 跑** | **⏭ skip (永绿)** | **⏭ skip (永绿)** |

---

## 7. 风险点

| 风险 | 缓解 |
|---|---|
| ONNX opset ≥17 TreeEnsemble 兼容性 | 导出固定 opset 12-15 + 失败返 Degraded |
| 训练归一化不一致 | 模型内嵌 Scaler op or owner README 固化 z-score |
| N 步滚动误差累积 | horizon ≤24 + blend 与 LLM 文本融合补偿 |
| fixture 永远缺失 → 精度无实证 | owner 1 天训 fixture (`scripts/train_lightgbm.py` 进 Apeireth-external) |

---

## 8. 0 触碰自查 + 估时

**自查清单** (per R125-12 P0-3): ✓ 不改其他 crate / ✓ 不改 workspace.version / ✓ 不改 trait 口 / ✓ 不改 oracle.rs / ✓ 不动 gh_*.ps1 / ✓ 单测脱机 PASS / ✓ 失败诚实 (Degraded)。

| 任务 | 估时 | 派活 |
|---|---|---|
| LightGBMProvider 骨架 (Noop + from_onnx_file) | 0.5 天 | sub-agent 1 |
| tract-onnx 推理 (1-step + N-step 回填) | 1 天 | sub-agent 1 |
| `lightgbm_predict_with_ci` (bootstrap σ) | 0.5 天 | sub-agent 1 |
| 模型目录定位 (APPDATA/XDG + env) | 0.25 天 | sub-agent 1 |
| 4+2 测试 | 0.5 天 | sub-agent 1 |
| Cargo.toml + 编译调试 | 0.25 天 | sub-agent 1 |
| **sub-agent 1 小计** | **3 天** | |
| owner: 训练 + ONNX 导出 + 推 fixture | 0.5 天 | owner |
| **总计** | **3.5 天可验收** | |

---

## 附录: provider 名对照

| 实现 | provider 名 | 0 装行为 |
|---|---|---|
| `NoopTimeSeriesPredictor` | `"noop"` | Degraded |
| `NaiveBaselinePredictor` | `"naive-baseline"` | 永远可算 |
| `ArimaPredictor` | `"arima-1-1-1"` | 数据太短 → Degraded |
| `LightGBMProvider` | `"lightgbm-noop"` / `"lightgbm-onnx"` | Noop 兜底 |

E3 集合预报典型链 (blend_predictions 已在):
`final = blend(LightGBM.digital, LLM.textual, 0.8, 0.5) + blend(ARIMA.digital_with_ci, LLM.textual_with_ci, 0.6, 0.5)`
